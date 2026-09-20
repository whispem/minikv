use crate::common::blake3_hash;
use crate::proto::volume_internal_server::{VolumeInternal, VolumeInternalServer};
use crate::proto::*;
use crate::volume::blob::BlobStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

pub const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;
const CHUNK_SIZE: usize = 1024 * 1024;
const STAGING_TTL: Duration = Duration::from_secs(120);

struct Staged {
    key: String,
    data: Vec<u8>,
    created: Instant,
}

pub struct VolumeGrpcService {
    volume_id: String,
    store: Arc<Mutex<BlobStore>>,
    staged: Arc<Mutex<HashMap<String, Staged>>>,
    started: Instant,
}

fn internal<E: std::fmt::Display>(e: E) -> Status {
    Status::internal(e.to_string())
}

impl VolumeGrpcService {
    pub fn new(store: BlobStore) -> Self {
        Self::with_store("vol-1".to_string(), Arc::new(Mutex::new(store)))
    }

    pub fn with_store(volume_id: String, store: Arc<Mutex<BlobStore>>) -> Self {
        VolumeGrpcService {
            volume_id,
            store,
            staged: Arc::new(Mutex::new(HashMap::new())),
            started: Instant::now(),
        }
    }

    pub fn into_server(self) -> VolumeInternalServer<Self> {
        VolumeInternalServer::new(self)
            .max_decoding_message_size(MAX_MESSAGE_SIZE)
            .max_encoding_message_size(MAX_MESSAGE_SIZE)
    }

    pub fn staged_uploads(&self) -> usize {
        self.staged.lock().unwrap().len()
    }
}

#[tonic::async_trait]
impl VolumeInternal for VolumeGrpcService {
    async fn prepare(
        &self,
        req: Request<PrepareRequest>,
    ) -> Result<Response<PrepareResponse>, Status> {
        let inner = req.into_inner();
        let refuse = |error: &str| {
            Ok(Response::new(PrepareResponse {
                ok: false,
                error: error.to_string(),
            }))
        };

        if inner.key.is_empty() {
            return refuse("key cannot be empty");
        }
        if inner.upload_id.is_empty() {
            return refuse("upload_id cannot be empty");
        }
        if inner.data.len() as u64 != inner.expected_size {
            return refuse("size mismatch");
        }
        if !inner.expected_blake3.is_empty() && blake3_hash(&inner.data) != inner.expected_blake3 {
            return refuse("checksum mismatch");
        }

        let mut staged = self.staged.lock().unwrap();
        staged.retain(|_, upload| upload.created.elapsed() < STAGING_TTL);
        staged.insert(
            inner.upload_id,
            Staged {
                key: inner.key,
                data: inner.data,
                created: Instant::now(),
            },
        );
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
        let upload = {
            let mut staged = self.staged.lock().unwrap();
            match staged.get(&inner.upload_id).map(|u| u.key == inner.key) {
                Some(false) => {
                    return Ok(Response::new(CommitResponse {
                        ok: false,
                        error: "key does not match the prepared upload".to_string(),
                    }));
                }
                Some(true) => staged.remove(&inner.upload_id),
                None => None,
            }
        };
        let Some(upload) = upload else {
            return Ok(Response::new(CommitResponse {
                ok: false,
                error: "unknown upload_id".to_string(),
            }));
        };

        let store = self.store.clone();
        let written = tokio::task::spawn_blocking(move || {
            store.lock().unwrap().put(&upload.key, &upload.data)
        })
        .await
        .map_err(internal)?;
        Ok(Response::new(match written {
            Ok(()) => CommitResponse {
                ok: true,
                error: String::new(),
            },
            Err(e) => CommitResponse {
                ok: false,
                error: e.to_string(),
            },
        }))
    }

    async fn abort(&self, req: Request<AbortRequest>) -> Result<Response<AbortResponse>, Status> {
        let inner = req.into_inner();
        self.staged.lock().unwrap().remove(&inner.upload_id);
        Ok(Response::new(AbortResponse { ok: true }))
    }

    async fn pull(&self, req: Request<PullRequest>) -> Result<Response<Self::PullStream>, Status> {
        let key = req.into_inner().key;
        let store = self.store.clone();
        let value = tokio::task::spawn_blocking(move || store.lock().unwrap().get(&key))
            .await
            .map_err(internal)?
            .map_err(internal)?;
        let data = value.ok_or_else(|| Status::not_found("key not found"))?;

        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            for chunk in data.chunks(CHUNK_SIZE) {
                let chunk = Chunk {
                    data: chunk.to_vec(),
                };
                if tx.send(Ok(chunk)).await.is_err() {
                    break;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn delete(
        &self,
        req: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let key = req.into_inner().key;
        let store = self.store.clone();
        let deleted = tokio::task::spawn_blocking(move || store.lock().unwrap().delete(&key))
            .await
            .map_err(internal)?;
        Ok(Response::new(match deleted {
            Ok(()) => DeleteResponse {
                ok: true,
                error: String::new(),
            },
            Err(e) => DeleteResponse {
                ok: false,
                error: e.to_string(),
            },
        }))
    }

    async fn ping(&self, _req: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let stats = self.store.lock().unwrap().stats();
        Ok(Response::new(PingResponse {
            volume_id: self.volume_id.clone(),
            uptime_secs: self.started.elapsed().as_secs(),
            total_keys: stats.total_keys as u64,
            total_bytes: stats.total_bytes,
        }))
    }

    async fn stats(&self, _req: Request<StatsRequest>) -> Result<Response<StatsResponse>, Status> {
        let stats = self.store.lock().unwrap().stats();
        Ok(Response::new(StatsResponse {
            total_keys: stats.total_keys as u64,
            total_bytes: stats.total_bytes,
            free_bytes: 0,
            shards: vec![],
        }))
    }

    type PullStream = ReceiverStream<Result<Chunk, Status>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::WalSyncPolicy;
    use tempfile::tempdir;

    fn service(dir: &std::path::Path) -> VolumeGrpcService {
        let store =
            BlobStore::open(&dir.join("data"), &dir.join("wal"), WalSyncPolicy::Never).unwrap();
        VolumeGrpcService::with_store("vol-test".to_string(), Arc::new(Mutex::new(store)))
    }

    fn prepare_request(key: &str, upload_id: &str, data: &[u8]) -> PrepareRequest {
        PrepareRequest {
            key: key.to_string(),
            upload_id: upload_id.to_string(),
            expected_size: data.len() as u64,
            expected_blake3: blake3_hash(data),
            data: data.to_vec(),
        }
    }

    async fn pull_all(service: &VolumeGrpcService, key: &str) -> Result<Vec<u8>, Status> {
        let mut stream = service
            .pull(Request::new(PullRequest {
                key: key.to_string(),
                source_url: String::new(),
            }))
            .await?
            .into_inner();
        let mut data = Vec::new();
        while let Some(chunk) = tokio_stream::StreamExt::next(&mut stream).await {
            data.extend(chunk?.data);
        }
        Ok(data)
    }

    #[tokio::test]
    async fn prepared_data_is_invisible_until_commit() {
        let dir = tempdir().unwrap();
        let service = service(dir.path());
        let prepared = service
            .prepare(Request::new(prepare_request("k#1", "u1", b"hello")))
            .await
            .unwrap()
            .into_inner();
        assert!(prepared.ok);
        assert_eq!(service.staged_uploads(), 1);
        let missing = pull_all(&service, "k#1").await.unwrap_err();
        assert_eq!(missing.code(), tonic::Code::NotFound);

        let committed = service
            .commit(Request::new(CommitRequest {
                upload_id: "u1".to_string(),
                key: "k#1".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(committed.ok);
        assert_eq!(service.staged_uploads(), 0);
        assert_eq!(pull_all(&service, "k#1").await.unwrap(), b"hello".to_vec());
    }

    #[tokio::test]
    async fn corrupted_or_aborted_uploads_are_never_written() {
        let dir = tempdir().unwrap();
        let service = service(dir.path());
        let mut corrupted = prepare_request("k#2", "u2", b"hello");
        corrupted.data = b"hellO".to_vec();
        let refused = service
            .prepare(Request::new(corrupted))
            .await
            .unwrap()
            .into_inner();
        assert!(!refused.ok);

        service
            .prepare(Request::new(prepare_request("k#3", "u3", b"bye")))
            .await
            .unwrap();
        service
            .abort(Request::new(AbortRequest {
                upload_id: "u3".to_string(),
            }))
            .await
            .unwrap();
        let late = service
            .commit(Request::new(CommitRequest {
                upload_id: "u3".to_string(),
                key: "k#3".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(!late.ok);
        assert!(pull_all(&service, "k#3").await.is_err());
    }
}
