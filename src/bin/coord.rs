//! The `minikv-coord` binary.
//!
//! `serve` takes `bind_addr`, `grpc_addr`, `db_path`, `peers` and `replicas`
//! from its flags and from the `[coordinator]` section of the configuration
//! file (see [`minikv::common::Config::load`]). It does not apply the other
//! settings of that section, and logs a warning for each one that differs
//! from its default.

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
    Serve {
        #[arg(long)]
        id: String,

        #[arg(long, default_value = "0.0.0.0:8000")]
        bind: String,

        #[arg(long, default_value = "0.0.0.0:8001")]
        grpc: String,

        #[arg(long, default_value = "./coord-data")]
        db: PathBuf,

        #[arg(long, value_delimiter = ',')]
        peers: Vec<String>,

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
            let file_config = match minikv::common::config::Config::try_load() {
                Ok(config) => config.coordinator,
                Err(e) => {
                    tracing::info!("No usable config.toml ({}), using command line flags", e);
                    None
                }
            };
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
            if let Some(file_conf) = file_config {
                warn_about_ignored_settings(&file_conf);
                // A value from the file replaces the flag, unless it equals
                // the value it is compared with below: then the flag stays.
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
            }
            let coord = Coordinator::new(coord_config, id);
            coord.serve().await?;
        }
    }

    Ok(())
}

/// Logs a warning for each setting of the file's `[coordinator]` section that
/// `serve` does not apply, when it differs from the value in use.
fn warn_about_ignored_settings(file: &CoordinatorConfig) {
    let default = CoordinatorConfig::default();
    let mut ignored = Vec::new();
    if file.election_timeout_ms != default.election_timeout_ms {
        ignored.push(format!(
            "election_timeout_ms = {} (the coordinator uses {})",
            file.election_timeout_ms, default.election_timeout_ms
        ));
    }
    if file.heartbeat_interval_ms != default.heartbeat_interval_ms {
        ignored.push(format!(
            "heartbeat_interval_ms = {} (the coordinator uses {})",
            file.heartbeat_interval_ms, default.heartbeat_interval_ms
        ));
    }
    if file.snapshot_threshold != default.snapshot_threshold {
        ignored.push(format!(
            "snapshot_threshold = {} (reserved: there are no Raft snapshots)",
            file.snapshot_threshold
        ));
    }
    if file.num_shards != default.num_shards {
        ignored.push(format!(
            "num_shards = {} (reserved: placement uses no shards)",
            file.num_shards
        ));
    }
    if file.tls_cert_path.is_some() || file.tls_key_path.is_some() {
        ignored.push("tls_cert_path and tls_key_path (TLS stays off)".to_string());
    }
    for setting in ignored {
        tracing::warn!("[coordinator] {} is not applied by minikv-coord", setting);
    }
}
