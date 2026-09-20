use crate::common::raft::{AppendRequest, AppendResponse, VoteRequest, VoteResponse};
use crate::proto::coordinator_internal_client::CoordinatorInternalClient;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};

pub type PeerClient = CoordinatorInternalClient<Channel>;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);
const REQUEST_TIMEOUT: Duration = Duration::from_millis(500);

pub fn normalize_addr(addr: &str) -> String {
    let addr = addr.trim();
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{}", addr)
    }
}

pub fn lazy_client(addr: &str) -> Result<PeerClient, tonic::Status> {
    let endpoint = Endpoint::from_shared(normalize_addr(addr))
        .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT);
    Ok(CoordinatorInternalClient::new(endpoint.connect_lazy()))
}

pub async fn append_entries(
    client: &mut PeerClient,
    req: &AppendRequest,
) -> Result<AppendResponse, tonic::Status> {
    let resp = client
        .append_entries(crate::proto::AppendRequest::from(req))
        .await?
        .into_inner();
    Ok((&resp).into())
}

pub async fn request_vote(
    client: &mut PeerClient,
    req: &VoteRequest,
) -> Result<VoteResponse, tonic::Status> {
    let resp = client
        .request_vote(crate::proto::VoteRequest::from(req))
        .await?
        .into_inner();
    Ok((&resp).into())
}

pub async fn read_index(
    client: &mut PeerClient,
) -> Result<crate::proto::ReadIndexResponse, tonic::Status> {
    Ok(client
        .read_index(crate::proto::ReadIndexRequest {})
        .await?
        .into_inner())
}

pub async fn send_append_entries_rpc(
    peer_addr: &str,
    req: AppendRequest,
) -> Result<AppendResponse, tonic::Status> {
    let mut client = lazy_client(peer_addr)?;
    append_entries(&mut client, &req).await
}

pub async fn send_request_vote_rpc(
    peer_addr: &str,
    req: VoteRequest,
) -> Result<VoteResponse, tonic::Status> {
    let mut client = lazy_client(peer_addr)?;
    request_vote(&mut client, &req).await
}
