//! In-memory index for fast key lookups
//!
//! Maps: key → BlobLocation (shard, offset, size, blake3)

use crate::common::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const SNAPSHOT_MAGIC: &[u8; 8] = b"KVINDEX2";

/// Blob location metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobLocation {
    pub shard: u64,
    pub offset: u64,
    pub size: u64,
    pub blake3: String,
}

/// In-memory index
#[derive(Debug, Default)]
pub struct Index {
    map: HashMap<String, BlobLocation>,
}

impl Index {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Insert or update key
    pub fn insert(&mut self, key: String, location: BlobLocation) {
        self.map.insert(key, location);
    }

    /// Get location for key
    pub fn get(&self, key: &str) -> Option<&BlobLocation> {
        self.map.get(key)
    }

    /// Remove key
    pub fn remove(&mut self, key: &str) -> Option<BlobLocation> {
        self.map.remove(key)
    }

    /// Check if key exists
    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Number of keys
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Iterate over all keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }

    /// Iterate over all entries
    pub fn iter(&self) -> impl Iterator<Item = (&String, &BlobLocation)> {
        self. map.iter()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self. map.clear();
    }

    /// Save index snapshot to file
    pub fn save_snapshot(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        // Write magic
        writer.write_all(SNAPSHOT_MAGIC)?;

        // Write number of entries
        writer.write_all(&(self.map.len() as u64).to_le_bytes())?;

        // Write each entry
        for (key, loc) in &self.map {
            // Key length + key
            let key_bytes = key.as_bytes();
            writer.write_all(&(key_bytes.len() as u32).to_le_bytes())? ;
            writer.write_all(key_bytes)?;

            // Location data
            writer.write_all(&loc.shard.to_le_bytes())?;
            writer.write_all(&loc.offset.to_le_bytes())? ;
            writer.write_all(&loc.size.to_le_bytes())?;

            // BLAKE3 hash length + hash
            let hash_bytes = loc.blake3. as_bytes();
            writer. write_all(&(hash_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(hash_bytes)?;
        }

        writer.flush()?;
        Ok(())
    }

    /// Load index snapshot from file
    pub fn load_snapshot(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        // Read and verify magic
        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        if &magic != SNAPSHOT_MAGIC {
            return Err(crate::Error::Corrupted("Invalid snapshot magic". into()));
        }

        // Read number of entries
        let mut num_entries_bytes = [0u8; 8];
        reader.read_exact(&mut num_entries_bytes)? ;
        let num_entries = u64::from_le_bytes(num_entries_bytes);

        let mut index = Index::new();

        // Read each entry
        for _ in 0..num_entries {
            // Read key
            let mut key_len_bytes = [0u8; 4];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u32::from_le_bytes(key_len_bytes) as usize;

            let mut key_bytes = vec![0u8; key_len];
            reader. read_exact(&mut key_bytes)?;
            let key = String::from_utf8(key_bytes)
                .map_err(|_| crate::Error::Corrupted("Invalid UTF-8 in key".into()))?;

            // Read location
            let mut shard_bytes = [0u8; 8];
            reader. read_exact(&mut shard_bytes)?;
            let shard = u64::from_le_bytes(shard_bytes);

            let mut offset_bytes = [0u8; 8];
            reader.read_exact(&mut offset_bytes)?;
            let offset = u64::from_le_bytes(offset_bytes);

            let mut size_bytes = [0u8; 8];
            reader.read_exact(&mut size_bytes)?;
            let size = u64::from_le_bytes(size_bytes);

            // Read BLAKE3 hash
            let mut hash_len_bytes = [0u8; 4];
            reader.read_exact(&mut hash_len_bytes)?;
            let hash_len = u32::from_le_bytes(hash_len_bytes) as usize;

            let mut hash_bytes = vec![0u8; hash_len];
            reader.read_exact(&mut hash_bytes)?;
            let blake3 = String::from_utf8(hash_bytes)
                .map_err(|_| crate::Error::Corrupted("Invalid UTF-8 in hash".into()))?;

            index.insert(
                key,
                BlobLocation {
                    shard,
                    offset,
                    size,
                    blake3,
                },
            );
        }

        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::blake3_hash;
    use tempfile::tempdir;

    #[test]
    fn test_index_basic() {
        let mut index = Index::new();

        index.insert(
            "key1".to_string(),
            BlobLocation {
                shard: 0,
                offset: 100,
                size: 1024,
                blake3: "abc123".to_string(),
            },
        );

        assert_eq!(index.len(), 1);
        assert!(index.contains("key1"));

        let loc = index.get("key1"). unwrap();
        assert_eq!(loc.shard, 0);
        assert_eq!(loc.offset, 100);
        assert_eq!(loc.size, 1024);

        index.remove("key1");
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let dir = tempdir().unwrap();
        let snapshot_path = dir.path().join("index.snap");

        let mut index = Index::new();
        index.insert(
            "key1".to_string(),
            BlobLocation {
                shard: 0,
                offset: 100,
                size: 1024,
                blake3: blake3_hash(b"data1"),
            },
        );
        index.insert(
            "key2".to_string(),
            BlobLocation {
                shard: 1,
                offset: 200,
                size: 2048,
                blake3: blake3_hash(b"data2"),
            },
        );

        // Save
        index.save_snapshot(&snapshot_path).unwrap();

        // Load
        let loaded = Index::load_snapshot(&snapshot_path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains("key1"));
        assert!(loaded.contains("key2"));

        let loc1 = loaded.get("key1").unwrap();
        assert_eq!(loc1.offset, 100);
    }
}
