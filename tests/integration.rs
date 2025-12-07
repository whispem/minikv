use minikv::volume::blob::BlobStore;
use std::path::PathBuf;

#[test]
fn test_store_creation() {
    let data_path = PathBuf::from("/tmp/minikv_test_data");
    let mut store = BlobStore::new(data_path.clone());

    // Add your test logic here
    // Example: store.put("key", b"value").unwrap();
}

#[test]
fn test_store_operations() {
    let data_path = PathBuf::from("/tmp/minikv_test_data");
    let store = BlobStore::new(data_path.clone());

    // Example operations
    // store.put("another_key", b"another_value").unwrap();
    // let value = store.get("another_key").unwrap();
    // assert_eq!(value, b"another_value");
}

#[test]
fn test_multiple_stores() {
    let data_path = PathBuf::from("/tmp/minikv_test_data");
    let mut store1 = BlobStore::new(data_path.clone());
    let store2 = BlobStore::new(data_path);

    // Example: testing isolation or shared data
    // store1.put("key1", b"value1").unwrap();
    // let value = store2.get("key1").unwrap();
    // assert_eq!(value, b"value1");
}
