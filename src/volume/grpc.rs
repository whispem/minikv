//! gRPC internal service for coordinator communication

use crate::proto::{
    volume_internal_server::{VolumeInternal, VolumeInternalServer},
    AbortRequest, AbortResponse, Chunk, CommitRequest, CommitResponse, DeleteRequest,
    DeleteResponse, PingRequest, PingResponse, PrepareRequest, PrepareResponse, PullRequest,
    StatsRequest, StatsResponse,
};
use crate::volume::blob::BlobStore;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status};

/// Temporary upload tracking for 2PC
#[derive(Clone)]
struct UploadState {
    key: String,
    expected_size: u64,
    expected_blake3: String,
    data: Vec<u8>,
}

pub struct VolumeGrpcService {
    store: Arc<Mutex<BlobStore>>,
    volume_id: String,
    start_time: Instant,
    uploads: Arc<Mutex<HashMap<String, UploadState>>>,
}

impl VolumeGrpcService {
    pub fn new(store: Arc<Mutex<BlobStore>>, volume_id: String) -> Self {
        Self {
            store,
            volume_id,
            start_time: Instant::now(),
            uploads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn into_server(self) -> VolumeInternalServer<Self> {
        VolumeInternalServer::new(self)
    }
}

#[tonic::async_trait]
impl VolumeInternal for VolumeGrpcService {
    async fn prepare(
        &self,
        request: Request<PrepareRequest>,
    ) -> Result<Response<PrepareResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!(
            "PREPARE: upload_id={}, key={}, size={}",
            req.upload_id,
            req.key,
            req.expected_size
        );

        // Check if we have enough space (simplified - just check)
        let store = self.store.lock().unwrap();
        let stats = store.stats();
        drop(store);

        // Store upload state
        let state = UploadState {
            key: req.key.clone(),
            expected_size: req.expected_size,
            expected_blake3: req.expected_blake3,
            data: Vec::with_capacity(req.expected_size as usize),
        };

        self.uploads
            .lock()
            .unwrap()
            .insert(req.upload_id.clone(), state);

        tracing::debug!("PREPARE OK: {}", req.upload_id);

        Ok(Response::new(PrepareResponse {
            ok: true,
            error: String::new(),
        }))
    }

    async fn commit(
        &self,
        request: Request<CommitRequest>,
    ) -> Result<Response<CommitResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!("COMMIT: upload_id={}, key={}", req.upload_id, req.key);

        // Get upload state
        let state = {
            let mut uploads = self.uploads.lock().unwrap();
            match uploads.remove(&req.upload_id) {
                Some(s) => s,
                None => {
                    tracing::error!("COMMIT FAILED: upload {} not found", req.upload_id);
                    return Ok(Response::new(CommitResponse {
                        ok: false,
                        error: "Upload not found".to_string(),
                    }));
                }
            }
        };

        // Verify size
        if state.data.len() as u64 != state.expected_size {
            tracing::error!(
                "COMMIT FAILED: size mismatch {} vs {}",
                state.data.len(),
                state.expected_size
            );
            return Ok(Response::new(CommitResponse {
                ok: false,
                error: format!(
                    "Size mismatch: expected {}, got {}",
                    state.expected_size,
                    state.data.len()
                ),
            }));
        }

        // Verify BLAKE3 hash
        let computed_hash = crate::common::blake3_hash(&state.data);
        if computed_hash != state.expected_blake3 {
            tracing::error!(
                "COMMIT FAILED: hash mismatch {} vs {}",
                computed_hash,
                state.expected_blake3
            );
            return Ok(Response::new(CommitResponse {
                ok: false,
                error: "Hash mismatch".to_string(),
            }));
        }

        // Write to store
        let mut store = self.store.lock().unwrap();
        match store.put(&state.key, &state.data) {
            Ok(_) => {
                tracing::info!("COMMIT OK: {} ({} bytes)", state.key, state.data.len());
                Ok(Response::new(CommitResponse {
                    ok: true,
                    error: String::new(),
                }))
            }
            Err(e) => {
                tracing::error!("COMMIT FAILED: {}", e);
                Ok(Response::new(CommitResponse {
                    ok: false,
                    error: e.to_string(),
                }))
            }
        }
    }

    async fn abort(
        &self,
        request: Request<AbortRequest>,
    ) -> Result<Response<AbortResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!("ABORT: upload_id={}", req.upload_id);

        self.uploads.lock().unwrap().remove(&req.upload_id);

        Ok(Response::new(AbortResponse { ok: true }))
    }

    type PullStream = ReceiverStream<Result<Chunk, Status>>;

    async fn pull(
        &self,
        request: Request<PullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let req = request.into_inner();

        tracing::debug!("PULL: key={} from {}", req.key, req.source_url);

        // Fetch blob from source URL
        let client = reqwest::Client::new();
        let response = client
            .get(&req.source_url)
            .send()
            .await
            .map_err(|e| Status::internal(format!("Failed to fetch from source: {}", e)))?;

        if !response.status().is_success() {
            return Err(Status::not_found("Source blob not found"));
        }

        // Stream response in chunks
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        if tx
                            .send(Ok(Chunk {
                                data: bytes.to_vec(),
                            }))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(Status::internal(format!("Stream error: {}", e))))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let req = request.into_inner();

        tracing::debug!("DELETE: key={}", req.key);

        let mut store = self.store.lock().unwrap();
        match store.delete(&req.key) {
            Ok(_) => {
                tracing::info!("DELETE OK: {}", req.key);
                Ok(Response::new(DeleteResponse {
                    ok: true,
                    error: String::new(),
                }))
            }
            Err(e) => {
                tracing::error!("DELETE FAILED: {}", e);
                Ok(Response::new(DeleteResponse {
                    ok: false,
                    error: e.to_string(),
                }))
            }
        }
    }

    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let store = self.store.lock().unwrap();
        let stats = store.stats();

        Ok(Response::new(PingResponse {
            volume_id: self.volume_id.clone(),
            uptime_secs: self.start_time.elapsed().as_secs(),
            total_keys: stats.total_keys as u64,
            total_bytes: stats.total_bytes,
        }))
    }

    async fn stats(
        &self,
        _request: Request<StatsRequest>,
    ) -> Result<Response<StatsResponse>, Status> {
        let store = self.store.lock().unwrap();
        let stats = store.stats();

        // TODO: Get actual free space from filesystem
        let free_bytes = 0;

        Ok(Response::new(StatsResponse {
            total_keys: stats.total_keys as u64,
            total_bytes: stats.total_bytes,
            free_bytes,
            shards: vec![], // TODO: Implement sharding
        }))
    }
}
