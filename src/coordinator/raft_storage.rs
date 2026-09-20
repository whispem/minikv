use crate::common::raft::LogEntry;
use crate::common::{crc32, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const RECORD_HEADER: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardState {
    pub term: u64,
    pub voted_for: Option<String>,
}

pub struct RaftStorage {
    dir: PathBuf,
    log_file: File,
    offsets: Vec<u64>,
    end: u64,
}

fn encode_error(e: bincode::Error) -> crate::Error {
    crate::Error::Raft(format!("encoding error: {}", e))
}

impl RaftStorage {
    pub fn open(dir: &Path) -> Result<(Self, HardState, Vec<LogEntry>)> {
        fs::create_dir_all(dir)?;

        let hard_state = match fs::read(dir.join("state")) {
            Ok(bytes) => bincode::deserialize(&bytes)
                .map_err(|e| crate::Error::Raft(format!("corrupted raft state: {}", e)))?,
            Err(e) if e.kind() == ErrorKind::NotFound => HardState::default(),
            Err(e) => return Err(e.into()),
        };

        let log_path = dir.join("log");
        let bytes = match fs::read(&log_path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };

        let mut entries = Vec::new();
        let mut offsets = Vec::new();
        let mut pos = 0usize;
        while pos + RECORD_HEADER <= bytes.len() {
            let len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            let checksum = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
            let start = pos + RECORD_HEADER;
            let Some(body) = bytes.get(start..start + len) else {
                break;
            };
            if crc32(body) != checksum {
                break;
            }
            let Ok(entry) = bincode::deserialize::<LogEntry>(body) else {
                break;
            };
            if entry.index != entries.len() as u64 + 1 {
                break;
            }
            offsets.push(pos as u64);
            entries.push(entry);
            pos = start + len;
        }

        let log_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&log_path)?;
        if (pos as u64) < bytes.len() as u64 {
            tracing::warn!(
                "raft log {} has a torn tail, keeping {} entries",
                log_path.display(),
                entries.len()
            );
            log_file.set_len(pos as u64)?;
            log_file.sync_all()?;
        }

        let storage = Self {
            dir: dir.to_path_buf(),
            log_file,
            offsets,
            end: pos as u64,
        };
        Ok((storage, hard_state, entries))
    }

    pub fn save_state(&self, state: &HardState) -> Result<()> {
        let bytes = bincode::serialize(state).map_err(encode_error)?;
        let tmp = self.dir.join("state.tmp");
        let mut file = File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, self.dir.join("state"))?;
        if let Ok(dir) = File::open(&self.dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    pub fn append(&mut self, entries: &[LogEntry]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut buffer = Vec::new();
        let mut new_offsets = Vec::with_capacity(entries.len());
        for entry in entries {
            let body = bincode::serialize(entry).map_err(encode_error)?;
            new_offsets.push(self.end + buffer.len() as u64);
            buffer.extend_from_slice(&(body.len() as u32).to_le_bytes());
            buffer.extend_from_slice(&crc32(&body).to_le_bytes());
            buffer.extend_from_slice(&body);
        }
        self.log_file.seek(SeekFrom::Start(self.end))?;
        self.log_file.write_all(&buffer)?;
        self.log_file.sync_data()?;
        self.end += buffer.len() as u64;
        self.offsets.extend(new_offsets);
        Ok(())
    }

    pub fn truncate_from(&mut self, index: u64) -> Result<()> {
        let position = index.saturating_sub(1) as usize;
        if position >= self.offsets.len() {
            return Ok(());
        }
        let offset = self.offsets[position];
        self.log_file.set_len(offset)?;
        self.log_file.sync_data()?;
        self.offsets.truncate(position);
        self.end = offset;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn entry(term: u64, index: u64, data: &[u8]) -> LogEntry {
        LogEntry {
            term,
            index,
            data: data.to_vec(),
        }
    }

    #[test]
    fn reopen_restores_state_and_log() {
        let dir = tempdir().unwrap();
        {
            let (mut storage, state, entries) = RaftStorage::open(dir.path()).unwrap();
            assert_eq!(state, HardState::default());
            assert!(entries.is_empty());
            storage
                .save_state(&HardState {
                    term: 3,
                    voted_for: Some("coord-2".into()),
                })
                .unwrap();
            storage
                .append(&[entry(1, 1, b"a"), entry(2, 2, b"b"), entry(3, 3, b"c")])
                .unwrap();
            storage.truncate_from(3).unwrap();
            storage.append(&[entry(3, 3, b"d")]).unwrap();
        }
        let (_, state, entries) = RaftStorage::open(dir.path()).unwrap();
        assert_eq!(state.term, 3);
        assert_eq!(state.voted_for.as_deref(), Some("coord-2"));
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].data, b"d".to_vec());
    }

    #[test]
    fn torn_tail_is_dropped() {
        let dir = tempdir().unwrap();
        {
            let (mut storage, _, _) = RaftStorage::open(dir.path()).unwrap();
            storage
                .append(&[entry(1, 1, b"a"), entry(1, 2, b"b")])
                .unwrap();
        }
        let log_path = dir.path().join("log");
        let len = fs::metadata(&log_path).unwrap().len();
        OpenOptions::new()
            .write(true)
            .open(&log_path)
            .unwrap()
            .set_len(len - 3)
            .unwrap();
        let (mut storage, _, entries) = RaftStorage::open(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        storage.append(&[entry(2, 2, b"c")]).unwrap();
        let (_, _, entries) = RaftStorage::open(dir.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].data, b"c".to_vec());
    }
}
