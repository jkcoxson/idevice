#![allow(dead_code)]

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::transaction::{BackupTransactionError, JOURNAL_DIR_NAME};

/// Monotonic operation id inside one backup transaction.
///
/// This id is not meaningful outside `.idevice-journal/journal.jsonl`.
/// It lets later records like `old_saved`, `installed`, and `undone` refer
/// back to the operation that was prepared earlier in the same transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct OpId(pub(crate) u64);

/// Identifier for one rollback-journal transaction.
///
/// The value is written in the `begin` record and is primarily diagnostic: it
/// helps humans correlate log lines with one attempted MobileBackup2 operation.
/// It is not an Apple backup identifier and is not stored in backup manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TxId(pub(crate) String);

/// Relative path to an item in the visible iTunes-compatible backup tree.
///
/// `BackupPath` represents a path requested by the MobileBackup2 protocol, such
/// as a backup blob, `Manifest.plist`, or `Status.plist`. The path is always
/// relative to the backup directory for one device/source identifier.
///
/// Example:
///
/// ```text
/// backup_root/
///   00008030-001C195E0E91802E/      <- backup directory
///     Manifest.plist                <- BackupPath("Manifest.plist")
///     Status.plist                  <- BackupPath("Status.plist")
///     ab/cdef1234...                <- BackupPath("ab/cdef1234...")
/// ```
///
/// Values usually originate from the device over MobileBackup2 messages. That
/// makes them external input. Constructors must reject absolute paths, Windows
/// prefixes, and parent-directory components before the path is joined onto the
/// host filesystem.
///
/// The empty string is accepted and represents the backup directory itself.
/// This is needed for protocol messages that list the backup root.
///
/// Rejected examples:
///
/// ```text
/// /tmp/file
/// ../Manifest.plist
/// ab/../../outside
/// C:\Users\name\file
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupPath(PathBuf);

impl BackupPath {
    /// Parse an untrusted MobileBackup2 path into a safe backup-relative path.
    ///
    /// Contract:
    /// - accepts only paths that stay inside one visible backup directory
    /// - rejects absolute paths, path prefixes, and `..`
    /// - accepts the empty string as the backup root
    /// - does not touch the filesystem
    pub(crate) fn parse(input: &str) -> Result<Self, BackupTransactionError> {
        validate_relative_path(input, true).map(Self)
    }

    /// Parse a path for operations that mutate or read a concrete backup item.
    ///
    /// Unlike `parse`, this rejects the backup root itself. Use `parse` only for
    /// protocol requests where the root directory is a legitimate target, such
    /// as directory listing.
    pub(crate) fn parse_item(input: &str) -> Result<Self, BackupTransactionError> {
        validate_relative_path(input, false).map(Self)
    }

    /// Join this validated path onto the visible backup directory.
    ///
    /// Example:
    ///
    /// ```text
    /// BackupPath("ab/cdef").join_to_backup_dir("/Backups/UDID")
    /// => /Backups/UDID/ab/cdef
    /// ```
    pub(crate) fn join_to_backup_dir(&self, backup_dir: &Path) -> PathBuf {
        backup_dir.join(&self.0)
    }

    /// Return the path as stored in the journal.
    pub(crate) fn as_relative_path(&self) -> &Path {
        &self.0
    }
}

/// Relative path to a file owned by `.idevice-journal/`.
///
/// `JournalPath` points to transaction-private files used for rollback and
/// staging. These files are not part of the iTunes backup format and should not
/// be referenced by `Manifest.plist` or exposed to the device as backup content.
///
/// Example:
///
/// ```text
/// backup_root/
///   00008030-001C195E0E91802E/
///     .idevice-journal/             <- journal root
///       journal.jsonl
///       tmp/00000001.tmp            <- JournalPath("tmp/00000001.tmp")
///       old/00000001.old            <- JournalPath("old/00000001.old")
/// ```
///
/// Paths are stored relative to `.idevice-journal/`, not as absolute host
/// paths. That keeps recovery possible if the whole backup directory is moved.
/// `tmp/` files contain newly received data before it is installed into the
/// visible backup tree. `old/` files contain previous versions of visible
/// backup files so rollback can restore the last committed backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JournalPath(PathBuf);

impl JournalPath {
    /// Deterministic tmp path for an operation.
    ///
    /// This only constructs the journal-relative path. It does not create the
    /// file or reserve the operation id.
    pub(crate) fn tmp(op: OpId) -> Self {
        Self(PathBuf::from(format!("tmp/{:016}.tmp", op.0)))
    }

    /// Deterministic old-state path for an operation.
    ///
    /// The old-state file stores the previous contents of a visible backup path
    /// before that path is replaced, removed, moved over, or copied over.
    pub(crate) fn old(op: OpId) -> Self {
        Self(PathBuf::from(format!("old/{:016}.old", op.0)))
    }

    /// Parse a path read from `journal.jsonl`.
    ///
    /// Contract:
    /// - accepts only paths under `.idevice-journal/tmp/` or `.idevice-journal/old/`
    /// - rejects absolute paths, path prefixes, empty paths, `.`, and `..`
    /// - does not touch the filesystem
    pub(crate) fn parse_recorded(path: PathBuf) -> Result<Self, BackupTransactionError> {
        let path_string = path.to_string_lossy();
        let path = validate_relative_path(&path_string, false)?;
        match path.components().next() {
            Some(Component::Normal(prefix)) if prefix == "tmp" || prefix == "old" => Ok(Self(path)),
            _ => Err(BackupTransactionError::InvalidJournalPath(
                path_string.into_owned(),
            )),
        }
    }

    /// Join this journal-relative path onto `.idevice-journal/`.
    ///
    /// Example:
    ///
    /// ```text
    /// JournalPath("tmp/0000000000000001.tmp").join_to_journal_dir("/Backups/UDID/.idevice-journal")
    /// => /Backups/UDID/.idevice-journal/tmp/0000000000000001.tmp
    /// ```
    pub(crate) fn join_to_journal_dir(&self, journal_dir: &Path) -> PathBuf {
        journal_dir.join(&self.0)
    }

    /// Return the path as stored in the journal.
    pub(crate) fn as_relative_path(&self) -> &Path {
        &self.0
    }
}

fn validate_relative_path(
    input: &str,
    allow_empty: bool,
) -> Result<PathBuf, BackupTransactionError> {
    if input.is_empty() {
        return if allow_empty {
            Ok(PathBuf::new())
        } else {
            Err(BackupTransactionError::InvalidBackupPath(input.into()))
        };
    }

    if input.contains('\\') || input.contains('\0') {
        return Err(BackupTransactionError::InvalidBackupPath(input.into()));
    }

    let path = Path::new(input);
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {
                return Err(BackupTransactionError::InvalidBackupPath(input.into()));
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(BackupTransactionError::InvalidBackupPath(input.into()));
            }
        }
    }

    if out.as_os_str().is_empty() && !allow_empty {
        return Err(BackupTransactionError::InvalidBackupPath(input.into()));
    }
    if matches!(
        out.components().next(),
        Some(Component::Normal(first)) if first == JOURNAL_DIR_NAME
    ) {
        return Err(BackupTransactionError::InvalidBackupPath(input.into()));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_path_accepts_relative_paths_and_empty_root() {
        assert_eq!(
            BackupPath::parse("ab/cdef")
                .unwrap()
                .as_relative_path()
                .to_string_lossy(),
            "ab/cdef"
        );
        assert_eq!(
            BackupPath::parse("").unwrap().as_relative_path(),
            Path::new("")
        );
    }

    #[test]
    fn backup_item_path_rejects_root_paths() {
        for input in ["", ".", "./"] {
            assert!(BackupPath::parse_item(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn backup_path_rejects_escape_and_platform_ambiguous_paths() {
        for input in [
            "/tmp/file",
            "../Manifest.plist",
            "ab/../../outside",
            "C:\\Users\\name\\file",
            "ab\\cdef",
            "ab/\0/cdef",
            ".",
            "./",
            ".idevice-journal",
            ".idevice-journal/journal.jsonl",
        ] {
            assert!(BackupPath::parse(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn journal_path_accepts_only_tmp_and_old_relative_paths() {
        assert_eq!(
            JournalPath::parse_recorded(PathBuf::from("tmp/0000000000000001.tmp"))
                .unwrap()
                .as_relative_path()
                .to_string_lossy(),
            "tmp/0000000000000001.tmp"
        );
        assert!(JournalPath::parse_recorded(PathBuf::from("Manifest.plist")).is_err());
        assert!(JournalPath::parse_recorded(PathBuf::from("../old/file")).is_err());
    }
}
