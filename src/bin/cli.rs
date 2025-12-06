//! CLI for cluster operations

use clap::{Parser, Subcommand};
use minikv::ops::{compact_cluster, repair_cluster, verify_cluster};

#[derive(Parser)]
#[command(name = "minikv")]
#[command(about = "minikv distributed key-value store CLI")]
#[command(version)]
struct Cli {
    /// Coordinator URL
    #[arg(long, default_value = "http://localhost:5000")]
    coordinator: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Verify cluster integrity
    Verify {
        /// Deep verification (checksums)
        #[arg(long)]
        deep: bool,

        /// Concurrency level
        #[arg(long, default_value = "16")]
        concurrency: usize,
    },

    /// Repair under-replicated keys
    Repair {
        /// Target replication factor
        #[arg(long, default_value = "3")]
        replicas: usize,

        /// Dry run
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
            println!("PUT not yet implemented");
        }

        Commands::Get { key, output } => {
            println!("GET not yet implemented");
        }

        Commands::Delete { key } => {
            println!("DELETE not yet implemented");
        }
    }

    Ok(())
}
