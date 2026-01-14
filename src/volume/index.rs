//! In-memory index for fast key lookups
//!
//! This module provides a fast, in-memory HashMap index for key-value lookups.
//! Each key maps to a BlobLocation, which describes where the value is stored on disk.
//! The index supports snapshotting for fast recovery after a crash.
//! TTL (Time-To-Live) support enables automatic key expiration.

use crate::common::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

const SNAPSHOT_MAGIC: &[u8; 8] = b"KVINDEX3"; // Bumped version for TTL support

/// Blob location metadata
/// Describes the physical location of a value in the log-structured storage engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobLocation {
    pub shard: u64,
    pub offset: u64,
    pub size: u64,
    pub blake3: String,
    /// Optional expiration timestamp in milliseconds since Unix epoch.
    /// If None, the key never expires.
    #[serde(default)]
    pub expires_at: Option<u64>,
}

/// In-memory index
/// HashMap-based index for fast key lookups.
/// Supports saving/loading snapshots for recovery.
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

    /// Insert or update a key in the index.
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
        self.map.iter()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Check if a key is expired.
    /// Returns true if the key exists and has expired.
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

    /// Get location for key, returning None if expired.
    pub fn get_if_valid(&self, key: &str) -> Option<&BlobLocation> {
        if self.is_expired(key) {
            return None;
        }
        self.map.get(key)
    }

    /// Remove all expired keys and return the number of keys removed.
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

    /// Get all keys that have TTL set (for monitoring)
    pub fn keys_with_ttl(&self) -> Vec<(&String, u64)> {
        self.map
            .iter()
            .filter_map(|(k, loc)| loc.expires_at.map(|exp| (k, exp)))
            .collect()
    }

    /// Save the current index as a snapshot file.
    /// Used for fast recovery after restart.
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
            writer.write_all(&(key_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(key_bytes)?;

            // Location data
            writer.write_all(&loc.shard.to_le_bytes())?;
            writer.write_all(&loc.offset.to_le_bytes())?;
            writer.write_all(&loc.size.to_le_bytes())?;

            // BLAKE3 hash length + hash
            let hash_bytes = loc.blake3.as_bytes();
            writer.write_all(&(hash_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(hash_bytes)?;

            // TTL: expires_at (0 = no expiration, >0 = timestamp)
            let expires_at = loc.expires_at.unwrap_or(0);
            writer.write_all(&expires_at.to_le_bytes())?;
        }

        writer.flush()?;
        Ok(())
    }

    /// Load an index snapshot from file.
    /// Returns a new Index instance populated from the snapshot.
    pub fn load_snapshot(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        // Read and verify magic
        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic)?;
        // Support both v2 (KVINDEX2) and v3 (KVINDEX3) formats
        let has_ttl = &magic == b"KVINDEX3";
        if &magic != b"KVINDEX2" && !has_ttl {
            return Err(crate::Error::Corrupted("Invalid snapshot magic".into()));
        }

        // Read number of entries
        let mut num_entries_bytes = [0u8; 8];
        reader.read_exact(&mut num_entries_bytes)?;
        let num_entries = u64::from_le_bytes(num_entries_bytes);

        let mut index = Index::new();

        // Read each entry
        for _ in 0..num_entries {
            // Read key
            let mut key_len_bytes = [0u8; 4];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u32::from_le_bytes(key_len_bytes) as usize;

            let mut key_bytes = vec![0u8; key_len];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8(key_bytes)
                .map_err(|_| crate::Error::Corrupted("Invalid UTF-8 in key".into()))?;

            // Read location
            let mut shard_bytes = [0u8; 8];
            reader.read_exact(&mut shard_bytes)?;
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

            // Read TTL if v3 format
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

        // Save
        index.save_snapshot(&snapshot_path).unwrap();

        // Load
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

        // Key with past expiration (already expired)
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

        // Key with future expiration (not expired)
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

        // Key with no expiration
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

        // Check expiration status
        assert!(index.is_expired("expired_key"));
        assert!(!index.is_expired("valid_key"));
        assert!(!index.is_expired("permanent_key"));

        // get_if_valid should not return expired keys
        assert!(index.get_if_valid("expired_key").is_none());
        assert!(index.get_if_valid("valid_key").is_some());
        assert!(index.get_if_valid("permanent_key").is_some());

        // Cleanup should remove only expired keys
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
