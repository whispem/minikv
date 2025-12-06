//! Blob store implementation

use bloomfilter::Bloom;
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::sync::{Arc, Mutex};

/// Error type for store operations
#[derive(Debug)]
pub enum StoreError {
    KeyNotFound,
    WriteError,
    ReadError,
}

impl Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::KeyNotFound => write!(f, "Key not found"),
            StoreError::WriteError => write!(f, "Write error"),
            StoreError::ReadError => write!(f, "Read error"),
        }
    }
}

impl std::error::Error for StoreError {}

/// A simple in-memory blob store
pub struct BlobStore {
    pub blobs: HashMap<String, Vec<u8>>,
    pub bloom: Bloom<[u8; 32]>,
}

impl BlobStore {
    /// Create a new blob store
    pub fn new() -> Self {
        Self {
            blobs: HashMap::new(),
            bloom: Bloom::new(1000, 10).expect("Failed to create bloom filter"),
        }
    }

    /// Put a blob
    pub fn put(&mut self, key: &str, data: &[u8]) -> Result<(), StoreError> {
        let hash = Self::hash_key(key);
        self.bloom.set(&hash);
        self.blobs.insert(key.to_string(), data.to_vec());
        Ok(())
    }

    /// Get a blob
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.blobs.get(key).cloned())
    }

    /// Delete a blob
    pub fn delete(&mut self, key: &str) -> Result<bool, StoreError> {
        match self.blobs.remove(key) {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// Return statistics
    pub fn stats(&self) -> BlobStoreStats {
        let total_bytes: u64 = self.blobs.values().map(|v| v.len() as u64).sum();
        BlobStoreStats {
            total_keys: self.blobs.len(),
            total_bytes,
        }
    }

    /// Example hash function for bloom filter
    pub fn hash_key(key: &str) -> [u8; 32] {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(key.as_bytes());
        hasher.finalize().as_bytes().clone()
    }
}

/// Statistics structure
pub struct BlobStoreStats {
    pub total_keys: usize,
    pub total_bytes: u64,
}

