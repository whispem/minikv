use crate::volume::blob::{BlobStore, StoreError};
use std::sync::{Arc, Mutex};
use tonic::{Request, Response, Status};
use tracing;

/// Example gRPC commit function
pub async fn commit_blob(
    store: Arc<Mutex<BlobStore>>,
    key: String,
    data: Vec<u8>,
) -> Result<Response<String>, Status> {
    let mut store = store.lock().unwrap();
    match store.put(&key, &data) {
        Ok(_) => Ok(Response::new("Commit successful".into())),
        Err(e) => {
            tracing::error!("COMMIT FAILED: {:?}", e); // use Debug
            Err(Status::internal(format!("Commit failed: {:?}", e)))
        }
    }
}

/// Example gRPC delete function
pub async fn delete_blob_grpc(
    store: Arc<Mutex<BlobStore>>,
    key: String,
) -> Result<Response<String>, Status> {
    let mut store = store.lock().unwrap();
    match store.delete(&key) {
        Ok(true) => Ok(Response::new("Delete successful".into())),
        Ok(false) => Err(Status::not_found("Blob not found")),
        Err(e) => {
            tracing::error!("DELETE FAILED: {:?}", e); // use Debug
            Err(Status::internal(format!("Delete failed: {:?}", e)))
        }
    }
}
