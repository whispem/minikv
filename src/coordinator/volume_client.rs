use crate::proto::volume_internal_client::VolumeInternalClient;
use crate::proto::*;
use tonic::transport::Channel;

pub struct VolumeClient {
    client: VolumeInternalClient<Channel>,
}

impl VolumeClient {
    pub async fn connect(addr: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = VolumeInternalClient::connect(addr).await?;
        Ok(Self { client })
    }

    pub async fn prepare(
        &mut self,
        key: String,
        upload_id: String,
        expected_size: u64,
        expected_blake3: String,
    ) -> Result<PrepareResponse, Box<dyn std::error::Error>> {
        let request = tonic::Request::new(PrepareRequest {
            key,
            upload_id,
            expected_size,
            expected_blake3,
        });

        let response = self.client.prepare(request).await?;
        Ok(response.into_inner())
    }

    pub async fn commit(
        &mut self,
        upload_id: String,
        key: String,
    ) -> Result<CommitResponse, Box<dyn std::error::Error>> {
        let request = tonic::Request::new(CommitRequest { upload_id, key });

        let response = self.client.commit(request).await?;
        Ok(response.into_inner())
    }

    pub async fn abort(
        &mut self,
        upload_id: String,
    ) -> Result<AbortResponse, Box<dyn std::error::Error>> {
        let request = tonic::Request::new(AbortRequest { upload_id });

        let response = self.client.abort(request).await?;
        Ok(response.into_inner())
    }
}
