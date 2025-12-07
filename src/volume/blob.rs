//! Blob storage with segmented append-only logs
//!
//! Simplified layout:
//! - Segments stored under `data/segments/seg_0000.blob`
//! - In-memory index: Index
//! - Bloom filter for fast negative lookups (stores raw blake3 32-byte digest)
//! - WAL for durability of delete/put intent (replay applies deletes; index locations come from snapshots or scanning segments)
//! - Snapshot for fast restarts
use crate::common::{blake3_hash, blob_prefix, crc32, Result, WalSyncPolicy};
use crate::volume::index::{BlobLocation, Index};
use crate::volume::wal::{Wal, WalOp};
use bloomfilter::Bloom;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BLOB_MAGIC: [u8; 4] = [0x42, 0x4C, 0x4F, 0x42];
const SEGMENT_SIZE: u64 = 64 * 1024 * 1024;
const MAX_SEGMENTS: u64 = 1000;

#[derive(Debug)]
struct BlobRecord {
    key: String,
    value: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_keys: usize,
    pub total_bytes: u64,
    pub active_segments: usize,
    pub index_size: usize,
    pub bloom_false_positives: u64,
}

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
    pub fn open(data_path: &Path, wal_path: &Path, sync_policy: WalSyncPolicy) -> Result<Self> {
        fs::create_dir_all(data_path)?;
        fs::create_dir_all(wal_path)?;

        let segments_dir = data_path.join("segments");
        fs::create_dir_all(&segments_dir)?;

        let snapshot_path = data_path.join("index.snap");
        let mut index = if snapshot_path.exists() {
            tracing::info!("Loading index snapshot from {:?}", snapshot_path);
            Index::load_snapshot(&snapshot_path)?
        } else {
            tracing::info!("No snapshot, building index from segments");
            Index::new()
        };

        let bloom_path = data_path.join("bloom.filter");
        let mut bloom = if bloom_path.exists() {
            let mut f = File::open(&bloom_path)?;
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes)?;
            Bloom::from_bytes(&bytes).unwrap_or_else(|_| {
                Bloom::new_for_fp_rate(100_000, 0.01).expect("Failed to create bloom filter")
            })
        } else {
            Bloom::new_for_fp_rate(100_000, 0.01).expect("Failed to create bloom filter")
        };

        // If no snapshot, rebuild index (and bloom) by scanning segments.
        if !snapshot_path.exists() {
            Self::rebuild_index_from_segments(&mut index, &mut bloom, &segments_dir)?;
        } else {
            // Ensure bloom matches index keys.
            for key in index.keys() {
                let hash = blake3_hash(key.as_bytes());
                let mut hash_bytes = [0u8; 32];
                let raw = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
                hash_bytes.copy_from_slice(&raw[..32]);
                bloom.set(&hash_bytes);
            }
        }

        // Open wal and replay deletes only (we don't have locations in WAL to reconstruct index)
        let wal_file = wal_path.join("wal.log");
        let mut wal = Wal::open(&wal_file, sync_policy)?;
        if wal_file.exists() {
            tracing::info!("Replaying WAL (applying deletes) {:?}", wal_file);
            Wal::replay(&wal_file, |entry| {
                match entry.op {
                    WalOp::Delete { ref key } => {
                        index.remove(key);
                    }
                    WalOp::Put { .. } => {
                        // Put operations in WAL may not contain segment location; ignore
                    }
                }
                Ok(())
            })?;
        }

        let (current_segment, current_offset) = Self::find_current_position(&segments_dir)?;
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

    pub fn put(&mut self, key: &str, value: &[u8]) -> Result<()> {
        self.wal.append_put(key, value)?;

        let hash = blake3_hash(key.as_bytes());
        let mut hash_bytes = [0u8; 32];
        let raw = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
        hash_bytes.copy_from_slice(&raw[..32]);
        self.bloom.set(&hash_bytes);

        let location = self.write_blob(key, value)?;
        self.index.insert(key.to_string(), location);

        if self.sync_policy == WalSyncPolicy::Always {
            let bloom_path = self.data_path.join("bloom.filter");
            let mut f = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&bloom_path)?;
            let bytes = self.bloom.to_bytes();
            f.write_all(&bytes)?;
            f.sync_all()?;
        }

        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let hash = blake3_hash(key.as_bytes());
        let mut hash_bytes = [0u8; 32];
        let raw = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
        hash_bytes.copy_from_slice(&raw[..32]);

        if !self.bloom.check(&hash_bytes) {
            return Ok(None);
        }

        let location = match self.index.get(key) {
            Some(loc) => loc,
            None => return Ok(None),
        };

        self.read_blob(location)
    }

    pub fn delete(&mut self, key: &str) -> Result<()> {
        self.wal.append_delete(key)?;
        self.index.remove(key);
        Ok(())
    }

    pub fn compact(&mut self) -> Result<()> {
        tracing::info!("Starting compaction");
        let segments_dir = self.data_path.join("segments");
        let temp_dir = self.data_path.join("compact_tmp");
        let backup_dir = self.data_path.join("compact_old");

        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }
        fs::create_dir_all(&temp_dir)?;

        let mut new_index = Index::new();
        let mut new_segment = 0u64;
        let mut new_offset = 0u64;

        for (key, old_location) in self.index.iter() {
            if let Ok(Some(value)) = self.read_blob(old_location) {
                let location =
                    self.write_blob_to_segment(&temp_dir, new_segment, new_offset, key, &value)?;

                new_index.insert(key.clone(), location.clone());

                let record_size = 4 + 4 + 8 + key.len() as u64 + value.len() as u64 + 4;
                new_offset = location.offset + record_size;

                if new_offset > SEGMENT_SIZE {
                    new_segment += 1;
                    new_offset = 0;
                }
            }
        }

        // Atomic swap: rename existing data to backup, temp to data, then remove backup
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)?;
        }
        fs::rename(&segments_dir, &backup_dir)?;
        fs::rename(&temp_dir, &segments_dir)?;

        self.index = new_index;
        self.current_segment = new_segment;
        self.current_offset = new_offset;

        self.save_snapshot()?;
        self.wal.truncate()?;

        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)?;
        }

        tracing::info!("Compaction complete: {} keys", self.index.len());
        Ok(())
    }

    pub fn save_snapshot(&self) -> Result<()> {
        let snapshot_path = self.data_path.join("index.snap");
        self.index.save_snapshot(&snapshot_path)?;
        tracing::info!("Index snapshot saved: {} keys", self.index.len());
        Ok(())
    }

    pub fn stats(&self) -> StoreStats {
        let total_bytes: u64 = self.index.iter().map(|(_, loc)| loc.size).sum();

        StoreStats {
            total_keys: self.index.len(),
            total_bytes,
            active_segments: (self.current_segment + 1) as usize,
            index_size: self.index.len(),
            bloom_false_positives: 0,
        }
    }

    fn write_blob(&mut self, key: &str, value: &[u8]) -> Result<BlobLocation> {
        if self.current_offset > SEGMENT_SIZE {
            self.current_segment += 1;
            self.current_offset = 0;

            if self.current_segment >= MAX_SEGMENTS {
                return Err(crate::Error::Internal("Max segments reached".into()));
            }
        }

        let location = self.write_blob_to_segment(
            &self.data_path.join("segments"),
            self.current_segment,
            self.current_offset,
            key,
            value,
        )?;

        let record_size = 4 + 4 + 8 + key.len() as u64 + value.len() as u64 + 4;
        self.current_offset = location.offset + record_size;

        Ok(location)
    }

    fn write_blob_to_segment(
        &self,
        base_path: &Path,
        segment: u64,
        offset: u64,
        key: &str,
        value: &[u8],
    ) -> Result<BlobLocation> {
        let segment_dir = base_path;
        fs::create_dir_all(&segment_dir)?;

        let segment_file = segment_dir.join(format!("seg_{:04}.blob", segment));

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&segment_file)?;

        file.seek(SeekFrom::Start(offset))?;

        let mut writer = BufWriter::new(&file);

        writer.write_all(&BLOB_MAGIC)?;
        writer.write_all(&(key.len() as u32).to_le_bytes())?;
        writer.write_all(&(value.len() as u64).to_le_bytes())?;

        writer.write_all(key.as_bytes())?;
        writer.write_all(value)?;

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

    fn read_blob(&self, location: &BlobLocation) -> Result<Option<Vec<u8>>> {
        let segment_file = self
            .data_path
            .join("segments")
            .join(format!("seg_{:04}.blob", location.shard));

        if !segment_file.exists() {
            return Ok(None);
        }

        let file = File::open(&segment_file)?;
        let mut reader = BufReader::new(file);

        reader.seek(SeekFrom::Start(location.offset))?;

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

        let mut key_bytes = vec![0u8; key_len];
        reader.read_exact(&mut key_bytes)?;

        let mut value = vec![0u8; val_len];
        reader.read_exact(&mut value)?;

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

    fn rebuild_index_from_segments(
        index: &mut Index,
        bloom: &mut Bloom<[u8; 32]>,
        segments_dir: &Path,
    ) -> Result<()> {
        tracing::info!("Rebuilding index from segments in {:?}", segments_dir);

        if !segments_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(segments_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("blob") {
                continue;
            }
            Self::scan_segment(index, bloom, &path)?;
        }

        Ok(())
    }

    fn scan_segment(index: &mut Index, bloom: &mut Bloom<[u8; 32]>, path: &Path) -> Result<()> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut offset = 0u64;

        let segment = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("seg_"))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        loop {
            let mut magic = [0u8; 4];
            match reader.read_exact(&mut magic) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            if magic != BLOB_MAGIC {
                break;
            }

            let mut key_len_bytes = [0u8; 4];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u32::from_le_bytes(key_len_bytes) as usize;

            let mut val_len_bytes = [0u8; 8];
            reader.read_exact(&mut val_len_bytes)?;
            let val_len = u64::from_le_bytes(val_len_bytes) as usize;

            let mut key_bytes = vec![0u8; key_len];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8_lossy(&key_bytes).to_string();

            reader.seek(SeekFrom::Current(val_len as i64))?;

            let mut checksum_bytes = [0u8; 4];
            reader.read_exact(&mut checksum_bytes)?;

            let hash = blake3_hash(key.as_bytes());
            let mut hash_bytes = [0u8; 32];
            let raw = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
            hash_bytes.copy_from_slice(&raw[..32]);
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

    fn find_current_position(segments_dir: &Path) -> Result<(u64, u64)> {
        let mut max_segment = 0u64;
        let mut max_offset = 0u64;

        if !segments_dir.exists() {
            return Ok((0, 0));
        }

        for entry in fs::read_dir(segments_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("blob") {
                continue;
            }

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
            let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
            store.put("key1", b"value1").unwrap();
            store.save_snapshot().unwrap();
        }

        {
            let store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
            assert_eq!(store.get("key1").unwrap().unwrap(), b"value1");
        }
    }
}
