//! Blob storage with segmented append-only logs
//!
//! Architecture:
//! - Segmented storage: data/00/ab/key.blob
//! - In-memory index: HashMap<String, BlobLocation>
//! - Bloom filter for fast negative lookups
//! - WAL for durability
//! - Index snapshots for fast restarts

use crate::common::{blake3_hash, blob_prefix, crc32, encode_key, Result, WalSyncPolicy};
use crate::volume::index::{BlobLocation, Index};
use crate::volume::wal::{Wal, WalOp};
use bloomfilter::Bloom;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BLOB_MAGIC: [u8; 4] = [0x42, 0x4C, 0x4F, 0x42]; // "BLOB"
const SEGMENT_SIZE: u64 = 64 * 1024 * 1024; // 64 MB per segment
const MAX_SEGMENTS: u64 = 1000;

/// Blob record format:
/// [MAGIC:4][KEY_LEN:4][VALUE_LEN:8][KEY:n][VALUE:m][CRC32:4]
#[derive(Debug)]
struct BlobRecord {
    key: String,
    value: Vec<u8>,
}

/// Blob storage statistics
#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_keys: usize,
    pub total_bytes: u64,
    pub active_segments: usize,
    pub index_size: usize,
    pub bloom_false_positives: u64,
}

/// Blob store with WAL and index
pub struct BlobStore {
    data_path: PathBuf,
    wal_path: PathBuf,
    index: Index,
    bloom: Bloom<[u8; 32]>,
    wal: Wal,
    current_segment: u64,
    current_offset: u64,
    sync_policy: WalSyncPolicy,
}

impl BlobStore {
    /// Open or create blob store
    pub fn open(data_path: &Path, wal_path: &Path, sync_policy: WalSyncPolicy) -> Result<Self> {
        // Create directories
        fs::create_dir_all(data_path)?;
        fs::create_dir_all(wal_path)?;

        // Try to load index snapshot
        let snapshot_path = data_path.join("index.snap");
        let mut index = if snapshot_path.exists() {
            tracing::info!("Loading index snapshot from {:?}", snapshot_path);
            Index::load_snapshot(&snapshot_path)?
        } else {
            tracing::info!("Building index from segments");
            Index::new()
        };

        // Initialize bloom filter
        let mut bloom: Bloom<[u8; 32]> = Bloom::new_for_fp_rate(100_000, 0.01);

        // Open WAL
        let wal_file = wal_path.join("wal.log");
        let mut wal = Wal::open(&wal_file, sync_policy)?;

        // Replay WAL entries
        tracing::info!("Replaying WAL from {:?}", wal_file);
        Wal::replay(&wal_file, |entry| {
            match entry.op {
                WalOp::Put { ref key, ref value } => {
                    // WAL replay: we don't have the actual location yet
                    // This is just to update the bloom filter
                    let hash = blake3_hash(key.as_bytes());
                    let hash_bytes: [u8; 32] = hex::decode(&hash)
                        .unwrap_or_else(|_| vec![0u8; 32])
                        .try_into()
                        .unwrap_or([0u8; 32]);
                    bloom.set(&hash_bytes);
                }
                WalOp::Delete { ref key } => {
                    index.remove(key);
                }
            }
            Ok(())
        })?;

        // Rebuild index from segments if snapshot is old or missing
        if !snapshot_path.exists() {
            Self::rebuild_index_from_segments(&mut index, &mut bloom, data_path)?;
        } else {
            // Update bloom filter from index
            for key in index.keys() {
                let hash = blake3_hash(key.as_bytes());
                let hash_bytes: [u8; 32] = hex::decode(&hash)
                    .unwrap_or_else(|_| vec![0u8; 32])
                    .try_into()
                    .unwrap_or([0u8; 32]);
                bloom.set(&hash_bytes);
            }
        }

        // Find current segment and offset
        let (current_segment, current_offset) = Self::find_current_position(data_path)?;

        tracing::info!(
            "BlobStore opened: {} keys, segment {}, offset {}",
            index.len(),
            current_segment,
            current_offset
        );

        Ok(Self {
            data_path: data_path.to_path_buf(),
            wal_path: wal_path.to_path_buf(),
            index,
            bloom,
            wal,
            current_segment,
            current_offset,
            sync_policy,
        })
    }

    /// Put a blob
    pub fn put(&mut self, key: &str, value: &[u8]) -> Result<()> {
        // Append to WAL first (durability)
        self.wal.append_put(key, value)?;

        // Update bloom filter
        let hash = blake3_hash(key.as_bytes());
        let hash_bytes: [u8; 32] = hex::decode(&hash)
            .unwrap_or_else(|_| vec![0u8; 32])
            .try_into()
            .unwrap_or([0u8; 32]);
        self.bloom.set(&hash_bytes);

        // Write to segment
        let location = self.write_blob(key, value)?;

        // Update index
        self.index.insert(key.to_string(), location);

        Ok(())
    }

    /// Get a blob
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        // Check bloom filter first (fast negative lookup)
        let hash = blake3_hash(key.as_bytes());
        let hash_bytes: [u8; 32] = hex::decode(&hash)
            .unwrap_or_else(|_| vec![0u8; 32])
            .try_into()
            .unwrap_or([0u8; 32]);

        if !self.bloom.check(&hash_bytes) {
            return Ok(None);
        }

        // Check index
        let location = match self.index.get(key) {
            Some(loc) => loc,
            None => return Ok(None),
        };

        // Read from segment
        self.read_blob(location)
    }

    /// Delete a blob
    pub fn delete(&mut self, key: &str) -> Result<()> {
        // Append to WAL
        self.wal.append_delete(key)?;

        // Remove from index
        self.index.remove(key);

        // Note: bloom filter is not updated (false positives are OK)

        Ok(())
    }

    /// Compact storage (remove deleted blobs)
    pub fn compact(&mut self) -> Result<()> {
        tracing::info!("Starting compaction");

        // Create temporary storage
        let temp_path = self.data_path.join("compact_temp");
        fs::create_dir_all(&temp_path)?;

        // Copy all active blobs to new segments
        let mut new_index = Index::new();
        let mut new_segment = 0u64;
        let mut new_offset = 0u64;

        for (key, old_location) in self.index.iter() {
            // Read old blob
            if let Ok(Some(value)) = self.read_blob(old_location) {
                // Write to new location
                let location = self.write_blob_to_segment(
                    &temp_path,
                    new_segment,
                    new_offset,
                    key,
                    &value,
                )?;

                new_index.insert(key.clone(), location);

                new_offset = location.offset + location.size + 16; // header + data + crc

                // Rotate segment if needed
                if new_offset > SEGMENT_SIZE {
                    new_segment += 1;
                    new_offset = 0;
                }
            }
        }

        // Replace old segments with new ones
        let backup_path = self.data_path.join("compact_backup");
        fs::rename(&self.data_path, &backup_path)?;
        fs::rename(&temp_path, &self.data_path)?;

        // Update state
        self.index = new_index;
        self.current_segment = new_segment;
        self.current_offset = new_offset;

        // Save snapshot
        self.save_snapshot()?;

        // Truncate WAL
        self.wal.truncate()?;

        // Remove backup
        fs::remove_dir_all(&backup_path)?;

        tracing::info!("Compaction complete: {} keys", self.index.len());

        Ok(())
    }

    /// Save index snapshot
    pub fn save_snapshot(&self) -> Result<()> {
        let snapshot_path = self.data_path.join("index.snap");
        self.index.save_snapshot(&snapshot_path)?;
        tracing::info!("Index snapshot saved: {} keys", self.index.len());
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> StoreStats {
        let total_bytes: u64 = self.index.iter().map(|(_, loc)| loc.size).sum();

        StoreStats {
            total_keys: self.index.len(),
            total_bytes,
            active_segments: (self.current_segment + 1) as usize,
            index_size: self.index.len(),
            bloom_false_positives: 0, // TODO: track this
        }
    }

    /// Write blob to current segment
    fn write_blob(&mut self, key: &str, value: &[u8]) -> Result<BlobLocation> {
        // Check if we need to rotate segment
        if self.current_offset > SEGMENT_SIZE {
            self.current_segment += 1;
            self.current_offset = 0;

            if self.current_segment >= MAX_SEGMENTS {
                return Err(crate::Error::Internal("Max segments reached".into()));
            }
        }

        let location = self.write_blob_to_segment(
            &self.data_path,
            self.current_segment,
            self.current_offset,
            key,
            value,
        )?;

        self.current_offset = location.offset + location.size + 16;

        Ok(location)
    }

    /// Write blob to specific segment
    fn write_blob_to_segment(
        &self,
        base_path: &Path,
        segment: u64,
        offset: u64,
        key: &str,
        value: &[u8],
    ) -> Result<BlobLocation> {
        let (dir1, dir2) = blob_prefix(key);
        let segment_dir = base_path.join(&dir1).join(&dir2);
        fs::create_dir_all(&segment_dir)?;

        let segment_file = segment_dir.join(format!("seg_{:04}.blob", segment));

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&segment_file)?;

        file.seek(SeekFrom::Start(offset))?;

        let mut writer = BufWriter::new(&file);

        // Write header
        writer.write_all(&BLOB_MAGIC)?;
        writer.write_all(&(key.len() as u32).to_le_bytes())?;
        writer.write_all(&(value.len() as u64).to_le_bytes())?;

        // Write payload
        writer.write_all(key.as_bytes())?;
        writer.write_all(value)?;

        // Compute and write checksum
        let mut checksum_data = Vec::new();
        checksum_data.extend_from_slice(&(key.len() as u32).to_le_bytes());
        checksum_data.extend_from_slice(&(value.len() as u64).to_le_bytes());
        checksum_data.extend_from_slice(key.as_bytes());
        checksum_data.extend_from_slice(value);

        let checksum = crc32(&checksum_data);
        writer.write_all(&checksum.to_le_bytes())?;

        writer.flush()?;

        if self.sync_policy == WalSyncPolicy::Always {
            file.sync_all()?;
        }

        let blake3 = blake3_hash(value);

        Ok(BlobLocation {
            shard: segment,
            offset,
            size: value.len() as u64,
            blake3,
        })
    }

    /// Read blob from location
    fn read_blob(&self, location: &BlobLocation) -> Result<Option<Vec<u8>>> {
        let (dir1, dir2) = blob_prefix(&format!("seg_{}", location.shard));
        let segment_file = self
            .data_path
            .join(&dir1)
            .join(&dir2)
            .join(format!("seg_{:04}.blob", location.shard));

        if !segment_file.exists() {
            return Ok(None);
        }

        let file = File::open(&segment_file)?;
        let mut reader = BufReader::new(file);

        reader.seek(SeekFrom::Start(location.offset))?;

        // Read header
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;

        if magic != BLOB_MAGIC {
            return Err(crate::Error::Corrupted("Invalid blob magic".into()));
        }

        let mut key_len_bytes = [0u8; 4];
        reader.read_exact(&mut key_len_bytes)?;
        let key_len = u32::from_le_bytes(key_len_bytes) as usize;

        let mut val_len_bytes = [0u8; 8];
        reader.read_exact(&mut val_len_bytes)?;
        let val_len = u64::from_le_bytes(val_len_bytes) as usize;

        // Read key
        let mut key_bytes = vec![0u8; key_len];
        reader.read_exact(&mut key_bytes)?;

        // Read value
        let mut value = vec![0u8; val_len];
        reader.read_exact(&mut value)?;

        // Read and verify checksum
        let mut checksum_bytes = [0u8; 4];
        reader.read_exact(&mut checksum_bytes)?;
        let stored_checksum = u32::from_le_bytes(checksum_bytes);

        let mut checksum_data = Vec::new();
        checksum_data.extend_from_slice(&key_len_bytes);
        checksum_data.extend_from_slice(&val_len_bytes);
        checksum_data.extend_from_slice(&key_bytes);
        checksum_data.extend_from_slice(&value);

        let computed_checksum = crc32(&checksum_data);

        if computed_checksum != stored_checksum {
            return Err(crate::Error::ChecksumMismatch {
                expected: format!("{:08x}", stored_checksum),
                actual: format!("{:08x}", computed_checksum),
            });
        }

        Ok(Some(value))
    }

    /// Rebuild index from segments
    fn rebuild_index_from_segments(
        index: &mut Index,
        bloom: &mut Bloom<[u8; 32]>,
        data_path: &Path,
    ) -> Result<()> {
        tracing::info!("Rebuilding index from segments");

        // Walk through all segment files
        for entry in fs::read_dir(data_path)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }

            for subentry in fs::read_dir(entry.path())? {
                let subentry = subentry?;
                if !subentry.path().is_dir() {
                    continue;
                }

                for file_entry in fs::read_dir(subentry.path())? {
                    let file_entry = file_entry?;
                    let path = file_entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("blob") {
                        Self::scan_segment(index, bloom, &path)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan a segment file and update index
    fn scan_segment(index: &mut Index, bloom: &mut Bloom<[u8; 32]>, path: &Path) -> Result<()> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut offset = 0u64;

        // Extract segment number from filename
        let segment = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("seg_"))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        loop {
            // Try to read magic
            let mut magic = [0u8; 4];
            match reader.read_exact(&mut magic) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            if magic != BLOB_MAGIC {
                break;
            }

            // Read header
            let mut key_len_bytes = [0u8; 4];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u32::from_le_bytes(key_len_bytes) as usize;

            let mut val_len_bytes = [0u8; 8];
            reader.read_exact(&mut val_len_bytes)?;
            let val_len = u64::from_le_bytes(val_len_bytes) as usize;

            // Read key
            let mut key_bytes = vec![0u8; key_len];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8_lossy(&key_bytes).to_string();

            // Skip value
            reader.seek(SeekFrom::Current(val_len as i64))?;

            // Read checksum
            let mut checksum_bytes = [0u8; 4];
            reader.read_exact(&mut checksum_bytes)?;

            // Update index and bloom
            let hash = blake3_hash(key.as_bytes());
            let hash_bytes: [u8; 32] = hex::decode(&hash)
                .unwrap_or_else(|_| vec![0u8; 32])
                .try_into()
                .unwrap_or([0u8; 32]);
            bloom.set(&hash_bytes);

            index.insert(
                key,
                BlobLocation {
                    shard: segment,
                    offset,
                    size: val_len as u64,
                    blake3: hash,
                },
            );

            offset += 4 + 4 + 8 + key_len as u64 + val_len as u64 + 4;
        }

        Ok(())
    }

    /// Find current segment and offset
    fn find_current_position(data_path: &Path) -> Result<(u64, u64)> {
        let mut max_segment = 0u64;
        let mut max_offset = 0u64;

        if !data_path.exists() {
            return Ok((0, 0));
        }

        for entry in fs::read_dir(data_path)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }

            for subentry in fs::read_dir(entry.path())? {
                let subentry = subentry?;
                if !subentry.path().is_dir() {
                    continue;
                }

                for file_entry in fs::read_dir(subentry.path())? {
                    let file_entry = file_entry?;
                    let path = file_entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("blob") {
                        let segment = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| s.strip_prefix("seg_"))
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);

                        let metadata = fs::metadata(&path)?;
                        let size = metadata.len();

                        if segment > max_segment || (segment == max_segment && size > max_offset) {
                            max_segment = segment;
                            max_offset = size;
                        }
                    }
                }
            }
        }

        Ok((max_segment, max_offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_blobstore_put_get() {
        let dir = tempdir().unwrap();
        let data_path = dir.path().join("data");
        let wal_path = dir.path().join("wal");

        let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();

        store.put("key1", b"value1").unwrap();
        store.put("key2", b"value2").unwrap();

        assert_eq!(store.get("key1").unwrap().unwrap(), b"value1");
        assert_eq!(store.get("key2").unwrap().unwrap(), b"value2");
        assert!(store.get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_blobstore_delete() {
        let dir = tempdir().unwrap();
        let data_path = dir.path().join("data");
        let wal_path = dir.path().join("wal");

        let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();

        store.put("key1", b"value1").unwrap();
        assert!(store.get("key1").unwrap().is_some());

        store.delete("key1").unwrap();
        assert!(store.get("key1").unwrap().is_none());
    }

    #[test]
    fn test_blobstore_persistence() {
        let dir = tempdir().unwrap();
        let data_path = dir.path().join("data");
        let wal_path = dir.path().join("wal");

        {
            let mut store =
                BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
            store.put("key1", b"value1").unwrap();
            store.save_snapshot().unwrap();
        }

        {
            let store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
            assert_eq!(store.get("key1").unwrap().unwrap(), b"value1");
        }
    }
}
