//! Recovery test for minikv: crash and restart

use minikv::volume::blob::BlobStore;
use tempfile::TempDir;

#[test]
fn test_recovery_after_crash() {
    let dir = TempDir::new().unwrap();
    let data_path = dir.path().join("data");
    let wal_path = dir.path().join("wal");

    // Write data
    {
        let mut store =
            BlobStore::open(&data_path, &wal_path, minikv::common::WalSyncPolicy::Always).unwrap();
        store.put("key_crash", b"value_crash").unwrap();
        store.save_snapshot().unwrap();
    }

    // Simulate crash (drop store)
    // Reopen and verify recovery
    {
        let store =
            BlobStore::open(&data_path, &wal_path, minikv::common::WalSyncPolicy::Always).unwrap();
        assert_eq!(store.get("key_crash").unwrap().unwrap(), b"value_crash");
    }
}
