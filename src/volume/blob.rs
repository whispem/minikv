//! Blob store implementation

use crate::common::crc32;
use bloomfilter::Bloom;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct BlobStore {
    blobs: HashMap<String, Vec<u8>>,
    bloom: Bloom<[u8; 32]>,
}

#[derive(Debug)]
pub struct BlobLocation {
    pub size: u64,
    pub blake3: String,
}

#[derive(Debug)]
pub enum StoreError {
    KeyTooLarge,
    Other(String),
}

impl BlobStore {
    pub fn new() -> Self {
        Self {
            blobs: HashMap::new(),
            bloom: Bloom::new(1000, 0.01),
        }
    }

    fn hash_key(key: &str) -> [u8; 32] {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(key.as_bytes());
        hasher.finalize().into()
    }

    pub fn stats(&self) -> BlobStats {
        let total_bytes = self.blobs.values().map(|v| v.len() as u64).sum();
        let total_keys = self.blobs.len();
        BlobStats {
            total_keys,
            total_bytes,
        }
    }

    pub fn put(&mut self, key: &str, data: &[u8]) -> Result<BlobLocation, StoreError> {
        self.bloom.set(&Self::hash_key(&key)); // OK, bloom est Bloom directement
        self.blobs.insert(key.to_string(), data.to_vec());

        Ok(BlobLocation {
            size: data.len() as u64,
            blake3: blake3::hash(data).to_hex().to_string(),
        })
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.blobs.get(key).cloned())
    }

    pub fn delete(&mut self, key: &str) -> Result<bool, StoreError> {
        Ok(self.blobs.remove(key).is_some())
    }
}

pub struct BlobStats {
    pub total_keys: usize,
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_get_delete() {
        let mut store = BlobStore::new();
        let key = "mykey";
        let data = b"hello";

        let loc = store.put(key, data).unwrap();
        assert_eq!(loc.size, 5);

        let retrieved = store.get(key).unwrap();
        assert_eq!(retrieved.unwrap(), data);

        let deleted = store.delete(key).unwrap();
        assert!(deleted);

        let not_found = store.get(key).unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_stats() {
        let mut store = BlobStore::new();
        store.put("a", b"123").unwrap();
        store.put("b", b"4567").unwrap();

        let stats = store.stats();
        assert_eq!(stats.total_keys, 2);
        assert_eq!(stats.total_bytes, 7);
    }
}
