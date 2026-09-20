use axum_server::tls_rustls::{bind_rustls, RustlsConfig};
use std::future::IntoFuture;

use crate::common::{CoordinatorConfig, Result};
use crate::coordinator::grpc::CoordGrpcService;
use crate::coordinator::http::{create_router, CoordState};
use crate::coordinator::metadata::MetadataStore;
use crate::coordinator::objects::ObjectStore;
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::{start_raft_tasks, RaftNode};
use crate::coordinator::state;
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

        let metadata = Arc::new(MetadataStore::open(&self.config.db_path)?);

        let placement = Arc::new(Mutex::new(PlacementManager::new(
            self.config.num_shards,
            self.config.replicas,
        )));

        let raft = Arc::new(RaftNode::with_config(
            self.node_id.clone(),
            self.config.peers.clone(),
            Some(self.config.db_path.join("raft")),
            self.config.election_timeout_ms,
            self.config.heartbeat_interval_ms,
        )?);
        tracing::info!("  Raft peers: {:?}", raft.get_peers());
        {
            let metadata = metadata.clone();
            raft.set_apply_fn(move |entry| state::apply(&metadata, entry));
        }
        let _raft_handle = start_raft_tasks(raft.clone());

        let objects = ObjectStore::new(metadata.clone(), placement.clone(), raft.clone());
        let http_state = CoordState {
            metadata: metadata.clone(),
            placement: placement.clone(),
            raft: raft.clone(),
            objects: objects.clone(),
        };
        let http_router = create_router(http_state);

        let use_tls = self.config.tls_cert_path.is_some() && self.config.tls_key_path.is_some();
        use std::future::Future;
        use std::pin::Pin;
        let http_server: Pin<
            Box<dyn Future<Output = std::result::Result<(), std::io::Error>> + Send>,
        > = if use_tls {
            let cert_path = self.config.tls_cert_path.as_ref().unwrap();
            let key_path = self.config.tls_key_path.as_ref().unwrap();
            let rustls_config = RustlsConfig::from_pem_file(cert_path, key_path)
                .await
                .unwrap();
            Box::pin(
                bind_rustls(self.config.bind_addr, rustls_config)
                    .serve(http_router.clone().into_make_service()),
            )
        } else {
            let http_listener = tokio::net::TcpListener::bind(self.config.bind_addr).await?;
            Box::pin(axum::serve(http_listener, http_router.clone()).into_future())
        };

        let grpc_service = CoordGrpcService::with_raft(raft.clone(), objects);
        let grpc_listener = tokio::net::TcpListener::bind(self.config.grpc_addr).await?;
        let grpc_incoming =
            tonic::transport::server::TcpIncoming::from_listener(grpc_listener, true, None)
                .map_err(|e| crate::Error::Internal(format!("gRPC listener error: {}", e)))?;
        let grpc_server = if let (Some(cert_path), Some(key_path)) = (
            self.config.tls_cert_path.as_ref(),
            self.config.tls_key_path.as_ref(),
        ) {
            use tokio::fs;
            use tonic::transport::{Identity, ServerTlsConfig};
            let cert = fs::read(cert_path).await.expect("Cannot read TLS cert");
            let key = fs::read(key_path).await.expect("Cannot read TLS key");
            let identity = Identity::from_pem(cert, key);
            tonic::transport::Server::builder()
                .tls_config(ServerTlsConfig::new().identity(identity))
                .expect("Invalid TLS config")
                .add_service(grpc_service.into_server())
                .serve_with_incoming(grpc_incoming)
        } else {
            tonic::transport::Server::builder()
                .add_service(grpc_service.into_server())
                .serve_with_incoming(grpc_incoming)
        };

        tracing::info!("Coordinator ready ({:?})", raft.get_role());

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
