use crate::volume::blob::BlobStore;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
