//! Volume server implementation
//!
//! This module provides the log-structured, append-only storage engine for data volumes.
//! Each volume uses a BlobStore backed by a Write-Ahead Log (WAL) for durability and fast recovery.

use crate::common::{Result, WalSyncPolicy};
use crate::volume::blob::BlobStore;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// VolumeServer manages a single data volume.
/// It wraps a BlobStore, which provides log-structured, append-only storage.
pub struct VolumeServer {
    #[allow(dead_code)]
    store: Arc<Mutex<BlobStore>>,
}

impl VolumeServer {
    /// Create a new VolumeServer instance.
    /// Initializes the BlobStore and WAL for this volume.
    pub fn new(data_path: PathBuf) -> Result<Self> {
        let wal_path = data_path.with_file_name("wal");
        let store = BlobStore::open(&data_path, &wal_path, WalSyncPolicy::Always)?;
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
        })
    }

    /// Start serving requests for this volume.
    /// In a real deployment, this would start the gRPC/HTTP server for client requests.
    pub async fn serve(&self) -> Result<()> {
        println!("Volume server running...");
        Ok(())
    }
}
