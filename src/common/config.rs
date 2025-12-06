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
            bind_addr: "0. 
