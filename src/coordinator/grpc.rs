use crate::common::NodeState;
use crate::coordinator::metadata::{MetadataStore, VolumeMetadata};
use crate::coordinator::objects::{now_ms, ObjectError, ObjectStore};
use crate::coordinator::raft_node::RaftNode;
use crate::proto::coordinator_internal_server::{CoordinatorInternal, CoordinatorInternalServer};
use crate::proto::*;
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub struct CoordGrpcService {
    raft: Option<Arc<RaftNode>>,
    objects: Option<ObjectStore>,
}

impl Default for CoordGrpcService {
    fn default() -> Self {
        Self::new()
    }
}

impl CoordGrpcService {
    pub fn new() -> Self {
        Self {
            raft: None,
            objects: None,
        }
    }

    pub fn with_raft(raft: Arc<RaftNode>, objects: ObjectStore) -> Self {
        Self {
            raft: Some(raft),
            objects: Some(objects),
        }
    }

    pub fn into_server(self) -> CoordinatorInternalServer<Self> {
        CoordinatorInternalServer::new(self)
    }

    fn raft(&self) -> Result<&Arc<RaftNode>, Status> {
        self.raft
            .as_ref()
            .ok_or_else(|| Status::unavailable("raft is not running on this coordinator"))
    }

    fn objects(&self) -> Result<&ObjectStore, Status> {
        self.objects
            .as_ref()
            .ok_or_else(|| Status::unavailable("storage is not running on this coordinator"))
    }

    fn metadata(&self) -> Result<&Arc<MetadataStore>, Status> {
        Ok(self.objects()?.metadata())
    }
}

#[tonic::async_trait]
impl CoordinatorInternal for CoordGrpcService {
    async fn range(&self, req: Request<RangeRequest>) -> Result<Response<RangeResponse>, Status> {
        let store = self.metadata()?;
        let params = req.into_inner();
        let keys = store
            .list_keys()
            .map_err(|e| Status::internal(format!("list_keys error: {}", e)))?;
        let mut filtered: Vec<String> = keys
            .into_iter()
            .filter(|k| k >= &params.start && k <= &params.end)
            .collect();
        filtered.sort();
        let mut values = Vec::new();
        if params.include_values {
            for key in &filtered {
                match store.get_key(key) {
                    Ok(Some(meta)) => values.push(bincode::serialize(&meta).unwrap_or_default()),
                    _ => values.push(vec![]),
                }
            }
        }
        Ok(Response::new(RangeResponse {
            keys: filtered,
            values,
        }))
    }

    async fn batch(&self, req: Request<BatchRequest>) -> Result<Response<BatchResponse>, Status> {
        let objects = self.objects()?;
        let mut results = Vec::new();
        for op in req.into_inner().ops {
            use crate::proto::batch_op::Type;
            let outcome = match Type::try_from(op.r#type) {
                Ok(Type::Put) => objects.put(&op.key, op.value).await.map(|_| vec![]),
                Ok(Type::Get) => objects.get(&op.key).await,
                Ok(Type::Delete) => objects.delete(&op.key).await.map(|_| vec![]),
                Err(_) => Err(ObjectError::Metadata("unknown operation".to_string())),
            };
            let (ok, value, error) = match outcome {
                Ok(value) => (true, value, String::new()),
                Err(e) => (false, vec![], e.to_string()),
            };
            results.push(BatchResult {
                ok,
                key: op.key,
                value,
                error,
            });
        }
        Ok(Response::new(BatchResponse { results }))
    }

    async fn request_vote(
        &self,
        req: Request<VoteRequest>,
    ) -> Result<Response<VoteResponse>, Status> {
        let response = self.raft()?.handle_request_vote(req.into_inner().into());
        Ok(Response::new(response.into()))
    }

    async fn append_entries(
        &self,
        req: Request<AppendRequest>,
    ) -> Result<Response<AppendResponse>, Status> {
        let response = self.raft()?.handle_append_entries(req.into_inner().into());
        Ok(Response::new(response.into()))
    }

    async fn install_snapshot(
        &self,
        _req: Request<SnapshotRequest>,
    ) -> Result<Response<SnapshotResponse>, Status> {
        Err(Status::unimplemented("InstallSnapshot not implemented"))
    }

    async fn read_index(
        &self,
        _req: Request<ReadIndexRequest>,
    ) -> Result<Response<ReadIndexResponse>, Status> {
        let raft = self.raft()?;
        let response = match raft.read_index().await {
            Ok(read_index) => ReadIndexResponse {
                is_leader: true,
                read_index,
                leader_id: raft.node_id().to_string(),
            },
            Err(_) => ReadIndexResponse {
                is_leader: false,
                read_index: 0,
                leader_id: raft.get_leader().unwrap_or_default(),
            },
        };
        Ok(Response::new(response))
    }

    async fn join(&self, req: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        let store = self.metadata()?;
        let req = req.into_inner();
        let shards = req.shards.iter().filter_map(|s| s.parse().ok()).collect();
        store
            .put_volume(&VolumeMetadata {
                volume_id: req.volume_id,
                address: req.address.clone(),
                grpc_address: req.address,
                state: NodeState::Alive,
                shards,
                total_keys: 0,
                total_bytes: 0,
                free_bytes: 0,
                last_heartbeat: now_ms(),
            })
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(JoinResponse {
            ok: true,
            cluster_id: "minikv".to_string(),
        }))
    }

    async fn heartbeat(
        &self,
        req: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let store = self.metadata()?;
        let req = req.into_inner();
        let known = store
            .get_volume(&req.volume_id)
            .map_err(|e| Status::internal(e.to_string()))?;
        let Some(mut volume) = known else {
            return Ok(Response::new(HeartbeatResponse {
                ok: false,
                commands: vec!["join".to_string()],
            }));
        };
        volume.total_keys = req.total_keys;
        volume.total_bytes = req.total_bytes;
        volume.free_bytes = req.free_bytes;
        volume.state = NodeState::Alive;
        volume.last_heartbeat = now_ms();
        store
            .put_volume(&volume)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(HeartbeatResponse {
            ok: true,
            commands: vec![],
        }))
    }
}
