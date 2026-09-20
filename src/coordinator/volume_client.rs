use crate::coordinator::raft_rpc_client::normalize_addr;
use crate::proto::volume_internal_client::VolumeInternalClient;
use crate::proto::*;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};

pub type ClientError = Box<dyn std::error::Error + Send + Sync>;

pub const MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct VolumeClient {
    client: VolumeInternalClient<Channel>,
}

impl VolumeClient {
    fn endpoint(addr: &str) -> Result<Endpoint, ClientError> {
        Ok(Endpoint::from_shared(normalize_addr(addr))?
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT))
    }

    fn from_channel(channel: Channel) -> Self {
        Self {
            client: VolumeInternalClient::new(channel)
                .max_decoding_message_size(MAX_MESSAGE_SIZE)
                .max_encoding_message_size(MAX_MESSAGE_SIZE),
        }
    }

    pub async fn connect(addr: String) -> Result<Self, ClientError> {
        let channel = Self::endpoint(&addr)?.connect().await?;
        Ok(Self::from_channel(channel))
    }

    pub fn lazy(addr: &str) -> Result<Self, ClientError> {
        Ok(Self::from_channel(Self::endpoint(addr)?.connect_lazy()))
    }

    pub async fn prepare(
        &mut self,
        key: String,
        upload_id: String,
        data: Vec<u8>,
        expected_blake3: String,
    ) -> Result<PrepareResponse, ClientError> {
        let request = PrepareRequest {
            key,
            upload_id,
            expected_size: data.len() as u64,
            expected_blake3,
            data,
        };
        Ok(self.client.prepare(request).await?.into_inner())
    }

    pub async fn commit(
        &mut self,
        upload_id: String,
        key: String,
    ) -> Result<CommitResponse, ClientError> {
        let request = CommitRequest { upload_id, key };
        Ok(self.client.commit(request).await?.into_inner())
    }

    pub async fn abort(&mut self, upload_id: String) -> Result<AbortResponse, ClientError> {
        let request = AbortRequest { upload_id };
        Ok(self.client.abort(request).await?.into_inner())
    }

    pub async fn pull(&mut self, key: String) -> Result<Option<Vec<u8>>, ClientError> {
        let request = PullRequest {
            key,
            source_url: String::new(),
        };
        let mut stream = match self.client.pull(request).await {
            Ok(response) => response.into_inner(),
            Err(status) if status.code() == tonic::Code::NotFound => return Ok(None),
            Err(status) => return Err(status.into()),
        };
        let mut data = Vec::new();
        while let Some(chunk) = stream.message().await? {
            data.extend_from_slice(&chunk.data);
        }
        Ok(Some(data))
    }

    pub async fn delete(&mut self, key: String) -> Result<DeleteResponse, ClientError> {
        Ok(self
            .client
            .delete(DeleteRequest { key })
            .await?
            .into_inner())
    }

    pub async fn ping(&mut self) -> Result<PingResponse, ClientError> {
        Ok(self.client.ping(PingRequest {}).await?.into_inner())
    }
}
