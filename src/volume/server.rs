//! Volume server

use crate::common::{Result, VolumeConfig};
use crate::volume::blob::BlobStore;
use crate::volume::compaction::CompactionManager;
use crate::volume::grpc::VolumeGrpcService;
use crate::volume::http::{create_router, VolumeState};
use std::sync::{Arc, Mutex};

pub struct VolumeServer {
    config: VolumeConfig,
    volume_id: String,
}

impl VolumeServer {
    pub fn new(config: VolumeConfig, volume_id: String) -> Self {
        Self { config, volume_id }
    }

    pub async fn serve(self) -> Result<()> {
        tracing::info!("Starting volume server: {}", self.volume_id);
        tracing::info!("  HTTP API: {}", self.config.bind_addr);
        tracing::info!("  gRPC API: {}", self.config.grpc_addr);
        tracing::info!("  Data path: {}", self.config.data_path.display());
        tracing::info!("  WAL path: {}", self.config.wal_path.display());

        // Initialize blob store
        let store = Arc::new(Mutex::new(BlobStore::open(
            &self.config.data_path,
            &self.config.wal_path,
            self.config.wal_sync,
        )?));

        // Start background compaction
        let compaction = CompactionManager::new(
            store.clone(),
            self.config.compaction_interval_secs,
            self.config.compaction_threshold,
        );
        let _compaction_handle = compaction.start();

        // Create HTTP server
        let http_state = VolumeState {
            store: store.clone(),
            volume_id: self.volume_id.clone(),
        };
        let http_router = create_router(
            http_state,
            self.config.max_blob_size as usize / (1024 * 1024),
        );

        // Create gRPC server
        let grpc_service = VolumeGrpcService::new(store, self.volume_id.clone());
        let grpc_server = tonic::transport::Server::builder()
            .add_service(grpc_service.into_server())
            .serve(self.config.grpc_addr);

        // Start servers
        let http_listener = tokio::net::TcpListener::bind(self.config.bind_addr).await?;
        let http_server = axum::serve(http_listener, http_router);

        tracing::info!("✓ Volume server ready");

        tokio::select! {
            res = http_server => {
                if let Err(e) = res {
                    tracing::error!("HTTP server error: {}", e);
                }
            }
            res = grpc_server => {
                if let Err(e) = res {
                    tracing::error!("gRPC server error: {}", e);
                }
            }
        }

        Ok(())
    }
}
