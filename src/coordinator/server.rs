//! Coordinator server

use crate::common::{CoordinatorConfig, Result};
use crate::coordinator::grpc::CoordGrpcService;
use crate::coordinator::http::{create_router, CoordState};
use crate::coordinator::metadata::MetadataStore;
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::{start_raft_tasks, RaftNode};
use std::sync::{Arc, Mutex};

pub struct Coordinator {
    config: CoordinatorConfig,
    node_id: String,
}

impl Coordinator {
    pub fn new(config: CoordinatorConfig, node_id: String) -> Self {
        Self { config, node_id }
    }

    pub async fn serve(self) -> Result<()> {
        tracing::info!("Starting coordinator: {}", self.node_id);
        tracing::info!("  HTTP API: {}", self.config.bind_addr);
        tracing::info!("  gRPC API: {}", self.config.grpc_addr);
        tracing::info!("  DB path: {}", self.config.db_path.display());
        tracing::info!("  Replicas: {}", self.config.replicas);

        // Initialize metadata store
        let metadata = Arc::new(MetadataStore::open(&self.config.db_path)?);

        // Initialize placement manager
        let placement = Arc::new(Mutex::new(PlacementManager::new(
            self.config.num_shards,
            self.config.replicas,
        )));

        // Initialize Raft
        let raft = Arc::new(RaftNode::new(self.node_id.clone()));
        let _raft_handle = start_raft_tasks(raft.clone());

        // Create HTTP server
        let http_state = CoordState {
            metadata: metadata.clone(),
            placement: placement.clone(),
            raft: raft.clone(),
        };
        let http_router = create_router(http_state);

        // Create gRPC server
        let grpc_service = CoordGrpcService::new();
        let grpc_server = tonic::transport::Server::builder()
            .add_service(grpc_service.into_server())
            .serve(self.config.grpc_addr);

        // Start servers
        let http_listener = tokio::net::TcpListener::bind(self.config.bind_addr).await?;
        let http_server = axum::serve(http_listener, http_router);

        tracing::info!("✓ Coordinator ready ({})", raft.get_role());

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
