#![allow(dead_code)]

use std::path::Path;

use super::transaction::BackupTransactionError;

/// Replace `to` with `from` atomically on the same filesystem.
///
/// Contract:
/// - `from` and `to` must be on the same filesystem
/// - `from` must be a fully written and flushed file
/// - on success, `to` contains the previous contents of `from`
/// - on success, `from` no longer exists
/// - on failure, callers must use journal recovery to decide whether to retry,
///   roll back, or clean up leftovers
///
/// This is intentionally an explicit platform boundary. POSIX and Windows have
/// different replacement semantics, especially when `to` already exists.
pub(crate) fn atomic_replace_file(_from: &Path, _to: &Path) -> Result<(), BackupTransactionError> {
    unimplemented!("atomic same-filesystem replace is intentionally left as a platform boundary")
}
