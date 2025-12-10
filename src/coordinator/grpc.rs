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
