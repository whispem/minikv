//! Volume binary

use clap::{Parser, Subcommand};
use minikv::{common::VolumeConfig, VolumeServer};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "minikv-volume")]
#[command(about = "minikv volume server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start volume server
    Serve {
        /// Volume ID
        #[arg(long)]
        id: String,

        /// Bind address for HTTP
        #[arg(long, default_value = "0.0.0.0:6000")]
        bind: String,

        /// Bind address for gRPC
        #[arg(long, default_value = "0.0.0.0:6001")]
        grpc: String,

        /// Data directory
        #[arg(long, default_value = "./vol-data")]
        data: PathBuf,

        /// WAL directory
        #[arg(long, default_value = "./vol-wal")]
        wal: PathBuf,

        /// Coordinator addresses (comma-separated)
        #[arg(long, value_delimiter = ',')]
        coordinators: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            id,
            bind,
            grpc,
            data,
            wal,
            coordinators,
        } => {
            let config = VolumeConfig {
                bind_addr: bind.parse()?,
                grpc_addr: grpc.parse()?,
                data_path: data,
                wal_path: wal,
                coordinators,
                ..Default::default()
            };

            let server = VolumeServer::new(config, id);
            server.serve().await?;
        }
    }

    Ok(())
}
