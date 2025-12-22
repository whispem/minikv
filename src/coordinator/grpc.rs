//! Coordinator gRPC service (internal)
//!
//! This module exposes the internal gRPC API for cluster coordination.
//! Security features (TLS, authentication) and cross-datacenter replication are planned for future releases.
//!
//! This module implements the internal gRPC protocol for cluster coordination.
//! Used for Raft consensus, metadata replication, and distributed operations between nodes.

use crate::proto::coordinator_internal_server::{CoordinatorInternal, CoordinatorInternalServer};
use crate::proto::*;
use tonic::{Request, Response, Status};

/// CoordGrpcService implements the internal gRPC API for cluster coordination.
pub struct CoordGrpcService {}

impl Default for CoordGrpcService {
    fn default() -> Self {
        Self::new()
    }
}

impl CoordGrpcService {
    pub fn new() -> Self {
        Self {}
    }

    /// Converts this service into a gRPC server instance.
    pub fn into_server(self) -> CoordinatorInternalServer<Self> {
        CoordinatorInternalServer::new(self)
    }
}

#[tonic::async_trait]
impl CoordinatorInternal for CoordGrpcService {
        async fn range(
            &self,
            req: Request<crate::proto::RangeRequest>,
        ) -> Result<Response<crate::proto::RangeResponse>, Status> {
            // Access MetadataStore (singleton/global or via a global Arc<...>)
            let store = crate::coordinator::metadata::get_global_store();
            let params = req.into_inner();
            let keys = match store.list_keys() {
                Ok(keys) => keys,
                Err(e) => return Err(Status::internal(format!("list_keys error: {}", e))),
            };
            let mut filtered: Vec<String> = keys
                .into_iter()
                .filter(|k| k >= &params.start && k <= &params.end)
                .collect();
            filtered.sort();
            let mut values = Vec::new();
            if params.include_values {
                for k in &filtered {
                    match store.get_key(k) {
                        Ok(Some(meta)) => values.push(bincode::serialize(&meta).unwrap_or_default()),
                        _ => values.push(vec![]),
                    }
                }
            }
            let resp = crate::proto::RangeResponse {
                keys: filtered,
                values,
            };
            Ok(Response::new(resp))
        }

        async fn batch(
            &self,
            req: Request<crate::proto::BatchRequest>,
        ) -> Result<Response<crate::proto::BatchResponse>, Status> {
            let store = crate::coordinator::metadata::get_global_store();
            let req = req.into_inner();
            let mut results = Vec::new();
            for op in req.ops {
                use crate::proto::batch_op::Type;
                let (ok, value, error) = match Type::try_from(op.r#type) {
                    Ok(Type::Put) => {
                        let meta = crate::coordinator::metadata::KeyMetadata {
                            key: op.key.clone(),
                            replicas: vec![],
                            size: op.value.len() as u64,
                            blake3: "".to_string(),
                            created_at: 0,
                            updated_at: 0,
                            state: crate::coordinator::metadata::KeyState::Active,
                        };
                        match store.put_key(&meta) {
                            Ok(_) => (true, vec![], None),
                            Err(e) => (false, vec![], Some(format!("{}", e))),
                        }
                    }
                    Ok(Type::Get) => {
                        match store.get_key(&op.key) {
                            Ok(Some(meta)) => (true, bincode::serialize(&meta).unwrap_or_default(), None),
                            Ok(None) => (false, vec![], Some("Not found".to_string())),
                            Err(e) => (false, vec![], Some(format!("{}", e))),
                        }
                    }
                    Ok(Type::Delete) => {
                        match store.delete_key(&op.key) {
                            Ok(_) => (true, vec![], None),
                            Err(e) => (false, vec![], Some(format!("{}", e))),
                        }
                    }
                    _ => (false, vec![], Some("Unknown op".to_string())),
                };
                results.push(crate::proto::BatchResult {
                    ok,
                    key: op.key,
                    value,
                    error: error.unwrap_or_default(),
                });
            }
            let resp = crate::proto::BatchResponse { results };
            Ok(Response::new(resp))
        }
    /// Handles Raft vote requests from other nodes.
    async fn request_vote(
        &self,
        req: Request<VoteRequest>,
    ) -> Result<Response<VoteResponse>, Status> {
        let vote_req = req.into_inner();

        let current_term = 1;
        let vote_granted = vote_req.term >= current_term;
        let resp = VoteResponse {
            term: current_term,
            vote_granted,
        };
        Ok(Response::new(resp))
    }

    async fn append_entries(
        &self,
        req: Request<AppendRequest>,
    ) -> Result<Response<AppendResponse>, Status> {
        let append_req = req.into_inner();

        let current_term = 1;
        let success = append_req.term >= current_term;
        let resp = AppendResponse {
            term: current_term,
            success,
            conflict_index: 0,
        };
        Ok(Response::new(resp))
    }

    async fn install_snapshot(
        &self,
        _req: Request<SnapshotRequest>,
    ) -> Result<Response<SnapshotResponse>, Status> {
        Err(Status::unimplemented("InstallSnapshot not implemented"))
    }

    async fn join(&self, _req: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        // Handle volume registration here
        Ok(Response::new(JoinResponse {
            ok: true,
            cluster_id: "cluster-1".to_string(),
        }))
    }

    async fn heartbeat(
        &self,
        _req: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        // Update volume state here
        Ok(Response::new(HeartbeatResponse {
            ok: true,
            commands: vec![],
        }))
    }
}
