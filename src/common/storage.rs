//! A small key-value interface, [`KVStore`], with an in-memory backend and,
//! behind the `rocksdb` and `sled-backend` features, RocksDB and Sled ones.
//!
//! No data path of minikv uses it: the coordinator keeps the metadata in its
//! own RocksDB store, and the volume servers keep the values, S3 objects
//! included, in their segment files. The RocksDB and Sled backends panic on
//! I/O errors.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(feature = "rocksdb")]
use rocksdb::{Options, DB};
#[cfg(feature = "sled")]
use sled;

pub trait KVStore: Send + Sync {
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    fn put(&self, key: &str, value: Vec<u8>);
    fn delete(&self, key: &str);
}

pub struct MemStore {
    map: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemStore {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KVStore for MemStore {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.map.lock().unwrap().get(key).cloned()
    }
    fn put(&self, key: &str, value: Vec<u8>) {
        self.map.lock().unwrap().insert(key.to_string(), value);
    }
    fn delete(&self, key: &str) {
        self.map.lock().unwrap().remove(key);
    }
}

#[cfg(feature = "rocksdb")]
pub struct RocksStore {
    db: DB,
}

#[cfg(feature = "rocksdb")]
impl RocksStore {
    pub fn new(path: &str) -> Self {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path).unwrap();
        Self { db }
    }
}

#[cfg(feature = "rocksdb")]
impl KVStore for RocksStore {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.db.get(key).unwrap()
    }
    fn put(&self, key: &str, value: Vec<u8>) {
        self.db.put(key, value).unwrap();
    }
    fn delete(&self, key: &str) {
        self.db.delete(key).unwrap();
    }
}

#[cfg(feature = "sled")]
pub struct SledStore {
    db: sled::Db,
}

#[cfg(feature = "sled")]
impl SledStore {
    pub fn new(path: &str) -> Self {
        let db = sled::open(path).unwrap();
        Self { db }
    }
}

#[cfg(feature = "sled")]
impl KVStore for SledStore {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.db.get(key).unwrap().map(|ivec| ivec.to_vec())
    }
    fn put(&self, key: &str, value: Vec<u8>) {
        self.db.insert(key, value).unwrap();
    }
    fn delete(&self, key: &str) {
        self.db.remove(key).unwrap();
    }
}

pub struct Storage {
    backend: Arc<dyn KVStore>,
}

impl Storage {
    pub fn new_memory() -> Self {
        Self {
            backend: Arc::new(MemStore::new()),
        }
    }
    #[cfg(feature = "rocksdb")]
    pub fn new_rocks(path: &str) -> Self {
        Self {
            backend: Arc::new(RocksStore::new(path)),
        }
    }
    #[cfg(feature = "sled")]
    pub fn new_sled(path: &str) -> Self {
        Self {
            backend: Arc::new(SledStore::new(path)),
        }
    }
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.backend.get(key)
    }
    pub fn put(&self, key: &str, value: Vec<u8>) {
        self.backend.put(key, value)
    }
    pub fn delete(&self, key: &str) {
        self.backend.delete(key)
    }
}
