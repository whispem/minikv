//! Write-Ahead Log (WAL) implementation
//!
//! Ensures durability by writing operations to a log before applying them.
//! WAL format: [MAGIC][SEQUENCE][OP][KEY_LEN][VALUE_LEN][KEY][VALUE][CRC32]
//!
//! This module provides append-only logging for all write and delete operations.
//! On recovery, the log is replayed to restore the latest state.

use crate::common::{crc32, Error, Result, WalSyncPolicy};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const WAL_MAGIC: [u8; 4] = [0x57, 0x41, 0x4C, 0x31]; // "WAL1"
const OP_PUT: u8 = 1;
const OP_DELETE: u8 = 2;

/// WAL entry
/// Represents a single operation in the log, either a write (Put) or a delete.
#[derive(Debug, Clone)]
pub struct WalEntry {
    pub sequence: u64,
    pub op: WalOp,
}

#[derive(Debug, Clone)]
pub enum WalOp {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}

/// Write-Ahead Log
/// Main WAL structure. Handles appending operations and syncing to disk.
pub struct Wal {
    path: PathBuf,
    writer: BufWriter<File>,
    next_sequence: u64,
    sync_policy: WalSyncPolicy,
}

impl Wal {
    /// Open or create WAL file.
    /// If the file exists, finds the last sequence number to continue appending.
    pub fn open(path: impl AsRef<Path>, sync_policy: WalSyncPolicy) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;

        // Find last sequence number by reading entire log
        let next_sequence = Self::find_last_sequence(&path)?;

        Ok(Self {
            path,
            writer: BufWriter::new(file),
            next_sequence,
            sync_policy,
        })
    }

    /// Find the last sequence number in the WAL.
    /// Used during WAL open to determine where to resume.
    fn find_last_sequence(path: &Path) -> Result<u64> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };

        let mut reader = BufReader::new(file);
        let mut max_seq = None;

        loop {
            match Self::read_entry_internal(&mut reader) {
                Ok(Some(entry)) => {
                    max_seq = Some(max_seq.unwrap_or(0).max(entry.sequence));
                }
                Ok(None) => break,
                Err(_) => break, // Corrupted entry, stop reading
            }
        }

        Ok(max_seq.map(|s| s + 1).unwrap_or(0))
    }

    /// Append a PUT operation to the WAL.
    /// Returns the sequence number assigned to this operation.
    pub fn append_put(&mut self, key: &str, value: &[u8]) -> Result<u64> {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        self.write_entry(sequence, OP_PUT, key, Some(value))?;
        self.maybe_sync()?;

        Ok(sequence)
    }

    /// Append a DELETE operation to the WAL.
    /// Returns the sequence number assigned to this operation.
    pub fn append_delete(&mut self, key: &str) -> Result<u64> {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        self.write_entry(sequence, OP_DELETE, key, None)?;
        self.maybe_sync()?;

        Ok(sequence)
    }

    /// Write an entry to the WAL file.
    /// Handles serialization and CRC protection.
    fn write_entry(
        &mut self,
        sequence: u64,
        op: u8,
        key: &str,
        value: Option<&[u8]>,
    ) -> Result<()> {
        let key_bytes = key.as_bytes();
        let val_bytes = value.unwrap_or(&[]);

        // Write header
        self.writer.write_all(&WAL_MAGIC)?;
        self.writer.write_all(&sequence.to_le_bytes())?;
        self.writer.write_all(&[op])?;
        self.writer
            .write_all(&(key_bytes.len() as u32).to_le_bytes())?;
        self.writer
            .write_all(&(val_bytes.len() as u32).to_le_bytes())?;

        // Write payload
        self.writer.write_all(key_bytes)?;
        if op == OP_PUT {
            self.writer.write_all(val_bytes)?;
        }

        // Write checksum
        let mut checksum_data = Vec::new();
        checksum_data.extend_from_slice(&sequence.to_le_bytes());
        checksum_data.push(op);
        checksum_data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
        checksum_data.extend_from_slice(&(val_bytes.len() as u32).to_le_bytes());
        checksum_data.extend_from_slice(key_bytes);
        if op == OP_PUT {
            checksum_data.extend_from_slice(val_bytes);
        }

        let checksum = crc32(&checksum_data);
        self.writer.write_all(&checksum.to_le_bytes())?;

        Ok(())
    }

    /// Sync based on policy
    fn maybe_sync(&mut self) -> Result<()> {
        match self.sync_policy {
            WalSyncPolicy::Always => {
                self.writer.flush()?;
                self.writer.get_ref().sync_all()?;
            }
            WalSyncPolicy::Interval => {
                self.writer.flush()?;
            }
            WalSyncPolicy::Never => {}
        }
        Ok(())
    }

    /// Replay WAL entries
    pub fn replay<F>(path: impl AsRef<Path>, mut callback: F) -> Result<()>
    where
        F: FnMut(WalEntry) -> Result<()>,
    {
        let file = match File::open(path.as_ref()) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        let mut reader = BufReader::new(file);

        loop {
            match Self::read_entry_internal(&mut reader) {
                Ok(Some(entry)) => callback(entry)?,
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!("WAL replay stopped at corrupted entry: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Read a single entry from the WAL
    fn read_entry_internal<R: Read>(reader: &mut R) -> Result<Option<WalEntry>> {
        // Read magic
        let mut magic = [0u8; 4];
        match reader.read_exact(&mut magic) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }

        if magic != WAL_MAGIC {
            return Err(Error::Wal("Invalid WAL magic".into()));
        }

        // Read sequence
        let mut seq_bytes = [0u8; 8];
        reader.read_exact(&mut seq_bytes)?;
        let sequence = u64::from_le_bytes(seq_bytes);

        // Read op
        let mut op = [0u8; 1];
        reader.read_exact(&mut op)?;

        // Read key length
        let mut key_len_bytes = [0u8; 4];
        reader.read_exact(&mut key_len_bytes)?;
        let key_len = u32::from_le_bytes(key_len_bytes) as usize;

        // Read value length
        let mut val_len_bytes = [0u8; 4];
        reader.read_exact(&mut val_len_bytes)?;
        let val_len = u32::from_le_bytes(val_len_bytes) as usize;

        // Read key
        let mut key_bytes = vec![0u8; key_len];
        reader.read_exact(&mut key_bytes)?;
        let key =
            String::from_utf8(key_bytes).map_err(|_| Error::Wal("Invalid UTF-8 in key".into()))?;

        // Read value
        let value = if op[0] == OP_PUT {
            let mut val = vec![0u8; val_len];
            reader.read_exact(&mut val)?;
            Some(val)
        } else {
            None
        };

        // Read checksum
        let mut checksum_bytes = [0u8; 4];
        reader.read_exact(&mut checksum_bytes)?;
        let stored_checksum = u32::from_le_bytes(checksum_bytes);

        // Verify checksum
        let mut checksum_data = Vec::new();
        checksum_data.extend_from_slice(&seq_bytes);
        checksum_data.push(op[0]);
        checksum_data.extend_from_slice(&key_len_bytes);
        checksum_data.extend_from_slice(&val_len_bytes);
        checksum_data.extend_from_slice(key.as_bytes());
        if let Some(ref v) = value {
            checksum_data.extend_from_slice(v);
        }

        let computed_checksum = crc32(&checksum_data);
        if computed_checksum != stored_checksum {
            return Err(Error::Wal("Checksum mismatch".into()));
        }

        let wal_op = match op[0] {
            OP_PUT => WalOp::Put {
                key,
                value: value.unwrap(),
            },
            OP_DELETE => WalOp::Delete { key },
            _ => return Err(Error::Wal(format!("Unknown op code: {}", op[0]))),
        };

        Ok(Some(WalEntry {
            sequence,
            op: wal_op,
        }))
    }

    /// Truncate WAL (after successful compaction)
    pub fn truncate(&mut self) -> Result<()> {
        self.writer.flush()?;
        drop(std::mem::replace(
            &mut self.writer,
            BufWriter::new(File::open(&self.path)?),
        ));

        // Truncate file
        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.path)?;

        self.writer = BufWriter::new(file);
        self.next_sequence = 0;

        Ok(())
    }

    /// Sync to disk
    pub fn sync(&mut self) -> Result<()> {
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_wal_basic() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("test.wal");

        {
            let mut wal = Wal::open(&wal_path, WalSyncPolicy::Always).unwrap();

            let seq1 = wal.append_put("key1", b"value1").unwrap();
            let seq2 = wal.append_put("key2", b"value2").unwrap();
            let seq3 = wal.append_delete("key1").unwrap();

            assert_eq!(seq1, 0);
            assert_eq!(seq2, 1);
            assert_eq!(seq3, 2);

            wal.sync().unwrap();
        }

        // Replay
        let mut entries = Vec::new();
        Wal::replay(&wal_path, |entry| {
            entries.push(entry);
            Ok(())
        })
        .unwrap();

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].sequence, 0);
        assert_eq!(entries[1].sequence, 1);
        assert_eq!(entries[2].sequence, 2);

        match &entries[0].op {
            WalOp::Put { key, value } => {
                assert_eq!(key, "key1");
                assert_eq!(value, b"value1");
            }
            _ => panic!("Expected Put"),
        }
    }

    #[test]
    fn test_wal_reopen() {
        let dir = tempdir().unwrap();
        let wal_path = dir.path().join("reopen.wal");

        {
            let mut wal = Wal::open(&wal_path, WalSyncPolicy::Always).unwrap();
            wal.append_put("key1", b"value1").unwrap();
            wal.append_put("key2", b"value2").unwrap();
            wal.sync().unwrap();
        }

        // Reopen and append more
        {
            let mut wal = Wal::open(&wal_path, WalSyncPolicy::Always).unwrap();
            // After 2 entries (seq 0, 1), next_sequence should be 2
            assert_eq!(wal.next_sequence, 2);
            let seq = wal.append_put("key3", b"value3").unwrap();
            assert_eq!(seq, 2);
            wal.sync().unwrap();
        }

        // Verify all entries
        let mut count = 0;
        Wal::replay(&wal_path, |_| {
            count += 1;
            Ok(())
        })
        .unwrap();

        assert_eq!(count, 3);
    }
}
