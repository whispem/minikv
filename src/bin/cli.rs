//! Command-line client for minikv.
//!
//! `put`, `get` and `delete` talk to a coordinator over HTTP and fail with a
//! non-zero exit code when the coordinator answers with an error.
//!
//! The cluster operations (`verify`, `repair`, `compact`, `rebalance`,
//! `upgrade` and `stream`) are not implemented yet: each one prints an error
//! that names the release planned for it and exits with status 1.

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use minikv::ops::{
    auto_rebalance_cluster, compact_cluster, prepare_seamless_upgrade, repair_cluster,
    verify_cluster, NotImplemented,
};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "minikv")]
#[command(about = "minikv distributed key-value store CLI")]
#[command(version)]
struct Cli {
    /// HTTP address of a coordinator. Accepted before or after the subcommand.
    #[arg(long, global = true, default_value = "http://localhost:5000")]
    coordinator: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check the replicas of every key (not implemented yet)
    Verify {
        #[arg(long)]
        deep: bool,

        #[arg(long, default_value = "16")]
        concurrency: usize,
    },

    /// Re-replicate under-replicated keys (not implemented yet)
    Repair {
        #[arg(long, default_value = "3")]
        replicas: usize,

        #[arg(long)]
        dry_run: bool,
    },

    /// Compact the volume segments (not implemented yet)
    Compact {
        #[arg(long)]
        shard: Option<u64>,
    },

    /// Store the content of a file under a key
    Put {
        key: String,

        #[arg(long)]
        file: PathBuf,
    },

    /// Read a key, into a file or to standard output
    Get {
        key: String,

        /// File to write the value to. Without it, the value goes to standard output.
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Delete a key
    Delete { key: String },

    /// Move data between volumes (not implemented yet)
    Rebalance {},

    /// Prepare a rolling upgrade (not implemented yet)
    Upgrade {},

    /// Stream a large blob (not implemented yet)
    Stream {
        #[arg(long)]
        key: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {:#}", error);
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let coordinator = cli.coordinator.trim_end_matches('/').to_string();
    let url = |key: &str| format!("{}/{}", coordinator, key);

    match cli.command {
        Commands::Verify { deep, concurrency } => {
            let report = verify_cluster(&coordinator, deep, concurrency).await?;
            println!("Verification report:");
            println!("  Total keys: {}", report.total_keys);
            println!("  Healthy: {}", report.healthy);
            println!("  Under-replicated: {}", report.under_replicated);
            println!("  Corrupted: {}", report.corrupted);
            println!("  Orphaned: {}", report.orphaned);
        }

        Commands::Repair { replicas, dry_run } => {
            let report = repair_cluster(&coordinator, replicas, dry_run).await?;
            println!("Repair report:");
            println!("  Keys checked: {}", report.keys_checked);
            println!("  Keys repaired: {}", report.keys_repaired);
            println!("  Bytes copied: {}", report.bytes_copied);
        }

        Commands::Compact { shard } => {
            let report = compact_cluster(&coordinator, shard).await?;
            println!("Compaction report:");
            println!("  Volumes compacted: {}", report.volumes_compacted);
            println!("  Bytes freed: {}", report.bytes_freed);
        }

        Commands::Rebalance {} => {
            auto_rebalance_cluster(&coordinator).await?;
            println!("Rebalancing done.");
        }

        Commands::Upgrade {} => {
            prepare_seamless_upgrade(&coordinator).await?;
            println!("Upgrade prepared.");
        }

        Commands::Stream { key: _ } => {
            return Err(NotImplemented::STREAM.into());
        }

        Commands::Put { key, file } => {
            let value =
                std::fs::read(&file).with_context(|| format!("cannot read {}", file.display()))?;
            let response = reqwest::Client::new()
                .post(url(&key))
                .body(value)
                .send()
                .await
                .with_context(|| format!("cannot reach the coordinator at {}", coordinator))?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                bail!("PUT {} failed: {}: {}", key, status, body.trim());
            }
            println!("PUT {}: {}", key, status);
        }

        Commands::Get { key, output } => {
            let response = reqwest::get(url(&key))
                .await
                .with_context(|| format!("cannot reach the coordinator at {}", coordinator))?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                bail!("GET {} failed: {}: {}", key, status, body.trim());
            }
            let value = response
                .bytes()
                .await
                .with_context(|| format!("cannot read the value of {}", key))?;
            match output {
                Some(path) => {
                    std::fs::write(&path, &value)
                        .with_context(|| format!("cannot write {}", path.display()))?;
                    println!(
                        "GET {}: {} bytes written to {}",
                        key,
                        value.len(),
                        path.display()
                    );
                }
                None => {
                    let mut stdout = std::io::stdout().lock();
                    stdout.write_all(&value)?;
                    stdout.flush()?;
                }
            }
        }

        Commands::Delete { key } => {
            let response = reqwest::Client::new()
                .delete(url(&key))
                .send()
                .await
                .with_context(|| format!("cannot reach the coordinator at {}", coordinator))?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                bail!("DELETE {} failed: {}: {}", key, status, body.trim());
            }
            println!("DELETE {}: {}", key, status);
        }
    }

    Ok(())
}
