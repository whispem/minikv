//! Integration tests for minikv

use minikv::{
    common::{CoordinatorConfig, VolumeConfig, WalSyncPolicy},
    Coordinator, VolumeServer,
};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Setup a test cluster (1 coordinator + 1 volume)
async fn setup_test_cluster() -> (
    TempDir,
    TempDir,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
) {
    let coord_dir = TempDir::new().unwrap();
    let vol_dir = TempDir::new().unwrap();

    // Coordinator config
    let coord_config = CoordinatorConfig {
        bind_addr: "127.0.0.1:15000".parse().unwrap(),
        grpc_addr: "127.0.0.1:15001".parse().unwrap(),
        db_path: coord_dir.path().join("coord-db"),
        peers: vec![],
        replicas: 1, // Single replica for testing
        ..Default::default()
    };

    // Volume config
    let vol_config = VolumeConfig {
        bind_addr: "127.0.0.1:16000".parse().unwrap(),
        grpc_addr: "127.0.0.1:16001".parse().unwrap(),
        data_path: vol_dir.path().join("data"),
        wal_path: vol_dir.path().join("wal"),
        coordinators: vec!["http://127.0.0.1:15000".to_string()],
        wal_sync: WalSyncPolicy::Always,
        ..Default::default()
    };

    // Start coordinator
    let coord_handle = tokio::spawn(async move {
        let coord = Coordinator::new(coord_config, "test-coord".to_string());
        coord.serve().await.unwrap();
    });

    // Wait for coordinator to start
    sleep(Duration::from_millis(500)).await;

    // Start volume
    let vol_handle = tokio::spawn(async move {
        let vol = VolumeServer::new(vol_config, "test-vol".to_string());
        vol.serve().await.unwrap();
    });

    // Wait for volume to start
    sleep(Duration::from_millis(500)).await;

    (coord_dir, vol_dir, coord_handle, vol_handle)
}

#[tokio::test]
async fn test_cluster_startup() {
    let (_coord_dir, _vol_dir, coord_handle, vol_handle) = setup_test_cluster().await;

    // Check coordinator health
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:15000/health")
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());

    // Check volume health
    let response = client
        .get("http://127.0.0.1:16000/health")
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());

    // Cleanup
    coord_handle.abort();
    vol_handle.abort();
}

#[tokio::test]
#[ignore] // Requires full 2PC implementation
async fn test_put_get_delete() {
    let (_coord_dir, _vol_dir, coord_handle, vol_handle) = setup_test_cluster().await;

    let client = reqwest::Client::new();

    // PUT
    let response = client
        .put("http://127.0.0.1:15000/test-key")
        .body("test data")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);

    // GET
    let response = client
        .get("http://127.0.0.1:15000/test-key")
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let body = response.text().await.unwrap();
    assert_eq!(body, "test data");

    // DELETE
    let response = client
        .delete("http://127.0.0.1:15000/test-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // GET (should be 404)
    let response = client
        .get("http://127.0.0.1:15000/test-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    // Cleanup
    coord_handle.abort();
    vol_handle.abort();
}

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

    let stats_before = store.stats();

    // Compact
    store.compact().unwrap();

    // Verify data still accessible
    for i in 0..50 {
        let value = store.get(&format!("key_{}", i)).unwrap().unwrap();
        assert_eq!(value, b"value_9");
    }
}
