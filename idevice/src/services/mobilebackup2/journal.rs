#![allow(dead_code)]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::IdeviceError;

const JOURNAL_MAGIC: &str = "# idevice mobilebackup2 journal v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SequencedRecord<T> {
    pub(crate) seq: u64,
    #[serde(flatten)]
    pub(crate) record: T,
}

#[derive(Debug)]
pub(crate) struct JsonLineJournal<T> {
    path: PathBuf,
    file: File,
    next_seq: u64,
    _marker: PhantomData<T>,
}

impl<T> JsonLineJournal<T>
where
    T: Serialize + DeserializeOwned,
{
    pub(crate) fn create(path: &Path) -> Result<Self, IdeviceError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let exists = path.exists();
        if exists {
            let valid_len = Self::valid_prefix_len(path)?;
            OpenOptions::new()
                .write(true)
                .open(path)?
                .set_len(valid_len)?;
        }

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        if !exists || file.metadata()?.len() == 0 {
            writeln!(file, "{JOURNAL_MAGIC}")?;
            file.sync_data()?;
        }

        let next_seq = Self::replay(path)?.last().map_or(1, |entry| entry.seq + 1);
        Ok(Self {
            path: path.to_path_buf(),
            file,
            next_seq,
            _marker: PhantomData,
        })
    }

    pub(crate) fn append(&mut self, record: &T) -> Result<u64, IdeviceError> {
        let seq = self.next_seq;
        let line = serde_json::to_string(&SequencedRecord { seq, record })?;
        writeln!(self.file, "{line}")?;
        self.file.sync_data()?;
        self.next_seq += 1;
        Ok(seq)
    }

    pub(crate) fn replay(path: &Path) -> Result<Vec<SequencedRecord<T>>, IdeviceError> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)?;
        let mut records = Vec::new();

        for (idx, line) in content.split_inclusive('\n').enumerate() {
            if !line.ends_with('\n') {
                break;
            }

            let line = line.trim_end_matches(['\r', '\n']);
            if idx == 0 && line == JOURNAL_MAGIC {
                continue;
            }
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<SequencedRecord<T>>(&line) {
                Ok(record) => records.push(record),
                Err(err) if err.is_eof() => break,
                Err(err) => return Err(err.into()),
            }
        }

        Ok(records)
    }

    fn valid_prefix_len(path: &Path) -> Result<u64, IdeviceError> {
        let content = fs::read_to_string(path)?;
        let mut offset = 0usize;

        for (idx, line) in content.split_inclusive('\n').enumerate() {
            if !line.ends_with('\n') {
                break;
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if idx == 0 && trimmed == JOURNAL_MAGIC {
                offset += line.len();
                continue;
            }
            if trimmed.trim().is_empty() {
                offset += line.len();
                continue;
            }

            serde_json::from_str::<SequencedRecord<T>>(trimmed)?;
            offset += line.len();
        }

        Ok(offset as u64)
    }

    #[allow(dead_code)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum TestRecord {
        Begin { tx_id: String },
        CommitReady,
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "idevice-mobilebackup2-journal-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn append_only_journal_replays_complete_records_and_ignores_partial_tail() {
        let root = temp_dir("jsonl");
        let journal_path = root.join("journal.jsonl");
        let mut journal = JsonLineJournal::create(&journal_path).unwrap();

        journal
            .append(&TestRecord::Begin {
                tx_id: "tx-1".into(),
            })
            .unwrap();
        journal.append(&TestRecord::CommitReady).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&journal_path)
            .unwrap()
            .write_all(b"{\"seq\":")
            .unwrap();

        let records = JsonLineJournal::<TestRecord>::replay(&journal_path).unwrap();

        assert_eq!(
            records,
            vec![
                SequencedRecord {
                    seq: 1,
                    record: TestRecord::Begin {
                        tx_id: "tx-1".into()
                    }
                },
                SequencedRecord {
                    seq: 2,
                    record: TestRecord::CommitReady
                }
            ]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_truncates_partial_tail_before_appending_next_record() {
        let root = temp_dir("truncate");
        let journal_path = root.join("journal.jsonl");
        let mut journal = JsonLineJournal::create(&journal_path).unwrap();
        journal
            .append(&TestRecord::Begin {
                tx_id: "tx-1".into(),
            })
            .unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&journal_path)
            .unwrap()
            .write_all(b"{\"seq\":")
            .unwrap();

        let mut reopened = JsonLineJournal::create(&journal_path).unwrap();
        reopened.append(&TestRecord::CommitReady).unwrap();

        let records = JsonLineJournal::<TestRecord>::replay(&journal_path).unwrap();

        assert_eq!(
            records,
            vec![
                SequencedRecord {
                    seq: 1,
                    record: TestRecord::Begin {
                        tx_id: "tx-1".into()
                    }
                },
                SequencedRecord {
                    seq: 2,
                    record: TestRecord::CommitReady
                }
            ]
        );

        fs::remove_dir_all(root).unwrap();
    }
}
