//! Blob storage with segmented logs and compaction

use crate::common::{blake3_hash, blob_prefix, decode_key, encode_key, Blake3Hasher, Result};
use crate::volume::index::{BlobLocation, Index};
use crate::volume::wal::{Wal, WalOp};
use bloomfilter::Bloom;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Blob storage engine
pub struct BlobStore {
    data_dir: PathBuf,
    index: Index,
    bloom: Bloom<[u8; 32]>,
    wal: Wal,
}

impl BlobStore {
    /// Open or create blob store
    pub fn open(
        data_dir: impl AsRef<Path>,
        wal_path: impl AsRef<Path>,
        wal_sync: crate::common::WalSyncPolicy,
    ) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&data_dir)?;

        let blobs_dir = data_dir.join("blobs");
        fs::create_dir_all(&blobs_dir)?;

        // Try to load index snapshot
        let snapshot_path = data_dir.join("index.snapshot");
        let mut index = if snapshot_path.exists() {
            match Index::load_snapshot(&snapshot_path) {
                Ok(idx) => {
                    tracing::info!("Loaded index snapshot with {} keys", idx.len());
                    idx
                }
                Err(e) => {
                    tracing::warn!("Failed to load snapshot: {}, rebuilding", e);
                    Index::new()
                }
            }
        } else {
            Index::new()
        };

        // Initialize bloom filter with FP rate
        let mut bloom = Bloom::new_for_fp_rate(100_000, 0.01);

        // Replay WAL
        let wal = Wal::open(&wal_path, wal_sync)?;
        Wal::replay(&wal_path, |entry| {
            match entry.op {
                WalOp::Put { key, value } => {
                    let hash = blake3_hash(&value);
                    let location = BlobLocation {
                        shard: 0,
                        offset: 0,
                        size: value.len() as u64,
                        blake3: hash,
                    };
                    index.insert(key.clone(), location);
                    bloom.set(&Self::hash_key(&key));
                }
                WalOp::Delete { key } => {
                    index.remove(&key);
                }
            }
            Ok(())
        })?;

        Ok(Self {
            data_dir,
            index,
            bloom,
            wal,
        })
    }

    /// Put a blob
    pub fn put(&mut self, key: &str, data: &[u8]) -> Result<BlobLocation> {
        // Write to WAL first
        self.wal.append_put(key, data)?;

        // Compute BLAKE3 hash
        let hash = blake3_hash(data);

        // Get blob path
        let blob_path = self.blob_path(key);
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write blob to disk
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&blob_path)?;
        file.write_all(data)?;
        file.sync_all()?;

        // Update index
        let location = BlobLocation {
            shard: 0,
            offset: 0,
            size: data.len() as u64,
            blake3: hash.clone(),
        };
        self.index.insert(key.to_string(), location.clone());

        // Update bloom filter
        self.bloom.set(&Self::hash_key(key));

        Ok(location)
    }

    /// Get a blob
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        // Bloom filter check
        if !self.bloom.check(&Self::hash_key(key)) {
            return Ok(None);
        }

        // Index lookup
        let location = match self.index.get(key) {
            Some(loc) => loc,
            None => return Ok(None),
        };

        // Read from disk
        let blob_path = self.blob_path(key);
        let mut file = match File::open(&blob_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        // Verify size
        if data.len() as u64 != location.size {
            return Err(crate::Error::Corrupted(format!(
                "Size mismatch: expected {}, got {}",
                location.size,
                data.len()
            )));
        }

        // Verify BLAKE3 hash
        let computed_hash = blake3_hash(&data);
        if computed_hash != location.blake3 {
            return Err(crate::Error::ChecksumMismatch {
                expected: location.blake3.clone(),
                actual: computed_hash,
            });
        }

        Ok(Some(data))
    }

    /// Delete a blob
    pub fn delete(&mut self, key: &str) -> Result<bool> {
        // Check if exists
        if !self.index.contains(key) {
            return Ok(false);
        }

        // Write to WAL
        self.wal.append_delete(key)?;

        // Remove from index
        self.index.remove(key);

        // Delete file
        let blob_path = self.blob_path(key);
        if blob_path.exists() {
            fs::remove_file(&blob_path)?;
        }

        Ok(true)
    }

    /// List all keys
    pub fn list_keys(&self) -> Vec<String> {
        self.index.keys().cloned().collect()
    }

    /// Get statistics
    pub fn stats(&self) -> BlobStats {
        let total_size: u64 = self.index.iter().map(|(_, loc)| loc.size).sum();

        BlobStats {
            total_keys: self.index.len(),
            total_bytes: total_size,
        }
    }

    /// Save index snapshot
    pub fn save_snapshot(&self) -> Result<()> {
        let snapshot_path = self.data_dir.join("index.snapshot");
        self.index.save_snapshot(&snapshot_path)?;
        Ok(())
    }

    /// Get blob path for a key
    fn blob_path(&self, key: &str) -> PathBuf {
        let (aa, bb) = blob_prefix(key);
        let encoded = encode_key(key);
        self.data_dir.join("blobs").join(aa).join(bb).join(encoded)
    }

    /// Hash key for bloom filter
    fn hash_key(key: &str) -> [u8; 32] {
        let hash = Sha256::digest(key.as_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(&hash);
        out
    }

    /// Compact (rebuild index, clean up orphans)
    pub fn compact(&mut self) -> Result<()> {
        tracing::info!("Starting compaction");

        // Save snapshot
        self.save_snapshot()?;

        // Truncate WAL
        self.wal.truncate()?;

        tracing::info!("Compaction completed");
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BlobStats {
    pub total_keys: usize,
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_blob_store_basic() {
        let dir = tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let wal_path = dir.path().join("wal.log");

        let mut store =
            BlobStore::open(&data_dir, &wal_path, crate::common::WalSyncPolicy::Always).unwrap();

        // Put
        let data = b"hello world";
        let loc = store.put("test-key", data).unwrap();
        assert_eq!(loc.size, data.len() as u64);

        // Get
        let retrieved = store.get("test-key").unwrap().unwrap();
        assert_eq!(retrieved, data);

        // Delete
        let deleted = store.delete("test-key").unwrap();
        assert!(deleted);

        let after_delete = store.get("test-key").unwrap();
        assert!(after_delete.is_none());
    }

    #[test]
    fn test_blob_store_persistence() {
        let dir = tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let wal_path = dir.path().join("wal.log");

        {
            let mut store =
                BlobStore::open(&data_dir, &wal_path, crate::common::WalSyncPolicy::Always)
                    .unwrap();
            store.put("key1", b"value1").unwrap();
            store.put("key2", b"value2").unwrap();
            store.save_snapshot().unwrap();
        }

        // Reopen
        {
            let store = BlobStore::open(&data_dir, &wal_path, crate::common::WalSyncPolicy::Always)
                .unwrap();
            assert_eq!(store.get("key1").unwrap().unwrap(), b"value1");
            assert_eq!(store.get("key2").unwrap().unwrap(), b"value2");
        }
    }
}
