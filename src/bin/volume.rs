use std::path::PathBuf;
use minikv::volume::server::VolumeServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from("volume_data");
    let server = VolumeServer::new(path);
    server.serve().await?;
    Ok(())
}
