#![allow(dead_code)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::paths::BackupPath;
use super::{BackupDelegate, DirEntryInfo};
use crate::IdeviceError;

/// Staged replacement for one backup file.
///
/// MobileBackup2 streams file data first and sends a success/error trailer
/// afterwards. The store API models that lifecycle explicitly: callers write
/// bytes into the replacement, then either `finish()` after a success trailer or
/// `abort()` after an error/cancel.
#[async_trait]
pub(crate) trait BackupFileReplacement: Write + Send {
    /// Install the staged file into the visible backup tree.
    async fn finish(self: Box<Self>) -> Result<(), IdeviceError>;

    /// Discard the staged file without installing it.
    async fn abort(self: Box<Self>) -> Result<(), IdeviceError>;
}

/// Host-side storage API used by the MobileBackup2 protocol loop.
///
/// This is intentionally higher level than the legacy `BackupDelegate`: paths
/// are validated `BackupPath`s, and file writes use a replacement lifecycle
/// instead of exposing direct final-path truncation to protocol handlers.
#[async_trait]
pub(crate) trait BackupStore: Send {
    /// Returns the available disk space in bytes for the backing storage.
    fn get_free_disk_space(&self) -> u64;

    /// Open an existing backup file for reading.
    async fn open_file_read(&self, path: &BackupPath)
    -> Result<Box<dyn Read + Send>, IdeviceError>;

    /// Begin replacing a visible backup file.
    ///
    /// Implementations may write directly to the final path or stage the data
    /// elsewhere. Transactional implementations must not mutate the visible path
    /// until `BackupFileReplacement::finish()`.
    async fn begin_replace(
        &mut self,
        path: &BackupPath,
    ) -> Result<Box<dyn BackupFileReplacement + Send>, IdeviceError>;

    async fn create_dir_all(&mut self, path: &BackupPath) -> Result<(), IdeviceError>;
    async fn remove(&mut self, path: &BackupPath) -> Result<(), IdeviceError>;
    async fn rename(&mut self, from: &BackupPath, to: &BackupPath) -> Result<(), IdeviceError>;
    async fn copy(&mut self, src: &BackupPath, dst: &BackupPath) -> Result<(), IdeviceError>;
    async fn exists(&self, path: &BackupPath) -> bool;
    async fn is_dir(&self, path: &BackupPath) -> bool;
    async fn list_dir(&self, path: &BackupPath) -> Result<Vec<DirEntryInfo>, IdeviceError>;

    fn on_file_received(&self, _path: &BackupPath, _file_count: u32) {}
    fn on_progress(&self, _bytes_done: u64, _bytes_total: u64, _overall_progress: f64) {}
}

/// Compatibility adapter for the existing delegate API.
///
/// This keeps current public/FFI callers compiling while moving the protocol
/// loop to `BackupStore`. New transactional storage should implement
/// `BackupStore` directly instead of going through this adapter.
pub(crate) struct DelegateBackupStore<'a> {
    root: PathBuf,
    delegate: &'a dyn BackupDelegate,
}

impl<'a> DelegateBackupStore<'a> {
    pub(crate) fn new(root: &Path, delegate: &'a dyn BackupDelegate) -> Self {
        Self {
            root: root.to_path_buf(),
            delegate,
        }
    }

    fn full_path(&self, path: &BackupPath) -> PathBuf {
        path.join_to_backup_dir(&self.root)
    }
}

#[async_trait]
impl BackupStore for DelegateBackupStore<'_> {
    fn get_free_disk_space(&self) -> u64 {
        self.delegate.get_free_disk_space(&self.root)
    }

    async fn open_file_read(
        &self,
        path: &BackupPath,
    ) -> Result<Box<dyn Read + Send>, IdeviceError> {
        self.delegate.open_file_read(&self.full_path(path)).await
    }

    async fn begin_replace(
        &mut self,
        path: &BackupPath,
    ) -> Result<Box<dyn BackupFileReplacement + Send>, IdeviceError> {
        let full = self.full_path(path);
        if let Some(parent) = full.parent() {
            self.delegate.create_dir_all(parent).await?;
        }
        let writer = self.delegate.create_file_write(&full).await?;
        Ok(Box::new(DelegateFileReplacement { writer }))
    }

    async fn create_dir_all(&mut self, path: &BackupPath) -> Result<(), IdeviceError> {
        self.delegate.create_dir_all(&self.full_path(path)).await
    }

    async fn remove(&mut self, path: &BackupPath) -> Result<(), IdeviceError> {
        self.delegate.remove(&self.full_path(path)).await
    }

    async fn rename(&mut self, from: &BackupPath, to: &BackupPath) -> Result<(), IdeviceError> {
        let to_full = self.full_path(to);
        if let Some(parent) = to_full.parent() {
            self.delegate.create_dir_all(parent).await?;
        }
        self.delegate.rename(&self.full_path(from), &to_full).await
    }

    async fn copy(&mut self, src: &BackupPath, dst: &BackupPath) -> Result<(), IdeviceError> {
        let dst_full = self.full_path(dst);
        if let Some(parent) = dst_full.parent() {
            self.delegate.create_dir_all(parent).await?;
        }
        self.delegate.copy(&self.full_path(src), &dst_full).await
    }

    async fn exists(&self, path: &BackupPath) -> bool {
        self.delegate.exists(&self.full_path(path)).await
    }

    async fn is_dir(&self, path: &BackupPath) -> bool {
        self.delegate.is_dir(&self.full_path(path)).await
    }

    async fn list_dir(&self, path: &BackupPath) -> Result<Vec<DirEntryInfo>, IdeviceError> {
        self.delegate.list_dir(&self.full_path(path)).await
    }

    fn on_file_received(&self, path: &BackupPath, file_count: u32) {
        self.delegate
            .on_file_received(&path.as_relative_path().to_string_lossy(), file_count);
    }

    fn on_progress(&self, bytes_done: u64, bytes_total: u64, overall_progress: f64) {
        self.delegate
            .on_progress(bytes_done, bytes_total, overall_progress);
    }
}

struct DelegateFileReplacement {
    writer: Box<dyn Write + Send>,
}

impl Write for DelegateFileReplacement {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

#[async_trait]
impl BackupFileReplacement for DelegateFileReplacement {
    async fn finish(mut self: Box<Self>) -> Result<(), IdeviceError> {
        self.flush()
            .map_err(|e| IdeviceError::InternalError(e.to_string()))
    }

    async fn abort(self: Box<Self>) -> Result<(), IdeviceError> {
        Ok(())
    }
}
