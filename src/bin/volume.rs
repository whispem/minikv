use clap::{Parser, Subcommand};
use minikv::volume::server::{VolumeServer, VolumeServerConfig};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
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
    Serve {
        #[arg(long)]
        id: String,

        #[arg(long, default_value = "0.0.0.0:6000")]
        bind: SocketAddr,

        #[arg(long, default_value = "0.0.0.0:6001")]
        grpc: SocketAddr,

        #[arg(long, default_value = "./vol-data")]
        data: PathBuf,

        #[arg(long, default_value = "./vol-wal")]
        wal: PathBuf,

        #[arg(long, value_delimiter = ',', required = true)]
        coordinators: Vec<String>,

        #[arg(long)]
        advertise: Option<String>,

        #[arg(long, default_value = "1000")]
        heartbeat_ms: u64,
    },
}

fn advertised_address(grpc: SocketAddr, advertise: Option<String>) -> String {
    match advertise {
        Some(addr) if addr.starts_with("http://") || addr.starts_with("https://") => addr,
        Some(addr) => format!("http://{}", addr),
        None if grpc.ip().is_unspecified() => format!("http://127.0.0.1:{}", grpc.port()),
        None => format!("http://{}", grpc),
    }
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
            data,
            wal,
            coordinators,
            advertise,
            heartbeat_ms,
        } => {
            let config = VolumeServerConfig {
                volume_id: id,
                http_addr: bind,
                grpc_addr: grpc,
                advertise_grpc: advertised_address(grpc, advertise),
                data_path: data,
                wal_path: wal,
                coordinators: coordinators
                    .into_iter()
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect(),
                heartbeat_interval: Duration::from_millis(heartbeat_ms.max(50)),
            };
            VolumeServer::open(config)?.serve().await?;
        }
    }

    Ok(())
}
