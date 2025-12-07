//! Integration tests for minikv

use minikv::{common::WalSyncPolicy, volume::blob::BlobStore};
use tempfile::TempDir;

#[test]
fn test_volume_persistence() {
    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    // Write data
    {
        let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
        store.put("key1", b"value1").unwrap();
        store.put("key2", b"value2").unwrap();
        store.save_snapshot().unwrap();
    }

    // Reopen and verify
    {
        let store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
        assert_eq!(store.get("key1").unwrap().unwrap(), b"value1");
        assert_eq!(store.get("key2").unwrap().unwrap(), b"value2");
    }
}

#[test]
fn test_wal_replay() {
    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    // Write to WAL
    {
        let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
        store.put("key1", b"value1").unwrap();
    }

    // Reopen and verify WAL replay
    {
        let store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
        assert_eq!(store.get("key1").unwrap().unwrap(), b"value1");
    }
}

#[test]
fn test_bloom_filter() {
    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();

    // Write keys
    for i in 0..100 {
        store.put(&format!("key_{}", i), b"value").unwrap();
    }

    // Positive lookup
    assert!(store.get("key_50").unwrap().is_some());

    // Negative lookup (bloom filter)
    assert!(store.get("nonexistent_key").unwrap().is_none());
}

#[test]
fn test_delete() {
    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();

    store.put("key1", b"value1").unwrap();
    assert!(store.get("key1").unwrap().is_some());

    store.delete("key1").unwrap();
    assert!(store.get("key1").unwrap().is_none());
}
