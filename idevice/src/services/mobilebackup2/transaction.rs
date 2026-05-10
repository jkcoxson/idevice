#![allow(dead_code)]

//! Rollback journal for MobileBackup2 host-side backup mutations.
//!
//! MobileBackup2 asks the host to create, replace, move, copy, and remove files
//! inside an iTunes-compatible backup directory. Those mutations are applied
//! immediately so the device sees the same filesystem state it requested.
//!
//! Before each visible backup mutation, this module appends enough information
//! to `.idevice-journal/journal.jsonl` to undo the mutation later. If the
//! process dies before commit, recovery replays the journal and rolls installed
//! operations back in reverse order. If commit completed, recovery only removes
//! leftover journal files.

use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::journal::{JsonLineJournal, SequencedRecord};
use super::paths::{BackupPath, JournalPath, OpId, TxId};
use crate::IdeviceError;

pub(crate) const JOURNAL_DIR_NAME: &str = ".idevice-journal";
pub(crate) const JOURNAL_FILE_NAME: &str = "journal.jsonl";

/// Error type for transaction and recovery code.
///
/// The transaction layer keeps its own error type so path validation, corrupt
/// journals, unsupported atomic primitives, and IO failures stay distinguishable
/// while this code is being developed. Public MobileBackup2 APIs can convert it
/// into `IdeviceError` at the boundary.
#[derive(Debug, Error)]
pub(crate) enum BackupTransactionError {
    #[error("transaction IO failed")]
    Io(#[from] io::Error),
    #[error("transaction JSON serialization failed")]
    Json(#[from] serde_json::Error),
    #[error("invalid backup path: {0}")]
    InvalidBackupPath(String),
    #[error("invalid journal path: {0}")]
    InvalidJournalPath(String),
    #[error("corrupt backup journal: {0}")]
    CorruptJournal(String),
    #[error("atomic filesystem operation is not implemented")]
    UnsupportedAtomicOperation,
}

impl From<BackupTransactionError> for IdeviceError {
    fn from(value: BackupTransactionError) -> Self {
        IdeviceError::InternalError(value.to_string())
    }
}

/// One durable fact in the append-only backup transaction journal.
///
/// Records are never edited in place. A later record advances the state of an
/// earlier operation. This makes crash recovery simple: ignore a partial final
/// line, replay complete records in order, then decide whether to clean up a
/// committed transaction or roll back an incomplete one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum BackupRecord {
    Begin {
        tx_id: TxId,
    },
    ReplacePrepared {
        op: OpId,
        path: BackupPath,
        old: Option<JournalPath>,
        tmp: JournalPath,
        old_existed: bool,
    },
    OldSaved {
        op: OpId,
    },
    TempWritten {
        op: OpId,
        size: u64,
    },
    Installed {
        op: OpId,
    },
    RemovePrepared {
        op: OpId,
        path: BackupPath,
        old: Option<JournalPath>,
        old_existed: bool,
    },
    Removed {
        op: OpId,
    },
    MovePrepared {
        op: OpId,
        from: BackupPath,
        to: BackupPath,
        replaced: Option<JournalPath>,
        replaced_existed: bool,
    },
    Moved {
        op: OpId,
    },
    CopyPrepared {
        op: OpId,
        from: BackupPath,
        to: BackupPath,
        replaced: Option<JournalPath>,
        replaced_existed: bool,
    },
    Copied {
        op: OpId,
    },
    MkdirPrepared {
        op: OpId,
        path: BackupPath,
        existed: bool,
    },
    MkdirDone {
        op: OpId,
    },
    CommitReady,
    Committed,
    RollbackStarted,
    Undone {
        op: OpId,
    },
    RolledBack,
}

/// Transaction-level status derived from replaying `BackupRecord`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransactionStatus {
    Active,
    CommitReady,
    Committed,
    RollbackStarted,
    RolledBack,
}

/// Operation-level phase derived from replaying `BackupRecord`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationPhase {
    Prepared,
    OldSaved,
    TempWritten,
    Installed,
    Undone,
}

/// Prepared filesystem mutation with all paths needed for rollback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PreparedOperation {
    Replace {
        path: BackupPath,
        old: Option<JournalPath>,
        tmp: JournalPath,
        old_existed: bool,
    },
    Remove {
        path: BackupPath,
        old: Option<JournalPath>,
        old_existed: bool,
    },
    Move {
        from: BackupPath,
        to: BackupPath,
        replaced: Option<JournalPath>,
        replaced_existed: bool,
    },
    Copy {
        from: BackupPath,
        to: BackupPath,
        replaced: Option<JournalPath>,
        replaced_existed: bool,
    },
    Mkdir {
        path: BackupPath,
        existed: bool,
    },
}

/// Current state of one prepared operation, reconstructed from the journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationState {
    pub(crate) op: OpId,
    pub(crate) operation: PreparedOperation,
    pub(crate) phase: OperationPhase,
}

/// In-memory projection of `.idevice-journal/journal.jsonl`.
///
/// The JSONL file is the source of truth. `TransactionState` is a replay result
/// optimized for recovery decisions: what transaction phase are we in, which
/// operations were installed, and what must be undone in reverse order?
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransactionState {
    pub(crate) tx_id: Option<TxId>,
    pub(crate) status: TransactionStatus,
    operations: BTreeMap<OpId, OperationState>,
    install_order: Vec<OpId>,
}

impl TransactionState {
    /// Reconstruct transaction state from complete journal records.
    ///
    /// Contract:
    /// - processes records in journal order
    /// - uses `BTreeMap` for deterministic operation ordering
    /// - does not touch the filesystem
    pub(crate) fn from_records(
        records: Vec<SequencedRecord<BackupRecord>>,
    ) -> Result<Self, BackupTransactionError> {
        let mut state = Self {
            tx_id: None,
            status: TransactionStatus::Active,
            operations: BTreeMap::new(),
            install_order: Vec::new(),
        };

        for entry in records {
            let is_begin = matches!(entry.record, BackupRecord::Begin { .. });
            if state.tx_id.is_none() && !is_begin {
                return Err(corrupt("journal record before begin"));
            }
            state.apply_record(entry.record)?;
        }

        Ok(state)
    }

    /// Return installed operations that still need rollback.
    ///
    /// Rollback callers should process the returned operations in reverse order
    /// so dependent operations are undone before the earlier mutations they may
    /// rely on.
    pub(crate) fn installed_operations(&self) -> Vec<OperationState> {
        if matches!(
            self.status,
            TransactionStatus::Committed | TransactionStatus::RolledBack
        ) {
            return Vec::new();
        }
        self.installed_operation_projection()
    }

    fn installed_operation_projection(&self) -> Vec<OperationState> {
        self.install_order
            .iter()
            .rev()
            .filter_map(|op| self.operations.get(op))
            .filter(|op| op.phase == OperationPhase::Installed)
            .cloned()
            .collect()
    }

    fn apply_record(&mut self, record: BackupRecord) -> Result<(), BackupTransactionError> {
        match record {
            BackupRecord::Begin { tx_id } => {
                if self.tx_id.is_some() || self.status != TransactionStatus::Active {
                    return Err(corrupt("duplicate or late begin record"));
                }
                self.tx_id = Some(tx_id);
            }
            BackupRecord::ReplacePrepared {
                op,
                path,
                old,
                tmp,
                old_existed,
            } => self.prepare(
                op,
                PreparedOperation::Replace {
                    path,
                    old,
                    tmp,
                    old_existed,
                },
            )?,
            BackupRecord::RemovePrepared {
                op,
                path,
                old,
                old_existed,
            } => self.prepare(
                op,
                PreparedOperation::Remove {
                    path,
                    old,
                    old_existed,
                },
            )?,
            BackupRecord::MovePrepared {
                op,
                from,
                to,
                replaced,
                replaced_existed,
            } => self.prepare(
                op,
                PreparedOperation::Move {
                    from,
                    to,
                    replaced,
                    replaced_existed,
                },
            )?,
            BackupRecord::CopyPrepared {
                op,
                from,
                to,
                replaced,
                replaced_existed,
            } => self.prepare(
                op,
                PreparedOperation::Copy {
                    from,
                    to,
                    replaced,
                    replaced_existed,
                },
            )?,
            BackupRecord::MkdirPrepared { op, path, existed } => {
                self.prepare(op, PreparedOperation::Mkdir { path, existed })?
            }
            BackupRecord::OldSaved { op } => {
                self.require_active()?;
                self.advance(op, OperationPhase::OldSaved, |operation, phase| {
                    old_state_needed(operation)
                        && match operation {
                            PreparedOperation::Replace { .. } => {
                                phase == OperationPhase::TempWritten
                            }
                            PreparedOperation::Remove { .. }
                            | PreparedOperation::Move { .. }
                            | PreparedOperation::Copy { .. } => phase == OperationPhase::Prepared,
                            PreparedOperation::Mkdir { .. } => false,
                        }
                })?;
            }
            BackupRecord::TempWritten { op, .. } => {
                self.require_active()?;
                self.advance(op, OperationPhase::TempWritten, |operation, phase| {
                    matches!(operation, PreparedOperation::Replace { .. })
                        && phase == OperationPhase::Prepared
                })?;
            }
            BackupRecord::Installed { op } => {
                self.require_active()?;
                self.advance(op, OperationPhase::Installed, |operation, phase| {
                    matches!(operation, PreparedOperation::Replace { .. })
                        && replace_ready(operation, phase)
                })?;
            }
            BackupRecord::Removed { op } => {
                self.require_active()?;
                self.advance(op, OperationPhase::Installed, |operation, phase| {
                    matches!(operation, PreparedOperation::Remove { .. })
                        && old_state_ready(operation, phase)
                })?;
            }
            BackupRecord::Moved { op } => {
                self.require_active()?;
                self.advance(op, OperationPhase::Installed, |operation, phase| {
                    matches!(operation, PreparedOperation::Move { .. })
                        && old_state_ready(operation, phase)
                })?;
            }
            BackupRecord::Copied { op } => {
                self.require_active()?;
                self.advance(op, OperationPhase::Installed, |operation, phase| {
                    matches!(operation, PreparedOperation::Copy { .. })
                        && old_state_ready(operation, phase)
                })?;
            }
            BackupRecord::MkdirDone { op } => {
                self.require_active()?;
                self.advance(op, OperationPhase::Installed, |operation, phase| {
                    matches!(
                        (operation, phase),
                        (PreparedOperation::Mkdir { .. }, OperationPhase::Prepared)
                    )
                })?;
            }
            BackupRecord::CommitReady => {
                if self.status != TransactionStatus::Active {
                    return Err(corrupt("commit_ready outside active transaction"));
                }
                self.status = TransactionStatus::CommitReady;
            }
            BackupRecord::Committed => {
                if self.status != TransactionStatus::CommitReady {
                    return Err(corrupt("committed without commit_ready"));
                }
                self.status = TransactionStatus::Committed;
            }
            BackupRecord::RollbackStarted => {
                if matches!(
                    self.status,
                    TransactionStatus::Committed | TransactionStatus::RolledBack
                ) {
                    return Err(corrupt("rollback_started after terminal transaction state"));
                }
                self.status = TransactionStatus::RollbackStarted;
            }
            BackupRecord::Undone { op } => {
                if self.status != TransactionStatus::RollbackStarted {
                    return Err(corrupt("undone outside rollback"));
                }
                if self
                    .installed_operation_projection()
                    .first()
                    .is_none_or(|operation| operation.op != op)
                {
                    return Err(corrupt("undone operation is not next in rollback order"));
                }
                self.advance(op, OperationPhase::Undone, |_operation, phase| {
                    phase == OperationPhase::Installed
                })?;
            }
            BackupRecord::RolledBack => {
                if self.status != TransactionStatus::RollbackStarted {
                    return Err(corrupt("rolled_back without rollback_started"));
                }
                if !self.installed_operation_projection().is_empty() {
                    return Err(corrupt("rolled_back with installed operations remaining"));
                }
                self.status = TransactionStatus::RolledBack;
            }
        }

        Ok(())
    }

    fn prepare(
        &mut self,
        op: OpId,
        operation: PreparedOperation,
    ) -> Result<(), BackupTransactionError> {
        if self.tx_id.is_none() {
            return Err(corrupt("operation prepared before begin"));
        }
        if self.operations.contains_key(&op) {
            return Err(corrupt("duplicate operation id"));
        }
        if self.status != TransactionStatus::Active {
            return Err(corrupt("operation prepared outside active transaction"));
        }
        validate_prepared_operation(&operation)?;

        self.operations.insert(
            op,
            OperationState {
                op,
                operation,
                phase: OperationPhase::Prepared,
            },
        );
        Ok(())
    }

    fn advance(
        &mut self,
        op: OpId,
        next: OperationPhase,
        allowed: impl FnOnce(&PreparedOperation, OperationPhase) -> bool,
    ) -> Result<(), BackupTransactionError> {
        let operation = self
            .operations
            .get_mut(&op)
            .ok_or_else(|| corrupt("operation phase references unknown operation"))?;
        if !allowed(&operation.operation, operation.phase) {
            return Err(corrupt("invalid operation phase transition"));
        }
        operation.phase = next;
        if next == OperationPhase::Installed {
            self.install_order.push(op);
        }
        Ok(())
    }

    fn require_active(&self) -> Result<(), BackupTransactionError> {
        if self.status == TransactionStatus::Active {
            Ok(())
        } else {
            Err(corrupt("operation phase outside active transaction"))
        }
    }
}

fn corrupt(message: impl Into<String>) -> BackupTransactionError {
    BackupTransactionError::CorruptJournal(message.into())
}

fn old_state_needed(operation: &PreparedOperation) -> bool {
    match operation {
        PreparedOperation::Replace { old_existed, .. }
        | PreparedOperation::Remove { old_existed, .. } => *old_existed,
        PreparedOperation::Move {
            replaced_existed, ..
        }
        | PreparedOperation::Copy {
            replaced_existed, ..
        } => *replaced_existed,
        PreparedOperation::Mkdir { .. } => false,
    }
}

fn old_state_ready(operation: &PreparedOperation, phase: OperationPhase) -> bool {
    match operation {
        PreparedOperation::Replace { old_existed, .. }
        | PreparedOperation::Remove { old_existed, .. } => {
            (*old_existed && phase == OperationPhase::OldSaved)
                || (!*old_existed && phase == OperationPhase::Prepared)
        }
        PreparedOperation::Move {
            replaced_existed, ..
        }
        | PreparedOperation::Copy {
            replaced_existed, ..
        } => {
            (*replaced_existed && phase == OperationPhase::OldSaved)
                || (!*replaced_existed && phase == OperationPhase::Prepared)
        }
        PreparedOperation::Mkdir { .. } => phase == OperationPhase::Prepared,
    }
}

fn replace_ready(operation: &PreparedOperation, phase: OperationPhase) -> bool {
    match operation {
        PreparedOperation::Replace { old_existed, .. } => {
            (*old_existed && phase == OperationPhase::OldSaved)
                || (!*old_existed && phase == OperationPhase::TempWritten)
        }
        _ => false,
    }
}

fn validate_prepared_operation(
    operation: &PreparedOperation,
) -> Result<(), BackupTransactionError> {
    let valid = match operation {
        PreparedOperation::Replace {
            old, old_existed, ..
        }
        | PreparedOperation::Remove {
            old, old_existed, ..
        } => old.is_some() == *old_existed,
        PreparedOperation::Move {
            replaced,
            replaced_existed,
            ..
        }
        | PreparedOperation::Copy {
            replaced,
            replaced_existed,
            ..
        } => replaced.is_some() == *replaced_existed,
        PreparedOperation::Mkdir { .. } => true,
    };

    if valid {
        Ok(())
    } else {
        Err(corrupt("prepared operation old-state flag/path mismatch"))
    }
}

/// Live rollback transaction for one visible backup directory.
///
/// `backup_dir` is the iTunes-compatible directory for one source identifier.
/// `journal_dir` is `backup_dir/.idevice-journal`. All `JournalPath`s are
/// relative to `journal_dir`; all `BackupPath`s are relative to `backup_dir`.
pub(crate) struct BackupTransaction {
    backup_dir: PathBuf,
    journal_dir: PathBuf,
    journal: JsonLineJournal<BackupRecord>,
    state: TransactionState,
    next_op: OpId,
}

impl BackupTransaction {
    /// Start a new transaction in `<backup_dir>/.idevice-journal`.
    ///
    /// Contract:
    /// - creates `tmp/`, `old/`, and `journal.jsonl`
    /// - appends a durable `begin` record
    /// - fails if an unfinished journal already exists; call `recover()` first
    pub(crate) fn begin(_backup_dir: &Path, _tx_id: TxId) -> Result<Self, BackupTransactionError> {
        todo!("start a backup rollback transaction")
    }

    /// Recover or clean up any existing transaction journal for `backup_dir`.
    ///
    /// Contract:
    /// - no-op if `.idevice-journal/` does not exist
    /// - committed or rolled-back journals are cleaned up
    /// - active or rollback-started journals are rolled back idempotently
    /// - safe to call before every public MobileBackup2 backup-facing operation
    pub(crate) fn recover(_backup_dir: &Path) -> Result<(), BackupTransactionError> {
        todo!("recover or clean up an existing backup rollback transaction")
    }

    /// Prepare replacement of one visible backup file.
    ///
    /// This does not modify `path` yet. The returned `FileReplacement` writes
    /// into `.idevice-journal/tmp/`. The visible path is changed only by
    /// `FileReplacement::finish()`, after the tmp file is complete and the old
    /// destination state has been journaled.
    pub(crate) fn begin_replace(
        &mut self,
        _path: BackupPath,
    ) -> Result<FileReplacement<'_>, BackupTransactionError> {
        todo!("prepare journal-backed file replacement")
    }

    /// Remove one visible backup path with rollback protection.
    ///
    /// Contract:
    /// - appends remove intent before touching `path`
    /// - preserves old state under `.idevice-journal/old/` when `path` exists
    /// - appends `removed` only after the visible mutation succeeds
    pub(crate) fn remove(&mut self, _path: BackupPath) -> Result<(), BackupTransactionError> {
        todo!("remove visible backup path with rollback journal")
    }

    /// Rename one visible backup path with rollback protection.
    ///
    /// Contract:
    /// - appends move intent before touching either path
    /// - preserves destination state if `to` exists
    /// - appends `moved` only after the visible mutation succeeds
    pub(crate) fn rename(
        &mut self,
        _from: BackupPath,
        _to: BackupPath,
    ) -> Result<(), BackupTransactionError> {
        todo!("rename visible backup path with rollback journal")
    }

    /// Copy one visible backup path with rollback protection.
    ///
    /// Contract:
    /// - appends copy intent before touching the destination
    /// - preserves destination state if `to` exists
    /// - appends `copied` only after the visible mutation succeeds
    pub(crate) fn copy(
        &mut self,
        _from: BackupPath,
        _to: BackupPath,
    ) -> Result<(), BackupTransactionError> {
        todo!("copy visible backup path with rollback journal")
    }

    /// Create a visible backup directory with rollback protection.
    ///
    /// Contract:
    /// - records whether the directory existed before this transaction
    /// - rollback removes only directories created by this operation, and only
    ///   when it is safe to do so
    pub(crate) fn create_dir(&mut self, _path: BackupPath) -> Result<(), BackupTransactionError> {
        todo!("create visible backup directory with rollback journal")
    }

    /// Mark this transaction successful and remove rollback data.
    ///
    /// Contract:
    /// - caller must validate MobileBackup2 final success before calling
    /// - appends `commit_ready`, then `committed`
    /// - after `committed` is durable, recovery must never roll this transaction
    ///   back; it may only finish cleanup
    pub(crate) fn commit(self) -> Result<(), BackupTransactionError> {
        todo!("commit backup rollback transaction")
    }

    /// Abort this live transaction and roll back installed operations.
    ///
    /// Contract:
    /// - appends `rollback_started`
    /// - undoes installed operations in reverse operation order
    /// - appends `undone` after each successful inverse mutation
    /// - appends `rolled_back` before cleanup
    pub(crate) fn rollback(self) -> Result<(), BackupTransactionError> {
        todo!("roll back backup transaction")
    }

    fn append(&mut self, _record: BackupRecord) -> Result<(), BackupTransactionError> {
        todo!("append record and update in-memory transaction state")
    }

    fn allocate_op(&mut self) -> OpId {
        todo!("allocate the next transaction-local operation id")
    }
}

/// Staged replacement for one visible backup file.
///
/// The file handle writes only to `.idevice-journal/tmp/`. `finish()` is the
/// only method that may install the tmp file into the visible backup tree.
/// `Drop` intentionally does not roll back because rollback is fallible.
pub(crate) struct FileReplacement<'tx> {
    tx: &'tx mut BackupTransaction,
    op: OpId,
    path: BackupPath,
    tmp: JournalPath,
    file: File,
    bytes_written: u64,
    finished: bool,
}

impl FileReplacement<'_> {
    /// Write data into journal tmp storage only.
    ///
    /// Contract:
    /// - never mutates the visible backup path
    /// - updates `bytes_written` only after the write succeeds
    pub(crate) fn write_all(&mut self, _data: &[u8]) -> Result<(), BackupTransactionError> {
        todo!("write replacement data to journal tmp file")
    }

    /// Flush tmp, save old destination state, and install tmp.
    ///
    /// Contract:
    /// - appends `temp_written` after the tmp file is flushed
    /// - appends old-state records before replacing the visible file
    /// - uses the atomic filesystem boundary to install the tmp file
    /// - appends `installed` only after the visible mutation succeeds
    pub(crate) fn finish(self) -> Result<(), BackupTransactionError> {
        todo!("finish and install journal-backed file replacement")
    }

    /// Discard tmp without touching the visible backup path.
    ///
    /// Contract:
    /// - used when the device does not send a successful file trailer
    /// - never appends `installed`
    pub(crate) fn abort(self) -> Result<(), BackupTransactionError> {
        todo!("abort journal-backed file replacement")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u64, record: BackupRecord) -> SequencedRecord<BackupRecord> {
        SequencedRecord { seq, record }
    }

    fn tx_id() -> TxId {
        TxId("tx-1".into())
    }

    fn backup_path(path: &str) -> BackupPath {
        BackupPath::parse_item(path).unwrap()
    }

    fn tmp(op: u64) -> JournalPath {
        JournalPath::tmp(OpId(op))
    }

    fn old(op: u64) -> JournalPath {
        JournalPath::old(OpId(op))
    }

    #[test]
    fn replay_derives_committed_replace_state() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::ReplacePrepared {
                    op: OpId(1),
                    path: backup_path("aa/bb"),
                    old: Some(old(1)),
                    tmp: tmp(1),
                    old_existed: true,
                },
            ),
            rec(
                3,
                BackupRecord::TempWritten {
                    op: OpId(1),
                    size: 12,
                },
            ),
            rec(4, BackupRecord::OldSaved { op: OpId(1) }),
            rec(5, BackupRecord::Installed { op: OpId(1) }),
            rec(6, BackupRecord::CommitReady),
            rec(7, BackupRecord::Committed),
        ];

        let state = TransactionState::from_records(records).unwrap();

        assert_eq!(state.tx_id, Some(tx_id()));
        assert_eq!(state.status, TransactionStatus::Committed);
        assert!(state.installed_operations().is_empty());
    }

    #[test]
    fn installed_operations_are_returned_in_reverse_install_order_for_rollback() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::MkdirPrepared {
                    op: OpId(1),
                    path: backup_path("aa"),
                    existed: false,
                },
            ),
            rec(3, BackupRecord::MkdirDone { op: OpId(1) }),
            rec(
                4,
                BackupRecord::RemovePrepared {
                    op: OpId(2),
                    path: backup_path("aa/file"),
                    old: Some(old(2)),
                    old_existed: true,
                },
            ),
            rec(5, BackupRecord::OldSaved { op: OpId(2) }),
            rec(6, BackupRecord::Removed { op: OpId(2) }),
        ];

        let state = TransactionState::from_records(records).unwrap();

        let ops: Vec<OpId> = state
            .installed_operations()
            .into_iter()
            .map(|op| op.op)
            .collect();
        assert_eq!(ops, vec![OpId(2), OpId(1)]);
    }

    #[test]
    fn installed_operations_follow_reverse_replay_order_not_operation_id_order() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::MkdirPrepared {
                    op: OpId(2),
                    path: backup_path("later-id"),
                    existed: false,
                },
            ),
            rec(
                3,
                BackupRecord::MkdirPrepared {
                    op: OpId(1),
                    path: backup_path("earlier-id"),
                    existed: false,
                },
            ),
            rec(4, BackupRecord::MkdirDone { op: OpId(2) }),
            rec(5, BackupRecord::MkdirDone { op: OpId(1) }),
        ];

        let state = TransactionState::from_records(records).unwrap();
        let ops: Vec<OpId> = state
            .installed_operations()
            .into_iter()
            .map(|op| op.op)
            .collect();

        assert_eq!(ops, vec![OpId(1), OpId(2)]);
    }

    #[test]
    fn replay_rejects_installed_before_required_replace_phases() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::ReplacePrepared {
                    op: OpId(1),
                    path: backup_path("aa/bb"),
                    old: Some(old(1)),
                    tmp: tmp(1),
                    old_existed: true,
                },
            ),
            rec(3, BackupRecord::Installed { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_new_replace_install_before_temp_written() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::ReplacePrepared {
                    op: OpId(1),
                    path: backup_path("aa/new"),
                    old: None,
                    tmp: tmp(1),
                    old_existed: false,
                },
            ),
            rec(3, BackupRecord::Installed { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_replace_old_saved_before_temp_written() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::ReplacePrepared {
                    op: OpId(1),
                    path: backup_path("aa/bb"),
                    old: Some(old(1)),
                    tmp: tmp(1),
                    old_existed: true,
                },
            ),
            rec(3, BackupRecord::OldSaved { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_remove_install_when_existing_old_state_was_not_saved() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::RemovePrepared {
                    op: OpId(1),
                    path: backup_path("aa/file"),
                    old: Some(old(1)),
                    old_existed: true,
                },
            ),
            rec(3, BackupRecord::Removed { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_allows_remove_install_without_old_state_when_path_did_not_exist() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::RemovePrepared {
                    op: OpId(1),
                    path: backup_path("aa/file"),
                    old: None,
                    old_existed: false,
                },
            ),
            rec(3, BackupRecord::Removed { op: OpId(1) }),
        ];

        let state = TransactionState::from_records(records).unwrap();

        assert_eq!(
            state.installed_operations(),
            vec![OperationState {
                op: OpId(1),
                operation: PreparedOperation::Remove {
                    path: backup_path("aa/file"),
                    old: None,
                    old_existed: false,
                },
                phase: OperationPhase::Installed,
            }]
        );
    }

    #[test]
    fn replay_rejects_old_state_flag_path_mismatch() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::RemovePrepared {
                    op: OpId(1),
                    path: backup_path("aa/file"),
                    old: None,
                    old_existed: true,
                },
            ),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_old_saved_when_old_state_was_not_needed() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::RemovePrepared {
                    op: OpId(1),
                    path: backup_path("aa/file"),
                    old: None,
                    old_existed: false,
                },
            ),
            rec(3, BackupRecord::OldSaved { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_duplicate_begin() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::Begin {
                    tx_id: TxId("tx-2".into()),
                },
            ),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_transaction_record_before_begin() {
        let records = vec![rec(1, BackupRecord::CommitReady)];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_committed_without_commit_ready() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(2, BackupRecord::Committed),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_transition_for_unknown_operation() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(2, BackupRecord::Installed { op: OpId(99) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_operation_progress_after_commit_ready() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::MkdirPrepared {
                    op: OpId(1),
                    path: backup_path("aa"),
                    existed: false,
                },
            ),
            rec(3, BackupRecord::CommitReady),
            rec(4, BackupRecord::MkdirDone { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_operation_before_begin() {
        let records = vec![rec(
            1,
            BackupRecord::MkdirPrepared {
                op: OpId(1),
                path: backup_path("aa"),
                existed: false,
            },
        )];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_undone_before_rollback_started() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::MkdirPrepared {
                    op: OpId(1),
                    path: backup_path("aa"),
                    existed: false,
                },
            ),
            rec(3, BackupRecord::MkdirDone { op: OpId(1) }),
            rec(4, BackupRecord::Undone { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_undone_outside_reverse_install_order() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::MkdirPrepared {
                    op: OpId(1),
                    path: backup_path("aa"),
                    existed: false,
                },
            ),
            rec(3, BackupRecord::MkdirDone { op: OpId(1) }),
            rec(
                4,
                BackupRecord::MkdirPrepared {
                    op: OpId(2),
                    path: backup_path("aa/bb"),
                    existed: false,
                },
            ),
            rec(5, BackupRecord::MkdirDone { op: OpId(2) }),
            rec(6, BackupRecord::RollbackStarted),
            rec(7, BackupRecord::Undone { op: OpId(1) }),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn replay_rejects_rolled_back_with_installed_operations_remaining() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::MkdirPrepared {
                    op: OpId(1),
                    path: backup_path("aa"),
                    existed: false,
                },
            ),
            rec(3, BackupRecord::MkdirDone { op: OpId(1) }),
            rec(4, BackupRecord::RollbackStarted),
            rec(5, BackupRecord::RolledBack),
        ];

        assert!(matches!(
            TransactionState::from_records(records),
            Err(BackupTransactionError::CorruptJournal(_))
        ));
    }

    #[test]
    fn installed_operations_excludes_undone_operations() {
        let records = vec![
            rec(1, BackupRecord::Begin { tx_id: tx_id() }),
            rec(
                2,
                BackupRecord::MkdirPrepared {
                    op: OpId(1),
                    path: backup_path("aa"),
                    existed: false,
                },
            ),
            rec(3, BackupRecord::MkdirDone { op: OpId(1) }),
            rec(4, BackupRecord::RollbackStarted),
            rec(5, BackupRecord::Undone { op: OpId(1) }),
        ];

        let state = TransactionState::from_records(records).unwrap();

        assert_eq!(state.status, TransactionStatus::RollbackStarted);
        assert!(state.installed_operations().is_empty());
    }
}
