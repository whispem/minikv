//! Coordinator gRPC service (internal)

use crate::proto::coordinator_internal_server::{CoordinatorInternal, CoordinatorInternalServer};
use crate::proto::*;
use tonic::{Request, Response, Status};

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

    pub fn into_server(self) -> CoordinatorInternalServer<Self> {
        CoordinatorInternalServer::new(self)
    }
}

#[tonic::async_trait]
impl CoordinatorInternal for CoordGrpcService {
    async fn request_vote(
        &self,
        _req: Request<VoteRequest>,
    ) -> Result<Response<VoteResponse>, Status> {
        // TODO: Implement Raft RequestVote RPC
        Err(Status::unimplemented("RequestVote not implemented"))
    }

    async fn append_entries(
        &self,
        _req: Request<AppendRequest>,
    ) -> Result<Response<AppendResponse>, Status> {
        // TODO: Implement Raft AppendEntries RPC
        Err(Status::unimplemented("AppendEntries not implemented"))
    }

    async fn install_snapshot(
        &self,
        _req: Request<SnapshotRequest>,
    ) -> Result<Response<SnapshotResponse>, Status> {
        Err(Status::unimplemented("InstallSnapshot not implemented"))
    }

    async fn join(&self, _req: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        // TODO: Handle volume registration
        Ok(Response::new(JoinResponse {
            ok: true,
            cluster_id: "cluster-1".to_string(),
        }))
    }

    async fn heartbeat(
        &self,
        _req: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        // TODO: Update volume state
        Ok(Response::new(HeartbeatResponse {
            ok: true,
            commands: vec![],
        }))
    }
}
