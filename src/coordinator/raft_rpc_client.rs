//! Raft gRPC client helpers
use crate::common::raft::{AppendRequest, AppendResponse, VoteRequest, VoteResponse};
use crate::proto::coordinator_internal_client::CoordinatorInternalClient;

pub async fn send_append_entries_rpc(
    peer_addr: &str,
    req: AppendRequest,
) -> Result<AppendResponse, tonic::Status> {
    let mut client = CoordinatorInternalClient::connect(peer_addr.to_string())
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
    let proto_req: crate::proto::AppendRequest = (&req).into();
    let resp = client.append_entries(proto_req).await?.into_inner();
    Ok((&resp).into())
}

pub async fn send_request_vote_rpc(
    peer_addr: &str,
    req: VoteRequest,
) -> Result<VoteResponse, tonic::Status> {
    let mut client = CoordinatorInternalClient::connect(peer_addr.to_string())
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
    let proto_req: crate::proto::VoteRequest = (&req).into();
    let resp = client.request_vote(proto_req).await?.into_inner();
    Ok((&resp).into())
}
