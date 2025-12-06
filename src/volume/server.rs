use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use crate::volume::blob::BlobStore;

pub struct VolumeServer {
    store: Arc<Mutex<BlobStore>>,
}

impl VolumeServer {
    pub fn new(path: PathBuf) -> Self {
        let store = Arc::new(Mutex::new(BlobStore::new(path)));
        Self { store }
    }

    pub async fn serve(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Volume server running...");
        Ok(())
    }
}