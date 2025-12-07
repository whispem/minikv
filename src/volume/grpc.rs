use crate::proto::volume_internal_server::{VolumeInternal, VolumeInternalServer};
use crate::proto::*;
use crate::volume::blob::BlobStore;
use std::sync::{Arc, Mutex};
use tonic::{Request, Response, Status};

pub struct VolumeGrpcService {
    store: Arc<Mutex<BlobStore>>,
}

impl VolumeGrpcService {
    pub fn new(store: BlobStore) -> Self {
        VolumeGrpcService {
            store: Arc::new(Mutex::new(store)),
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
        req: Request<PrepareRequest>,
    ) -> Result<Response<PrepareResponse>, Status> {
        let inner = req.into_inner();
        
        // Validate request
        if inner.key.is_empty() {
            return Ok(Response::new(PrepareResponse {
                ok: false,
                error: "key cannot be empty".to_string(),
            }));
        }

        // Check if we have space (simplified check)
        // In production: check disk space, quotas, etc.
        
        Ok(Response::new(PrepareResponse {
            ok: true,
            error: String::new(),
        }))
    }

    async fn commit(
        &self,
        req: Request<CommitRequest>,
    ) -> Result<Response<CommitResponse>, Status> {
        let inner = req.into_inner();
        
        // For now, we just acknowledge
        // In production: finalize the transaction, make data durable
        
        Ok(Response::new(CommitResponse {
            ok: true,
            error: String::new(),
        }))
    }

    async fn abort(
        &self,
        req: Request<AbortRequest>,
    ) -> Result<Response<AbortResponse>, Status> {
        let inner = req.into_inner();
        
        // Clean up any prepared state
        // In production: delete temp files, release locks
        
        Ok(Response::new(AbortResponse {
            ok: true,
        }))
    }

    async fn pull(
        &self,
        _req: Request<PullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        Err(Status::unimplemented("Pull not implemented"))
    }

    async fn delete(
        &self,
        req: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let inner = req.into_inner();
        
        match self.store.lock(). unwrap().delete(&inner.key) {
            Ok(_) => Ok(Response::new(DeleteResponse {
                ok: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(DeleteResponse {
                ok: false,
                error: e.to_string(),
            })),
        }
    }

    async fn ping(
        &self,
        _req: Request<PingRequest>,
    ) -> Result<Response<PingResponse>, Status> {
        Ok(Response::new(PingResponse {
            volume_id: "vol-1".to_string(),
            uptime_secs: 0,
            total_keys: 0,
            total_bytes: 0,
        }))
    }

    async fn stats(
        &self,
        _req: Request<StatsRequest>,
    ) -> Result<Response<StatsResponse>, Status> {
        Ok(Response::new(StatsResponse {
            total_keys: 0,
            total_bytes: 0,
            free_bytes: 0,
            shards: vec![],
        }))
    }

    type PullStream = tokio_stream::wrappers::ReceiverStream<Result<Chunk, Status>>;
}
