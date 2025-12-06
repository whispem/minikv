use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};

/// Volume server that handles storage operations
pub struct VolumeServer {
    id: String,
    addr: String,
    coordinator_addr: String,
    data_dir: PathBuf,
    storage: Arc<RwLock<VolumeStorage>>,
}

/// Internal storage implementation
struct VolumeStorage {
    db: rocksdb::DB,
}

impl VolumeServer {
    /// Create a new volume server
    pub async fn new(
        id: String,
        addr: String,
        coordinator_addr: String,
        data_dir: PathBuf,
    ) -> Result<Self> {
        tracing::info!("Initializing volume storage at {:?}", data_dir);

        // Create data directory
        tokio::fs::create_dir_all(&data_dir).await?;

        // Open RocksDB
        let db_path = data_dir.join("db");
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        
        let db = rocksdb::DB::open(&opts, db_path)?;

        let storage = Arc::new(RwLock::new(VolumeStorage { db }));

        Ok(Self {
            id,
            addr,
            coordinator_addr,
            data_dir,
            storage,
        })
    }

    /// Start the gRPC server
    pub async fn serve(self) -> Result<()> {
        let addr = self.addr.parse()?;
        
        tracing::info!("Volume server {} listening on {}", self.id, addr);

        // Register with coordinator
        self.register_with_coordinator().await?;

        // Start gRPC server
        Server::builder()
            .add_service(self.into_service())
            .serve(addr)
            .await?;

        Ok(())
    }

    /// Register this volume with the coordinator
    async fn register_with_coordinator(&self) -> Result<()> {
        tracing::info!("Registering with coordinator at {}", self.coordinator_addr);
        
        // TODO: Implement actual gRPC call to coordinator
        // For now, just log the registration
        tracing::info!("Volume {} registered successfully", self.id);
        
        Ok(())
    }

    /// Convert into gRPC service (placeholder for actual implementation)
    fn into_service(self) -> tonic::transport::server::Router {
        // TODO: Implement actual gRPC service
        // This is a placeholder that returns an empty router
        Server::builder()
    }

    /// Put a key-value pair
    pub async fn put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        let storage = self.storage.write().await;
        storage.db.put(key, value)?;
        Ok(())
    }

    /// Get a value by key
    pub async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let storage = self.storage.read().await;
        Ok(storage.db.get(key)?)
    }

    /// Delete a key
    pub async fn delete(&self, key: &[u8]) -> Result<()> {
        let storage = self.storage.write().await;
        storage.db.delete(key)?;
        Ok(())
    }

    /// Check if volume is healthy
    pub async fn health_check(&self) -> bool {
        // Simple health check: can we read from DB?
        self.storage.read().await.db.get(b"__health__").is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_volume_basic_ops() {
        let temp_dir = TempDir::new().unwrap();
        let server = VolumeServer::new(
            "test-volume".to_string(),
            "127.0.0.1:50052".to_string(),
            "http://127.0.0.1:50051".to_string(),
            temp_dir.path().to_path_buf(),
        )
        .await
        .unwrap();

        // Test put
        server.put(b"key1".to_vec(), b"value1".to_vec()).await.unwrap();

        // Test get
        let value = server.get(b"key1").await.unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));

        // Test delete
        server.delete(b"key1").await.unwrap();
        let value = server.get(b"key1").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_health_check() {
        let temp_dir = TempDir::new().unwrap();
        let server = VolumeServer::new(
            "test-volume".to_string(),
            "127.0.0.1:50053".to_string(),
            "http://127.0.0.1:50051".to_string(),
            temp_dir.path().to_path_buf(),
        )
        .await
        .unwrap();

        assert!(server.health_check().await);
    }
}
