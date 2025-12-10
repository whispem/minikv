//! Stress test for minikv cluster: high load, latency, throughput

use minikv::volume::blob::BlobStore;
use std::time::Instant;
use tempfile::TempDir;

#[test]
fn stress_write_read() {
    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");
    let mut store =
        BlobStore::open(&data_path, &wal_path, minikv::common::WalSyncPolicy::Never).unwrap();

    let n = 1_000;
    let start = Instant::now();
    for i in 0..n {
        store.put(&format!("key_{}", i), b"stress_value").unwrap();
    }
    let write_time = start.elapsed();

    let start = Instant::now();
    for i in 0..n {
        assert_eq!(
            store.get(&format!("key_{}", i)).unwrap().unwrap(),
            b"stress_value"
        );
    }
    let read_time = start.elapsed();

    println!("Write {} keys: {:?}", n, write_time);
    println!("Read {} keys: {:?}", n, read_time);
    assert!(write_time.as_secs_f64() < 30.0, "Write too slow");
    assert!(read_time.as_secs_f64() < 30.0, "Read too slow");
}
