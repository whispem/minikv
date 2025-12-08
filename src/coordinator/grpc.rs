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
