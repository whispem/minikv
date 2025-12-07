use crate::common::{Result, WalSyncPolicy};
use crate::volume::blob::BlobStore;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct VolumeServer {
    store: Arc<Mutex<BlobStore>>,
}

impl VolumeServer {
    pub fn new(data_path: PathBuf) -> Result<Self> {
        let wal_path = data_path.with_file_name("wal");
        let store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always)?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
        })
    }

    pub async fn serve(&self) -> Result<()> {
        println!("Volume server running...");
        Ok(())
    }
}
