//! Configuration for minikv components

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Global configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Node ID (unique identifier)
    pub node_id: String,

    /// Role (coordinator or volume)
    pub role: NodeRole,

    /// Coordinator-specific config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinator: Option<CoordinatorConfig>,

    /// Volume-specific config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<VolumeConfig>,

    /// Logging level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Coordinator,
    Volume,
}

/// Coordinator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Bind address for HTTP API
    pub bind_addr: SocketAddr,

    /// Bind address for internal gRPC
    pub grpc_addr: SocketAddr,

    /// RocksDB path for metadata
    pub db_path: PathBuf,

    /// Raft peers (other coordinators)
    pub peers: Vec<String>,

    /// Replication factor
    #[serde(default = "default_replicas")]
    pub replicas: usize,

    /// Raft election timeout
    #[serde(default = "default_election_timeout")]
    pub election_timeout_ms: u64,

    /// Raft heartbeat interval
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_ms: u64,

    /// Snapshot threshold (log entries before snapshot)
    #[serde(default = "default_snapshot_threshold")]
    pub snapshot_threshold: u64,

    /// Number of shards for consistent hashing
    #[serde(default = "default_num_shards")]
    pub num_shards: u64,
}

fn default_replicas() -> usize {
    3
}
fn default_election_timeout() -> u64 {
    300
}
fn default_heartbeat_interval() -> u64 {
    50
}
fn default_snapshot_threshold() -> u64 {
    10_000
}
fn default_num_shards() -> u64 {
    256
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0. 0.0:5000".parse().unwrap(),
            grpc_addr: "0.0.0.0:5001".parse().unwrap(),
            db_path: PathBuf::from("./coord-data"),
            peers: vec![],
            replicas: default_replicas(),
            election_timeout_ms: default_election_timeout(),
            heartbeat_interval_ms: default_heartbeat_interval(),
            snapshot_threshold: default_snapshot_threshold(),
            num_shards: default_num_shards(),
        }
    }
}

/// Volume configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    /// Bind address for HTTP API
    pub bind_addr: SocketAddr,

    /// Bind address for internal gRPC
    pub grpc_addr: SocketAddr,

    /// Data directory for blobs
    pub data_path: PathBuf,

    /// WAL directory
    pub wal_path: PathBuf,

    /// Coordinator addresses
    pub coordinators: Vec<String>,

    /// Max blob size (bytes)
    #[serde(default = "default_max_blob_size")]
    pub max_blob_size: u64,

    /// Compaction interval
    #[serde(default = "default_compaction_interval")]
    pub compaction_interval_secs: u64,

    /// Compaction threshold (segments)
    #[serde(default = "default_compaction_threshold")]
    pub compaction_threshold: usize,

    /// Heartbeat interval
    #[serde(default = "default_volume_heartbeat")]
    pub heartbeat_interval_secs: u64,

    /// Enable bloom filters
    #[serde(default = "default_true")]
    pub enable_bloom: bool,

    /// Enable index snapshots
    #[serde(default = "default_true")]
    pub enable_snapshots: bool,

    /// WAL sync policy
    #[serde(default)]
    pub wal_sync: WalSyncPolicy,
}

fn default_max_blob_size() -> u64 {
    1024 * 1024 * 1024 // 1 GB
}
fn default_compaction_interval() -> u64 {
    300 // 5 minutes
}
fn default_compaction_threshold() -> usize {
    10
}
fn default_volume_heartbeat() -> u64 {
    10
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WalSyncPolicy {
    /// fsync after every write
    Always,
    /// fsync periodically
    Interval,
    /// Never fsync (fastest, least durable)
    Never,
}

impl Default for WalSyncPolicy {
    fn default() -> Self {
        WalSyncPolicy::Always
    }
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:6000".parse().unwrap(),
            grpc_addr: "0.0.0.0:6001".parse().unwrap(),
            data_path: PathBuf::from("./vol-data"),
            wal_path: PathBuf::from("./vol-wal"),
            coordinators: vec! ["http://localhost:5000".to_string()],
            max_blob_size: default_max_blob_size(),
            compaction_interval_secs: default_compaction_interval(),
            compaction_threshold: default_compaction_threshold(),
            heartbeat_interval_secs: default_volume_heartbeat(),
            enable_bloom: true,
            enable_snapshots: true,
            wal_sync: WalSyncPolicy::default(),
        }
    }
}

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Request timeout
    pub request_timeout: Duration,

    /// Connection timeout
    pub connect_timeout: Duration,

    /// Max concurrent requests
    pub max_concurrent_requests: usize,

    /// Retry attempts
    pub max_retries: usize,

    /// Retry delay
    pub retry_delay: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(5),
            max_concurrent_requests: 1000,
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
        }
    }
}

impl Config {
    /// Load from file
    pub fn from_file(path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)
            .map_err(|e| crate::Error::Other(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    /// Save to file
    pub fn to_file(&self, path: impl AsRef<std::path::Path>) -> crate::Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| crate::Error::Other(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, content)? ;
        Ok(())
    }

    /// Validate configuration
    pub fn validate(&self) -> crate::Result<()> {
        if self.node_id.is_empty() {
            return Err(crate::Error::InvalidConfig("node_id is required".into()));
        }

        match self.role {
            NodeRole::Coordinator => {
                if self.coordinator.is_none() {
                    return Err(crate::Error::InvalidConfig(
                        "coordinator config required".into(),
                    ));
                }
            }
            NodeRole::Volume => {
                if self.volume.is_none() {
                    return Err(crate::Error::InvalidConfig("volume config required".into()));
                }
            }
        }

        Ok(())
    }
}
