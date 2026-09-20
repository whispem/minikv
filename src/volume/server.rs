use crate::common::{Result, WalSyncPolicy};
use crate::volume::blob::BlobStore;
use crate::volume::grpc::VolumeGrpcService;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use futures_util::future::join_all;
use serde_json::{json, Value};
use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tonic::transport::server::TcpIncoming;

#[derive(Debug, Clone)]
pub struct VolumeServerConfig {
    pub volume_id: String,
    pub http_addr: SocketAddr,
    pub grpc_addr: SocketAddr,
    pub advertise_grpc: String,
    pub data_path: PathBuf,
    pub wal_path: PathBuf,
    pub coordinators: Vec<String>,
    pub heartbeat_interval: Duration,
}

impl VolumeServerConfig {
    pub fn new(volume_id: impl Into<String>, data_path: PathBuf, wal_path: PathBuf) -> Self {
        let grpc_addr: SocketAddr = "127.0.0.1:6001".parse().unwrap();
        Self {
            volume_id: volume_id.into(),
            http_addr: "127.0.0.1:6000".parse().unwrap(),
            grpc_addr,
            advertise_grpc: format!("http://{}", grpc_addr),
            data_path,
            wal_path,
            coordinators: vec![],
            heartbeat_interval: Duration::from_secs(1),
        }
    }
}

pub struct VolumeServer {
    config: VolumeServerConfig,
    store: Arc<Mutex<BlobStore>>,
}

#[derive(Clone)]
struct HttpState {
    volume_id: String,
    store: Arc<Mutex<BlobStore>>,
}

impl VolumeServer {
    pub fn open(config: VolumeServerConfig) -> Result<Self> {
        let store = BlobStore::open(&config.data_path, &config.wal_path, WalSyncPolicy::Always)?;
        Ok(Self {
            config,
            store: Arc::new(Mutex::new(store)),
        })
    }

    pub fn store(&self) -> Arc<Mutex<BlobStore>> {
        self.store.clone()
    }

    pub async fn serve(self) -> Result<()> {
        let config = self.config;
        tracing::info!("Starting volume: {}", config.volume_id);
        tracing::info!("  HTTP: {}", config.http_addr);
        tracing::info!(
            "  gRPC: {} (advertised as {})",
            config.grpc_addr,
            config.advertise_grpc
        );
        tracing::info!("  Coordinators: {:?}", config.coordinators);

        let grpc_listener = tokio::net::TcpListener::bind(config.grpc_addr).await?;
        let incoming = TcpIncoming::from_listener(grpc_listener, true, None)
            .map_err(|e| crate::Error::Internal(format!("gRPC listener error: {}", e)))?;
        let service = VolumeGrpcService::with_store(config.volume_id.clone(), self.store.clone());
        let grpc_server = tonic::transport::Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(incoming);

        let http_listener = tokio::net::TcpListener::bind(config.http_addr).await?;
        let app = Router::new()
            .route("/health", get(health))
            .with_state(HttpState {
                volume_id: config.volume_id.clone(),
                store: self.store.clone(),
            });
        let http_server = axum::serve(http_listener, app).into_future();

        tokio::spawn(heartbeat_loop(config.clone(), self.store.clone()));

        tokio::select! {
            result = grpc_server => {
                result.map_err(|e| crate::Error::Internal(format!("gRPC server error: {}", e)))?
            }
            result = http_server => result?,
        }
        Ok(())
    }
}

async fn health(State(state): State<HttpState>) -> Json<Value> {
    let stats = state.store.lock().unwrap().stats();
    Json(json!({
        "status": "ok",
        "volume_id": state.volume_id,
        "total_keys": stats.total_keys,
        "total_bytes": stats.total_bytes,
    }))
}

async fn heartbeat_loop(config: VolumeServerConfig, store: Arc<Mutex<BlobStore>>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap_or_default();
    let http_address = format!("http://{}", config.http_addr);
    let mut reachable = vec![false; config.coordinators.len()];
    let mut ticker = tokio::time::interval(config.heartbeat_interval);
    loop {
        ticker.tick().await;
        let stats = store.lock().unwrap().stats();
        let payload = json!({
            "volume_id": config.volume_id,
            "grpc_address": config.advertise_grpc,
            "http_address": http_address,
            "total_keys": stats.total_keys,
            "total_bytes": stats.total_bytes,
            "free_bytes": 0,
        })
        .to_string();

        let sends = config.coordinators.iter().map(|coordinator| {
            let url = format!(
                "{}/internal/volumes/heartbeat",
                coordinator.trim_end_matches('/')
            );
            client
                .post(url)
                .header("content-type", "application/json")
                .body(payload.clone())
                .send()
        });
        let results = join_all(sends).await;

        for ((coordinator, result), was_reachable) in config
            .coordinators
            .iter()
            .zip(results)
            .zip(reachable.iter_mut())
        {
            let ok = matches!(&result, Ok(response) if response.status().is_success());
            if ok && !*was_reachable {
                tracing::info!("Registered with coordinator {}", coordinator);
            }
            if !ok && *was_reachable {
                tracing::warn!("Coordinator {} stopped answering heartbeats", coordinator);
            }
            *was_reachable = ok;
        }
    }
}
