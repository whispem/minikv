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
            let config = CoordinatorConfig {
                bind_addr: bind.parse()?,
                grpc_addr: grpc.parse()?,
                db_path: db,
                peers,
                replicas,
                ..Default::default()
            };

            let coord = Coordinator::new(config, id);
            coord.serve().await?;
        }
    }

    Ok(())
}
