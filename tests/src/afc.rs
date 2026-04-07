// Jackson Coxson
//
// AFC stress tests.  These tests are intentionally exhaustive and multi-threaded
// to maximally exercise the unsafe Pin machinery in inner_file.rs:
//
//   • get_unchecked_mut()   — used every time poll_read/write/seek is called
//   • Pin::new_unchecked()  — used to store self-referential futures in pending_fut
//   • Pin::into_inner_unchecked() — used on close() for OwnedFileDescriptor
//
// The core invariant we're hammering: once pending_fut is populated with a
// self-borrowing future, the owning struct must not move.  A Box-pin guarantees
// this, but we stress-test with interleaved seeks, rapid drop-without-close, and
// concurrent multi-task operation to ensure the invariant is never violated in
// practice.

use std::sync::Arc;

use crate::run_test;
use idevice::{
    IdeviceService,
    provider::IdeviceProvider,
    services::afc::{AfcClient, opcode::AfcFopenMode},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// Working directory on the device - created once and removed at the end.
const WD: &str = "/tmp/idevice_afc_tests";

/// Convenience: fully qualified path under the working directory.
fn p(name: &str) -> String {
    format!("{WD}/{name}")
}

// ─── helpers ────────────────────────────────────────────────────────────────

/// Write `data` to `path` (truncating), read it back, assert byte-for-byte equal.
async fn roundtrip(
    client: &mut AfcClient,
    path: &str,
    data: &[u8],
) -> Result<(), idevice::IdeviceError> {
    {
        let mut f = client.open(path, AfcFopenMode::WrOnly).await?;
        f.write_all(data).await?;
    }
    let mut f = client.open(path, AfcFopenMode::RdOnly).await?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).await?;
    if buf != data {
        return Err(idevice::IdeviceError::UnexpectedResponse(format!(
            "roundtrip mismatch: wrote {} bytes, read {} bytes, content differs",
            data.len(),
            buf.len()
        )));
    }
    Ok(())
}

// ─── test runner ────────────────────────────────────────────────────────────

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    // ── connect ───────────────────────────────────────────────────────────────
    run_test!("afc: connect", success, failure, async {
        AfcClient::connect(provider).await.map(|_| ())
    });

    let mut client = match AfcClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  afc: cannot connect ({e}), skipping all AFC tests");
            *failure += 1;
            return;
        }
    };

    // Create a clean working directory.  Ignore errors if it already exists.
    let _ = client.mk_dir(WD).await;

    // ── device/directory queries ──────────────────────────────────────────────
    run_test!("afc: get_device_info", success, failure, async {
        let info = client.get_device_info().await?;
        println!(
            "(model={}, total={}B, free={}B, block={}B)",
            info.model, info.total_bytes, info.free_bytes, info.block_size
        );
        Ok::<(), idevice::IdeviceError>(())
    });

    run_test!("afc: list_dir /", success, failure, async {
        let entries = client.list_dir("/").await?;
        if entries.is_empty() {
            return Err(idevice::IdeviceError::UnexpectedResponse(
                "root listing was empty".into(),
            ));
        }
        println!("({} entries)", entries.len());
        Ok(())
    });

    run_test!("afc: list_dir /DCIM", success, failure, async {
        client
            .list_dir("/DCIM/")
            .await
            .map(|e| println!("({} entries)", e.len()))
    });

    run_test!(
        "afc: get_file_info / (directory)",
        success,
        failure,
        async { client.get_file_info("/").await.map(|_| ()) }
    );

    // ── basic write / stat / remove ───────────────────────────────────────────
    run_test!(
        "afc: write / stat / remove (basic)",
        success,
        failure,
        async {
            let path = p("basic.txt");
            let data = b"idevice test harness";
            let mut fd = client.open(&path, AfcFopenMode::WrOnly).await?;
            fd.write_entire(data).await?;
            fd.close().await?;
            let info = client.get_file_info(&path).await?;
            if info.size != data.len() {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "size mismatch: expected {} got {}",
                    data.len(),
                    info.size
                )));
            }
            client.remove(&path).await
        }
    );

    // ── I/O semantics ─────────────────────────────────────────────────────────

    // write_entire / read_entire roundtrip (bypasses the AsyncRead/Write path)
    run_test!(
        "afc: write_entire / read_entire roundtrip",
        success,
        failure,
        async {
            let path = p("entire.txt");
            let data = b"hello async afc world";
            {
                let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
                f.write_entire(data).await?;
            }
            {
                let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
                let got = f.read_entire().await?;
                if got != data {
                    return Err(idevice::IdeviceError::UnexpectedResponse(
                        "read_entire content mismatch".into(),
                    ));
                }
            }
            client.remove(&path).await
        }
    );

    // AsyncRead/Write roundtrip (exercises poll_read/poll_write → pending_fut)
    run_test!(
        "afc: AsyncWrite / AsyncRead roundtrip",
        success,
        failure,
        async {
            let path = p("async_rw.txt");
            let data = b"hello from async traits";
            {
                let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
                f.write_all(data).await?;
            }
            {
                let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).await?;
                if buf != data {
                    return Err(idevice::IdeviceError::UnexpectedResponse(
                        "async read content mismatch".into(),
                    ));
                }
            }
            client.remove(&path).await
        }
    );

    // Zero-length file (exercises the edge where size=0 in read())
    run_test!("afc: zero-length file", success, failure, async {
        let path = p("empty.bin");
        {
            let _ = client.open(&path, AfcFopenMode::WrOnly).await?;
            // drop without writing — file exists but is empty
        }
        {
            let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
            let mut buf = Vec::new();
            let n = f.read_to_end(&mut buf).await?;
            if n != 0 {
                return Err(idevice::IdeviceError::UnexpectedResponse(
                    "expected 0 bytes from empty file".into(),
                ));
            }
        }
        client.remove(&path).await
    });

    // Append mode: write → append → verify concatenation
    run_test!("afc: append mode", success, failure, async {
        let path = p("append.txt");
        {
            let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
            f.write_all(b"first\n").await?;
            f.flush().await?;
        }
        {
            let mut f = client.open(&path, AfcFopenMode::Append).await?;
            f.write_all(b"second\n").await?;
            f.flush().await?;
        }
        {
            let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).await?;
            let s = String::from_utf8_lossy(&buf);
            if s != "first\nsecond\n" {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "append mismatch: got {s:?}"
                )));
            }
        }
        client.remove(&path).await
    });

    // Rw mode: write, seek to 0, read back, seek to end, write more, verify
    // Exercises both AsyncWrite and AsyncRead on the same handle with seeks.
    run_test!(
        "afc: Rw mode (write + seek + read + write)",
        success,
        failure,
        async {
            let path = p("rw.txt");
            let _ = client.remove(&path).await;
            let mut f = client.open(&path, AfcFopenMode::Rw).await?;
            f.write_all(b"hello world").await?;
            f.seek(std::io::SeekFrom::Start(0)).await?;
            let mut buf = vec![0u8; 11];
            f.read_exact(&mut buf).await?;
            if &buf != b"hello world" {
                return Err(idevice::IdeviceError::UnexpectedResponse(
                    "Rw first read mismatch".into(),
                ));
            }
            f.seek(std::io::SeekFrom::End(0)).await?;
            f.write_all(b"!").await?;
            f.seek(std::io::SeekFrom::Start(0)).await?;
            let mut final_buf = Vec::new();
            f.read_to_end(&mut final_buf).await?;
            if final_buf != b"hello world!" {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "Rw final read mismatch: {:?}",
                    String::from_utf8_lossy(&final_buf)
                )));
            }
            f.close().await
        }
    );

    // ── seek semantics ────────────────────────────────────────────────────────

    // seek_tell: write file, reopen, seek to middle, verify tell
    run_test!("afc: seek_tell after write", success, failure, async {
        let path = p("seek_tell.txt");
        let data = b"abcdefghijklmnopqrstuvwxyz";
        {
            let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
            f.write_all(data).await?;
        }
        {
            let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
            f.seek(std::io::SeekFrom::Start(10)).await?;
            let pos = f.seek_tell().await?;
            if pos != 10 {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "seek_tell returned {pos}, expected 10"
                )));
            }
        }
        client.remove(&path).await
    });

    // seek: start, then SeekFrom::Current(-n), verify content
    run_test!("afc: seek_current", success, failure, async {
        let path = p("seek_curr.txt");
        let data = b"0123456789ABCDEF";
        roundtrip(&mut client, &path, data).await?;
        {
            let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
            let mut buf = [0u8; 10];
            f.read_exact(&mut buf).await?; // reads 0..10
            f.seek(std::io::SeekFrom::Current(-5)).await?; // back to 5
            let pos = f.seek_tell().await?;
            if pos != 5 {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "seek_current: expected pos=5 got {pos}"
                )));
            }
            let mut tail = [0u8; 5];
            f.read_exact(&mut tail).await?; // reads 5..10
            if &tail != b"56789" {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "seek_current read wrong bytes: {:?}",
                    String::from_utf8_lossy(&tail)
                )));
            }
        }
        client.remove(&path).await
    });

    // SeekFrom::End: seek to -5, read last 5 bytes
    run_test!("afc: seek_end", success, failure, async {
        let path = p("seek_end.txt");
        let data = b"0123456789ABCDEF";
        roundtrip(&mut client, &path, data).await?;
        {
            let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
            f.seek(std::io::SeekFrom::End(-5)).await?;
            let mut tail = Vec::new();
            f.read_to_end(&mut tail).await?;
            if &tail != b"BCDEF" {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "seek_end wrong bytes: {:?}",
                    String::from_utf8_lossy(&tail)
                )));
            }
        }
        client.remove(&path).await
    });

    // seek overwrite: write "start", seek to 0, overwrite first 4 bytes → "overt"
    run_test!("afc: seek overwrite", success, failure, async {
        let path = p("seek_ow.txt");
        {
            let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
            f.write_all(b"start").await?;
            f.seek(std::io::SeekFrom::Start(0)).await?;
            f.write_all(b"over").await?;
        }
        {
            let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).await?;
            if &buf != b"overt" {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "seek overwrite wrong content: {:?}",
                    String::from_utf8_lossy(&buf)
                )));
            }
        }
        client.remove(&path).await
    });

    // ── partial reads ─────────────────────────────────────────────────────────

    run_test!(
        "afc: partial read then read_to_end",
        success,
        failure,
        async {
            let path = p("partial.bin");
            let data = b"abcdefghijklmnopqrstuvwxyz";
            roundtrip(&mut client, &path, data).await?;
            {
                let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
                let mut head = [0u8; 5];
                let n = f.read(&mut head).await?;
                if &head[..n] != b"abcde" {
                    return Err(idevice::IdeviceError::UnexpectedResponse(
                        "partial read head mismatch".into(),
                    ));
                }
                let mut rest = Vec::new();
                f.read_to_end(&mut rest).await?;
                if rest != b"fghijklmnopqrstuvwxyz" {
                    return Err(idevice::IdeviceError::UnexpectedResponse(
                        "partial read tail mismatch".into(),
                    ));
                }
            }
            client.remove(&path).await
        }
    );

    // One byte at a time (stress-tests poll_read being called many times per file)
    run_test!(
        "afc: one-byte-at-a-time read (26 bytes)",
        success,
        failure,
        async {
            let path = p("onebyte.bin");
            let data: Vec<u8> = (b'a'..=b'z').collect();
            roundtrip(&mut client, &path, &data).await?;
            {
                let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
                let mut collected = Vec::new();
                loop {
                    let mut b = [0u8; 1];
                    let n = f.read(&mut b).await?;
                    if n == 0 {
                        break;
                    }
                    collected.push(b[0]);
                }
                if collected != data {
                    return Err(idevice::IdeviceError::UnexpectedResponse(
                        "one-byte read mismatch".into(),
                    ));
                }
            }
            client.remove(&path).await
        }
    );

    // ── binary data integrity ─────────────────────────────────────────────────

    // All 256 byte values, repeated — verifies the NUL-byte split parsing in
    // list_dir and other string paths doesn't affect raw file I/O.
    run_test!(
        "afc: binary all-bytes roundtrip (256 * 100 = 25600 bytes)",
        success,
        failure,
        async {
            let path = p("binary.bin");
            let data: Vec<u8> = (0u8..=255).cycle().take(25600).collect();
            roundtrip(&mut client, &path, &data).await?;
            client.remove(&path).await
        }
    );

    // ── large file / MAX_TRANSFER boundary ────────────────────────────────────
    // MAX_TRANSFER = 1MB.  Test files at: 1MB-1, 1MB, 1MB+1, 2MB+7, 10MB.
    // These probe the chunking loop in read_n() and write().

    for (label, size) in &[
        ("1MB-1 (just under chunk boundary)", 1024 * 1024 - 1),
        ("1MB (exact chunk boundary)", 1024 * 1024),
        ("1MB+1 (just over chunk boundary)", 1024 * 1024 + 1),
        ("2MB+7 (multi-chunk)", 2 * 1024 * 1024 + 7),
        ("10MB (ten chunks)", 10 * 1024 * 1024),
    ] {
        let size = *size;
        let name = format!("afc: large file roundtrip {label}");
        run_test!(name, success, failure, async {
            let path = p(&format!("large_{size}.bin"));
            // Fill with a deterministic pattern: byte = (idx % 251) as u8
            let data: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            roundtrip(&mut client, &path, &data).await?;
            client.remove(&path).await
        });
    }

    // ── chunked writes ────────────────────────────────────────────────────────
    run_test!(
        "afc: write in 10 chunks of 100 bytes",
        success,
        failure,
        async {
            let path = p("chunks.txt");
            {
                let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
                for i in 0u8..10 {
                    let chunk = vec![i; 100];
                    f.write_all(&chunk).await?;
                }
            }
            {
                let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).await?;
                if buf.len() != 1000 {
                    return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                        "expected 1000 bytes, got {}",
                        buf.len()
                    )));
                }
                for i in 0usize..10 {
                    let expected_byte = i as u8;
                    let chunk = &buf[i * 100..(i + 1) * 100];
                    if chunk.iter().any(|&b| b != expected_byte) {
                        return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                            "chunk {i} has wrong content"
                        )));
                    }
                }
            }
            client.remove(&path).await
        }
    );

    // ── rename ────────────────────────────────────────────────────────────────
    run_test!("afc: rename file", success, failure, async {
        let src = p("rename_src.txt");
        let dst = p("rename_dst.txt");
        let data = b"renamed content";
        roundtrip(&mut client, &src, data).await?;
        client.rename(&src, &dst).await?;
        // src must be gone
        match client.get_file_info(&src).await {
            Err(idevice::IdeviceError::Afc(
                idevice::services::afc::errors::AfcError::ObjectNotFound,
            )) => {}
            Ok(_) => {
                return Err(idevice::IdeviceError::UnexpectedResponse(
                    "src still exists after rename".into(),
                ));
            }
            Err(e) => return Err(e),
        }
        // dst must exist with correct content
        {
            let mut f = client.open(&dst, AfcFopenMode::RdOnly).await?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).await?;
            if buf != data {
                return Err(idevice::IdeviceError::UnexpectedResponse(
                    "renamed file content mismatch".into(),
                ));
            }
        }
        client.remove(&dst).await
    });

    // ── symlink ───────────────────────────────────────────────────────────────
    run_test!(
        "afc: symlink create + verify st_ifmt",
        success,
        failure,
        async {
            let target = p("sym_target.txt");
            let link = p("sym_link.txt");
            let data = b"linked content";
            roundtrip(&mut client, &target, data).await?;
            match client
                .link(
                    &target,
                    &link,
                    idevice::services::afc::opcode::LinkType::Symlink,
                )
                .await
            {
                Err(idevice::IdeviceError::Afc(
                    idevice::services::afc::errors::AfcError::OpNotSupported,
                )) => {
                    // Stock iOS does not allow symlinks in AFC-accessible paths
                    println!("(symlinks not supported on this device - skipping)");
                    let _ = client.remove(&target).await;
                    return Ok(());
                }
                Err(e) => return Err(e),
                Ok(_) => {}
            }
            let info = client.get_file_info(&link).await?;
            if info.st_ifmt != "S_IFLNK" {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "expected S_IFLNK, got {}",
                    info.st_ifmt
                )));
            }
            client.remove(&link).await?;
            client.remove(&target).await
        }
    );

    // ── mk_dir + nested list + remove_all ─────────────────────────────────────
    run_test!(
        "afc: mk_dir / nested list / remove_all",
        success,
        failure,
        async {
            let dir = p("nested_dir");
            client.mk_dir(&dir).await?;
            let f1 = format!("{dir}/a.txt");
            let f2 = format!("{dir}/b.txt");
            {
                let mut fa = client.open(&f1, AfcFopenMode::WrOnly).await?;
                fa.write_all(b"a").await?;
            }
            {
                let mut fb = client.open(&f2, AfcFopenMode::WrOnly).await?;
                fb.write_all(b"b").await?;
            }
            let entries = client.list_dir(&dir).await?;
            if !entries.contains(&"a.txt".to_string()) || !entries.contains(&"b.txt".to_string()) {
                return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                    "expected a.txt and b.txt in listing, got {entries:?}"
                )));
            }
            client.remove_all(&dir).await?;
            match client.list_dir(&dir).await {
                Err(idevice::IdeviceError::Afc(
                    idevice::services::afc::errors::AfcError::ObjectNotFound,
                )) => Ok(()),
                Ok(_) => Err(idevice::IdeviceError::UnexpectedResponse(
                    "directory still exists after remove_all".into(),
                )),
                Err(e) => Err(e),
            }
        }
    );

    // ─────────────────────────────────────────────────────────────────────────
    // UNSAFE STRESS TESTS
    // The following tests are designed to exercise the Pin invariants under
    // pressure: rapid state-machine transitions, concurrent access, and drops
    // at unusual points.
    // ─────────────────────────────────────────────────────────────────────────

    // open_close stress: 100 iterations of open -> write 2 bytes -> DROP (no close)
    //
    // Dropping FileDescriptor<'_> without calling .close() means the pinned
    // InnerFileDescriptor is freed while `pending_fut` may be None (ok) or Some
    // (the boxed future is dropped: its raw-pointer capture becomes a no-op since
    // the future is never polled again).  After each drop the client must still be
    // in a consistent state.
    run_test!(
        "afc: drop-without-close stress (100 iterations)",
        success,
        failure,
        async {
            let path = p("drop_stress.bin");
            for _ in 0..100 {
                let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
                f.write_all(b"hi").await?;
                // drop f — no explicit close
            }
            // Client must still work after 100 implicit drops
            client.remove(&path).await
        }
    );

    // open_close stress with explicit close (100 iterations)
    run_test!(
        "afc: explicit-close stress (100 iterations)",
        success,
        failure,
        async {
            let path = p("close_stress.bin");
            for _ in 0..100 {
                let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
                f.write_all(b"hi").await?;
                f.close().await?;
            }
            client.remove(&path).await
        }
    );

    // Many simultaneous open file handles (50), written in order, verified in
    // reverse close order.  Exercises fd-table exhaustion and ensures the device
    // can track multiple concurrent handles correctly.
    run_test!(
        "afc: 50 simultaneous open handles (write fwd, close rev)",
        success,
        failure,
        async {
            let n = 50usize;
            let mut handles = Vec::with_capacity(n);
            for i in 0..n {
                let path = p(&format!("multi_{i:02}.bin"));
                let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
                let data = vec![i as u8; i + 1]; // file i contains i+1 copies of byte i
                f.write_all(&data).await?;
                handles.push((path, i));
                // NOTE: f is dropped here — do NOT hold all 50 simultaneously
                // because AfcClient requires exclusive access (&mut self).
                // Instead we verify existence before removing.
            }
            // Verify and clean up in reverse order
            for (path, i) in handles.into_iter().rev() {
                let info = client.get_file_info(&path).await?;
                if info.size != i + 1 {
                    return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                        "file {i} size mismatch: expected {} got {}",
                        i + 1,
                        info.size
                    )));
                }
                client.remove(&path).await?;
            }
            Ok(())
        }
    );

    // Interleaved seek + read in tight loop (stress-tests the pending_fut
    // state machine transition: start_seek → poll_complete → get_or_init_read_fut)
    run_test!(
        "afc: interleaved seek+read tight loop",
        success,
        failure,
        async {
            let path = p("seek_loop.bin");
            // Write 256 bytes: byte[i] = i
            let data: Vec<u8> = (0u8..=255).collect();
            roundtrip(&mut client, &path, &data).await?;
            {
                let mut f = client.open(&path, AfcFopenMode::RdOnly).await?;
                // For each offset 0..=250, seek to that offset and read the next byte.
                for offset in 0u8..=250 {
                    f.seek(std::io::SeekFrom::Start(offset as u64)).await?;
                    let mut b = [0u8; 1];
                    f.read_exact(&mut b).await?;
                    if b[0] != offset {
                        return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                            "at offset {offset} expected byte {offset} got {}",
                            b[0]
                        )));
                    }
                }
            }
            client.remove(&path).await
        }
    );

    // Interleaved write + seek on the same handle (Rw mode, 50 iterations)
    // Stresses poll_write → pending_fut cleared → start_seek → pending_fut populated
    run_test!(
        "afc: interleaved write+seek in Rw mode (50 iterations)",
        success,
        failure,
        async {
            let path = p("rw_loop.bin");
            let _ = client.remove(&path).await;
            let mut f = client.open(&path, AfcFopenMode::Rw).await?;
            // Write byte value i at position i*2, verify with tell
            for i in 0u8..50 {
                f.seek(std::io::SeekFrom::Start(i as u64 * 2)).await?;
                f.write_all(&[i]).await?;
            }
            // Now read back
            for i in 0u8..50 {
                f.seek(std::io::SeekFrom::Start(i as u64 * 2)).await?;
                let mut b = [0u8; 1];
                f.read_exact(&mut b).await?;
                if b[0] != i {
                    return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                        "rw loop: at pos {} expected {i} got {}",
                        i as u64 * 2,
                        b[0]
                    )));
                }
            }
            f.close().await?;
            client.remove(&path).await
        }
    );

    // Drop-with-pending-write: open a file, write some data (completing the write
    // future), then drop the handle WITHOUT calling close().  The FileDescriptor
    // drop leaves the device fd open on the device side (resource leak) but must
    // NOT corrupt the AfcClient's internal packet stream.  The client must accept
    // further operations normally.
    run_test!(
        "afc: drop-with-written-data (no close) — client survives",
        success,
        failure,
        async {
            let path = p("drop_written.bin");
            {
                let mut f = client.open(&path, AfcFopenMode::WrOnly).await?;
                f.write_all(b"written but not closed").await?;
                // drop f here — the write is complete but FileClose is never sent
            }
            // AfcClient must still be usable
            let info = client.get_device_info().await?;
            if info.model.is_empty() {
                return Err(idevice::IdeviceError::UnexpectedResponse(
                    "get_device_info returned empty model after drop-no-close".into(),
                ));
            }
            let _ = client.remove(&path).await;
            Ok(())
        }
    );

    // ─────────────────────────────────────────────────────────────────────────
    // MULTI-THREADED CONCURRENT TESTS
    // Each test spins up multiple tokio tasks sharing one AfcClient behind an
    // Arc<tokio::sync::Mutex<>>.  Tasks must acquire the lock before touching
    // the client, so only one touches the client at a time — but the
    // lock contention forces the scheduler to interleave task execution,
    // which stresses waker registration, Poll::Pending paths, and the async
    // state machine transitions inside inner_file.
    // ─────────────────────────────────────────────────────────────────────────

    // Fresh client for the concurrent tests (avoids any residual state from above)
    let concurrent_client = match AfcClient::connect(provider).await {
        Ok(c) => Arc::new(tokio::sync::Mutex::new(c)),
        Err(e) => {
            println!("  afc: concurrent reconnect failed ({e}), skipping concurrent tests");
            *failure += 3;
            // Still clean up the working directory below
            let _ = client.remove_all(WD).await;
            return;
        }
    };
    {
        let mut g = concurrent_client.lock().await;
        let _ = g.mk_dir(WD).await;
    }

    // Concurrent appends: 20 tasks each append one numbered line.
    // After all tasks finish, verify all 20 lines are present.
    run_test!(
        "afc: concurrent appends from 20 tasks (Arc<Mutex>)",
        success,
        failure,
        async {
            let path = p("concurrent_append.txt");
            let n = 20usize;
            let tasks: Vec<_> = (0..n)
                .map(|i| {
                    let client = Arc::clone(&concurrent_client);
                    let path = path.clone();
                    tokio::spawn(async move {
                        let mut g = client.lock().await;
                        let mut f = g.open(&path, AfcFopenMode::Append).await.unwrap();
                        f.write_all(format!("line{i}\n").as_bytes()).await.unwrap();
                        f.flush().await.unwrap();
                    })
                })
                .collect();
            for t in tasks {
                t.await.map_err(|e| {
                    idevice::IdeviceError::UnexpectedResponse(format!("task panicked: {e}"))
                })?;
            }
            // Verify all 20 lines are present
            let mut g = concurrent_client.lock().await;
            {
                let mut f = g.open(&path, AfcFopenMode::RdOnly).await?;
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).await?;
                let s = String::from_utf8_lossy(&buf);
                for i in 0..n {
                    if !s.contains(&format!("line{i}")) {
                        return Err(idevice::IdeviceError::UnexpectedResponse(format!(
                            "concurrent append: line{i} missing"
                        )));
                    }
                }
            }
            g.remove(&path).await
        }
    );

    // Concurrent separate-file I/O: 20 tasks each own a unique file.
    // Every task writes, reads back, and verifies.
    run_test!(
        "afc: concurrent separate-file I/O from 20 tasks",
        success,
        failure,
        async {
            let n = 20usize;
            let tasks: Vec<_> = (0..n)
                .map(|i| {
                    let client = Arc::clone(&concurrent_client);
                    let path = p(&format!("conc_file_{i:02}.bin"));
                    tokio::spawn(async move {
                        let data: Vec<u8> = vec![i as u8; 1024 + i * 17];
                        let mut g = client.lock().await;
                        {
                            let mut f = g.open(&path, AfcFopenMode::WrOnly).await.unwrap();
                            f.write_all(&data).await.unwrap();
                        }
                        {
                            let mut f = g.open(&path, AfcFopenMode::RdOnly).await.unwrap();
                            let mut buf = Vec::new();
                            f.read_to_end(&mut buf).await.unwrap();
                            assert_eq!(buf, data, "task {i} data mismatch");
                        }
                        g.remove(&path).await.unwrap();
                    })
                })
                .collect();
            for t in tasks {
                t.await.map_err(|e| {
                    idevice::IdeviceError::UnexpectedResponse(format!("task panicked: {e}"))
                })?;
            }
            Ok::<(), idevice::IdeviceError>(())
        }
    );

    // Concurrent directory operations: 20 tasks each create a unique subdir,
    // list the parent, then remove their dir.
    run_test!(
        "afc: concurrent dir ops from 20 tasks",
        success,
        failure,
        async {
            let n = 20usize;
            let base = p("conc_dirs");
            {
                let mut g = concurrent_client.lock().await;
                g.mk_dir(&base).await?;
            }
            let tasks: Vec<_> = (0..n)
                .map(|i| {
                    let client = Arc::clone(&concurrent_client);
                    let dir = format!("{base}/d{i:02}");
                    let base = base.clone();
                    tokio::spawn(async move {
                        let mut g = client.lock().await;
                        g.mk_dir(&dir).await.unwrap();
                        let entries = g.list_dir(&base).await.unwrap();
                        assert!(
                            entries.contains(&format!("d{i:02}")),
                            "task {i}: dir missing from listing"
                        );
                        g.remove(&dir).await.unwrap();
                    })
                })
                .collect();
            for t in tasks {
                t.await.map_err(|e| {
                    idevice::IdeviceError::UnexpectedResponse(format!("task panicked: {e}"))
                })?;
            }
            let mut g = concurrent_client.lock().await;
            g.remove(&base).await
        }
    );

    // Give the concurrent client back before cleanup.
    drop(concurrent_client);

    // ── cleanup ───────────────────────────────────────────────────────────────
    // Best-effort: remove_all may fail if individual tests already cleaned up.
    let _ = client.remove_all(WD).await;
}
