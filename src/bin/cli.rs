//! CLI for cluster operations
//!
//! This module implements the command-line interface for cluster management.
//! Provides commands for verification, repair, and compaction of the distributed key-value store.

use clap::{Parser, Subcommand};
use minikv::ops::{
    auto_rebalance_cluster, compact_cluster, prepare_seamless_upgrade, repair_cluster,
    stream_large_blob, verify_cluster,
};

/// CLI arguments for cluster management.
#[derive(Parser)]
#[command(name = "minikv")]
#[command(about = "minikv distributed key-value store CLI")]
#[command(version)]
struct Cli {
    /// Coordinator URL
    #[arg(long, default_value = "http://localhost:5000")]
    coordinator: String,

    /// Cluster operation to perform
    #[command(subcommand)]
    command: Commands,
}

/// Supported cluster operations for the CLI.
#[derive(Subcommand)]
enum Commands {
    /// Verify cluster integrity
    /// Checks for missing or corrupted keys, optionally verifies checksums.
    Verify {
        /// Deep verification (checksums)
        #[arg(long)]
        deep: bool,

        /// Concurrency level for verification
        #[arg(long, default_value = "16")]
        concurrency: usize,
    },

    /// Repair under-replicated keys
    /// Attempts to restore the desired replication factor for all keys.
    Repair {
        /// Target replication factor
        #[arg(long, default_value = "3")]
        replicas: usize,

        /// Dry run (do not perform actual repair)
        #[arg(long)]
        dry_run: bool,
    },

    /// Compact cluster
    Compact {
        /// Specific shard (all if omitted)
        #[arg(long)]
        shard: Option<u64>,
    },

    /// Put a blob
    Put {
        /// Key
        key: String,

        /// File path
        #[arg(long)]
        file: std::path::PathBuf,
    },

    /// Get a blob
    Get {
        /// Key
        key: String,

        /// Output file
        #[arg(long)]
        output: std::path::PathBuf,
    },

    /// Delete a blob
    Delete {
        /// Key
        key: String,
    },

    /// Trigger auto-rebalancing of cluster data
    Rebalance {},

    /// Prepare cluster for seamless upgrade
    Upgrade {},

    /// Stream a large blob by key
    Stream {
        /// Key to stream
        #[arg(long)]
        key: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Verify { deep, concurrency } => {
            let report = verify_cluster(&cli.coordinator, deep, concurrency).await?;
            println!("Verification report:");
            println!("  Total keys: {}", report.total_keys);
            println!("  Healthy: {}", report.healthy);
            println!("  Under-replicated: {}", report.under_replicated);
            println!("  Corrupted: {}", report.corrupted);
            println!("  Orphaned: {}", report.orphaned);
        }

        Commands::Repair { replicas, dry_run } => {
            let report = repair_cluster(&cli.coordinator, replicas, dry_run).await?;
            println!("Repair report:");
            println!("  Keys checked: {}", report.keys_checked);
            println!("  Keys repaired: {}", report.keys_repaired);
            println!("  Bytes copied: {}", report.bytes_copied);
        }

        Commands::Compact { shard } => {
            let report = compact_cluster(&cli.coordinator, shard).await?;
            println!("Compaction report:");
            println!("  Volumes compacted: {}", report.volumes_compacted);
            println!("  Bytes freed: {}", report.bytes_freed);
        }

        Commands::Put { key, file } => {
            // Read value from file
            let value = std::fs::read(&file)?;
            let url = format!("{}/{}", cli.coordinator, key);
            let client = reqwest::Client::new();
            let resp = client.post(&url).body(value).send().await?;
            println!("PUT {}: {}", key, resp.status());
        }

        Commands::Rebalance {} => {
            auto_rebalance_cluster(&cli.coordinator).await?;
            println!("Auto-rebalancing triggered.");
        }

        Commands::Upgrade {} => {
            prepare_seamless_upgrade(&cli.coordinator).await?;
            println!("Seamless upgrade prepared.");
        }

        Commands::Stream { key } => {
            stream_large_blob("volume-1", &key).await?;
            println!("Streaming large blob for key: {}", key);
        }

        Commands::Get { key, output } => {
            let url = format!("{}/{}", cli.coordinator, key);
            let resp = reqwest::get(&url).await?;
            let value = resp.text().await?;
            if output.as_os_str().is_empty() {
                println!("GET {}: {}", key, value);
            } else {
                std::fs::write(&output, &value)?;
                println!("GET {}: value written to file", key);
            }
        }

        Commands::Delete { key } => {
            let url = format!("{}/{}", cli.coordinator, key);
            let client = reqwest::Client::new();
            let resp = client.delete(&url).send().await?;
            println!("DELETE {}: {}", key, resp.status());
        }
    }

    Ok(())
}
