use anyhow::Result;
use clap::Parser;
use minikv::volume::VolumeServer;
use std::path::PathBuf;
use tracing_subscriber;

#[derive(Parser, Debug)]
#[command(name = "minikv-volume")]
#[command(about = "MiniKV Volume Server - Distributed KV storage node")]
struct Args {
    /// Volume ID (unique identifier for this volume)
    #[arg(short, long, default_value = "volume-1")]
    id: String,

    /// gRPC address to listen on
    #[arg(short, long, default_value = "127.0.0.1:50052")]
    addr: String,

    /// Coordinator gRPC address
    #[arg(short, long, default_value = "http://127.0.0.1:50051")]
    coordinator: String,

    /// Data directory for storage
    #[arg(short, long, default_value = "./data/volume")]
    data_dir: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| args.log_level.clone().into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting MiniKV Volume Server");
    tracing::info!("Volume ID: {}", args.id);
    tracing::info!("Listening on: {}", args.addr);
    tracing::info!("Coordinator: {}", args.coordinator);
    tracing::info!("Data directory: {}", args.data_dir.display());

    // Create data directory if it doesn't exist
    tokio::fs::create_dir_all(&args.data_dir).await?;

    // Create and start volume server
    let server = VolumeServer::new(
        args.id.clone(),
        args.addr.clone(),
        args.coordinator.clone(),
        args.data_dir,
    )
    .await?;

    // Run server
    server.serve().await?;

    Ok(())
}
