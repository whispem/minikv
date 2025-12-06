use minikv::volume::server::VolumeServer;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from("volume_data");
    let server = VolumeServer::new(path);
    server.serve().await?;
    Ok(())
}
