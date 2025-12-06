//! Integration tests for minikv

use minikv::{
    common::{CoordinatorConfig, VolumeConfig, WalSyncPolicy},
    Coordinator, VolumeServer,
};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

#[tokio::test]
async fn test_volume_persistence() {
    use minikv::volume::blob::BlobStore;

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

#[tokio::test]
async fn test_wal_replay() {
    use minikv::volume::blob::BlobStore;

    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    // Write to WAL
    {
        let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
        store.put("key1", b"value1").unwrap();
        // Don't save snapshot - WAL only
    }

    // Reopen and verify WAL replay
    {
        let store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();
        assert_eq!(store.get("key1").unwrap().unwrap(), b"value1");
    }
}

#[tokio::test]
async fn test_bloom_filter() {
    use minikv::volume::blob::BlobStore;

    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();

    // Write keys
    for i in 0..100 {
        store.put(&format!("key_{}", i), b"value").unwrap();
    }

    // Positive lookup (should exist)
    assert!(store.get("key_50").unwrap().is_some());

    // Negative lookup (bloom filter should speed this up)
    assert!(store.get("nonexistent_key").unwrap().is_none());
}

#[tokio::test]
async fn test_compaction() {
    use minikv::volume::blob::BlobStore;

    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    let mut store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always).unwrap();

    // Write many versions of same keys
    for round in 0..10 {
        for i in 0..50 {
            store
                .put(&format!("key_{}", i), format!("value_{}", round).as_bytes())
                .unwrap();
        }
    }

    let _stats_before = store.stats();

    // Compact
    store.compact().unwrap();

    // Verify data still accessible
    for i in 0..50 {
        let value = store.get(&format!("key_{}", i)).unwrap().unwrap();
        assert_eq!(value, b"value_9");
    }
}
