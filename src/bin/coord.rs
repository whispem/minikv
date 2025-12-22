//! Coordinator binary

use clap::{Parser, Subcommand};
use minikv::{common::CoordinatorConfig, Coordinator};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "minikv-coord")]
#[command(about = "minikv coordinator with Raft consensus")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start coordinator server
    Serve {
        /// Node ID
        #[arg(long)]
        id: String,

        /// Bind address for HTTP
        #[arg(long, default_value = "0.0.0.0:5000")]
        bind: String,

        /// Bind address for gRPC
        #[arg(long, default_value = "0.0.0.0:5001")]
        grpc: String,

        /// Database directory
        #[arg(long, default_value = "./coord-data")]
        db: PathBuf,

        /// Raft peers (comma-separated)
        #[arg(long, value_delimiter = ',')]
        peers: Vec<String>,

        /// Replication factor
        #[arg(long, default_value = "3")]
        replicas: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            id,
            bind,
            grpc,
            db,
            peers,
            replicas,
        } => {
            // Load config from file, then override with CLI arguments
            let config = minikv::common::config::Config::load();
            // Override fields if provided via CLI
            let bind_addr = bind.parse()?;
            let grpc_addr = grpc.parse()?;
            let db_path = db;
            let mut coord_config = CoordinatorConfig {
                bind_addr,
                grpc_addr,
                db_path,
                peers,
                replicas,
                ..Default::default()
            };
            // If file config exists, merge it (CLI has priority)
            if let Some(file_conf) = config.coordinator {
                let bind_addr = file_conf.bind_addr;
                let grpc_addr = file_conf.grpc_addr;
                let db_path = file_conf.db_path.clone();
                let peers = file_conf.peers.clone();
                let replicas = file_conf.replicas;
                if bind_addr != "0.0.0.0:5000".parse().unwrap() {
                    coord_config.bind_addr = bind_addr;
                }
                if grpc_addr != "0.0.0.0:5001".parse().unwrap() {
                    coord_config.grpc_addr = grpc_addr;
                }
                if db_path.as_path() != std::path::Path::new("./coord-data") {
                    coord_config.db_path = db_path;
                }
                if !peers.is_empty() {
                    coord_config.peers = peers;
                }
                if replicas != 3 {
                    coord_config.replicas = replicas;
                }
                // ... other fields if needed
            }
            let coord = Coordinator::new(coord_config, id);
            coord.serve().await?;
        }
    }

    Ok(())
}
