//! In-memory index for fast key lookups
//!
//! This module provides a fast, in-memory HashMap index for key-value lookups.
//! Each key maps to a BlobLocation, which describes where the value is stored on disk.
//! The index can be saved to a snapshot file and loaded back; the volume
//! server rebuilds it from the segments and the WAL, and only takes the
//! expiries from a snapshot.
//! A key can carry an expiry: reads skip expired keys, and `cleanup_expired`
//! removes them.

use crate::common::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const SNAPSHOT_MAGIC: &[u8; 8] = b"KVINDEX3"; // Bumped version for TTL support

/// Describes the physical location of a value in the log-structured storage engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobLocation {
    /// Number of the segment file that holds the record (`seg_NNNN.blob`).
    /// Despite its name, it has nothing to do with sharding: read it with
    /// [`segment`](Self::segment). A future major version will rename it.
    pub shard: u64,
    pub offset: u64,
    /// Size of the value, before compression.
    pub size: u64,
    /// BLAKE3 of the value, in hexadecimal. Empty when the index was rebuilt
    /// from a record that could not be read.
    pub blake3: String,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

impl BlobLocation {
    /// Number of the segment file that holds the record. Same value as the
    /// `shard` field, under its real meaning.
    pub fn segment(&self) -> u64 {
        self.shard
    }
}

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

    pub fn insert(&mut self, key: String, location: BlobLocation) {
        self.map.insert(key, location);
    }

    pub fn get(&self, key: &str) -> Option<&BlobLocation> {
        self.map.get(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<BlobLocation> {
        self.map.remove(key)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &BlobLocation)> {
        self.map.iter()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn is_expired(&self, key: &str) -> bool {
        if let Some(loc) = self.map.get(key) {
            if let Some(expires_at) = loc.expires_at {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                return now > expires_at;
            }
        }
        false
    }

    pub fn get_if_valid(&self, key: &str) -> Option<&BlobLocation> {
        if self.is_expired(key) {
            return None;
        }
        self.map.get(key)
    }

    pub fn cleanup_expired(&mut self) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let expired_keys: Vec<String> = self
            .map
            .iter()
            .filter(|(_, loc)| loc.expires_at.map(|exp| now > exp).unwrap_or(false))
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired_keys.len();
        for key in expired_keys {
            self.map.remove(&key);
        }
        count
    }

    pub fn keys_with_ttl(&self) -> Vec<(&String, u64)> {
        self.map
            .iter()
            .filter_map(|(k, loc)| loc.expires_at.map(|exp| (k, exp)))
            .collect()
    }

    pub fn save_snapshot(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        writer.write_all(SNAPSHOT_MAGIC)?;

        writer.write_all(&(self.map.len() as u64).to_le_bytes())?;

        for (key, loc) in &self.map {
            let key_bytes = key.as_bytes();
            writer.write_all(&(key_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(key_bytes)?;

            writer.write_all(&loc.shard.to_le_bytes())?;
            writer.write_all(&loc.offset.to_le_bytes())?;
            writer.write_all(&loc.size.to_le_bytes())?;

            let hash_bytes = loc.blake3.as_bytes();
            writer.write_all(&(hash_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(hash_bytes)?;

            let expires_at = loc.expires_at.unwrap_or(0);
            writer.write_all(&expires_at.to_le_bytes())?;
        }

        writer.flush()?;
        Ok(())
    }

    pub fn load_snapshot(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        let has_ttl = &magic == b"KVINDEX3";
        if &magic != b"KVINDEX2" && !has_ttl {
            return Err(crate::Error::Corrupted("Invalid snapshot magic".into()));
        }

        let mut num_entries_bytes = [0u8; 8];
        reader.read_exact(&mut num_entries_bytes)?;
        let num_entries = u64::from_le_bytes(num_entries_bytes);

        let mut index = Index::new();

        for _ in 0..num_entries {
            let mut key_len_bytes = [0u8; 4];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u32::from_le_bytes(key_len_bytes) as usize;

            let mut key_bytes = vec![0u8; key_len];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8(key_bytes)
                .map_err(|_| crate::Error::Corrupted("Invalid UTF-8 in key".into()))?;

            let mut shard_bytes = [0u8; 8];
            reader.read_exact(&mut shard_bytes)?;
            let shard = u64::from_le_bytes(shard_bytes);

            let mut offset_bytes = [0u8; 8];
            reader.read_exact(&mut offset_bytes)?;
            let offset = u64::from_le_bytes(offset_bytes);

            let mut size_bytes = [0u8; 8];
            reader.read_exact(&mut size_bytes)?;
            let size = u64::from_le_bytes(size_bytes);

            let mut hash_len_bytes = [0u8; 4];
            reader.read_exact(&mut hash_len_bytes)?;
            let hash_len = u32::from_le_bytes(hash_len_bytes) as usize;

            let mut hash_bytes = vec![0u8; hash_len];
            reader.read_exact(&mut hash_bytes)?;
            let blake3 = String::from_utf8(hash_bytes)
                .map_err(|_| crate::Error::Corrupted("Invalid UTF-8 in hash".into()))?;

            let expires_at = if has_ttl {
                let mut expires_bytes = [0u8; 8];
                reader.read_exact(&mut expires_bytes)?;
                let ts = u64::from_le_bytes(expires_bytes);
                if ts == 0 {
                    None
                } else {
                    Some(ts)
                }
            } else {
                None
            };

            index.insert(
                key,
                BlobLocation {
                    shard,
                    offset,
                    size,
                    blake3,
                    expires_at,
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
                expires_at: None,
            },
        );

        assert_eq!(index.len(), 1);
        assert!(index.contains("key1"));

        let loc = index.get("key1").unwrap();
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
                expires_at: None,
            },
        );
        index.insert(
            "key2".to_string(),
            BlobLocation {
                shard: 1,
                offset: 200,
                size: 2048,
                blake3: blake3_hash(b"data2"),
                expires_at: Some(9999999999999), // Far future expiration
            },
        );

        index.save_snapshot(&snapshot_path).unwrap();

        let loaded = Index::load_snapshot(&snapshot_path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains("key1"));
        assert!(loaded.contains("key2"));

        let loc1 = loaded.get("key1").unwrap();
        assert_eq!(loc1.offset, 100);
        assert_eq!(loc1.expires_at, None);

        let loc2 = loaded.get("key2").unwrap();
        assert_eq!(loc2.expires_at, Some(9999999999999));
    }

    #[test]
    fn test_ttl_expiration() {
        let mut index = Index::new();

        let past_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 1000; // 1 second in the past

        index.insert(
            "expired_key".to_string(),
            BlobLocation {
                shard: 0,
                offset: 0,
                size: 100,
                blake3: "test".to_string(),
                expires_at: Some(past_time),
            },
        );

        let future_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 60000; // 60 seconds in the future

        index.insert(
            "valid_key".to_string(),
            BlobLocation {
                shard: 0,
                offset: 100,
                size: 100,
                blake3: "test".to_string(),
                expires_at: Some(future_time),
            },
        );

        index.insert(
            "permanent_key".to_string(),
            BlobLocation {
                shard: 0,
                offset: 200,
                size: 100,
                blake3: "test".to_string(),
                expires_at: None,
            },
        );

        assert!(index.is_expired("expired_key"));
        assert!(!index.is_expired("valid_key"));
        assert!(!index.is_expired("permanent_key"));

        assert!(index.get_if_valid("expired_key").is_none());
        assert!(index.get_if_valid("valid_key").is_some());
        assert!(index.get_if_valid("permanent_key").is_some());

        let removed = index.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(index.len(), 2);
        assert!(!index.contains("expired_key"));
        assert!(index.contains("valid_key"));
        assert!(index.contains("permanent_key"));
    }

    #[test]
    fn test_keys_with_ttl() {
        let mut index = Index::new();

        index.insert(
            "key_with_ttl".to_string(),
            BlobLocation {
                shard: 0,
                offset: 0,
                size: 100,
                blake3: "test".to_string(),
                expires_at: Some(12345),
            },
        );

        index.insert(
            "key_without_ttl".to_string(),
            BlobLocation {
                shard: 0,
                offset: 100,
                size: 100,
                blake3: "test".to_string(),
                expires_at: None,
            },
        );

        let keys_with_ttl = index.keys_with_ttl();
        assert_eq!(keys_with_ttl.len(), 1);
        assert_eq!(keys_with_ttl[0].0, "key_with_ttl");
        assert_eq!(keys_with_ttl[0].1, 12345);
    }
}
