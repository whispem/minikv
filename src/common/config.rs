//! The configuration file format.
//!
//! Only `minikv-coord` reads a configuration file, and it only applies part of
//! the `[coordinator]` section: see [`CoordinatorConfig`]. `minikv-volume`
//! takes its settings from its command line.

impl Config {
    /// Reads `config.toml`, then `config.local.toml` (both optional, in the
    /// working directory), then the environment variables whose names start
    /// with `MINIKV_`. Panics when the result is not a valid configuration.
    ///
    /// The environment uses `_` both after the prefix and between nested
    /// keys, so it can only set keys whose names contain no `_`: `MINIKV_ROLE`
    /// sets `role` and `MINIKV_COORDINATOR_REPLICAS` sets
    /// `coordinator.replicas`, but `MINIKV_NODE_ID` sets `node.id`, not
    /// `node_id`.
    pub fn load() -> Self {
        Self::builder()
            .build()
            .expect("Failed to load config")
            .try_deserialize()
            .expect("Failed to parse config")
    }

    /// Same as [`Config::load`], but returns the error instead of panicking.
    pub fn try_load() -> std::result::Result<Self, config::ConfigError> {
        Self::builder().build()?.try_deserialize()
    }

    fn builder() -> config::ConfigBuilder<config::builder::DefaultState> {
        config::Config::builder()
            .add_source(config::File::with_name("config.toml").required(false))
            .add_source(config::File::with_name("config.local.toml").required(false))
            .add_source(config::Environment::with_prefix("MINIKV").separator("_"))
    }
}

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Required by the format. `minikv-coord` uses its `--id` flag instead.
    pub node_id: String,

    pub role: NodeRole,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinator: Option<CoordinatorConfig>,

    /// Not read: see [`VolumeConfig`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<VolumeConfig>,

    /// Not read: the binaries take their log level from `RUST_LOG`.
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

/// Settings of a coordinator.
///
/// [`crate::Coordinator`] applies all of them, but `minikv-coord` only takes
/// `bind_addr`, `grpc_addr`, `db_path`, `peers` and `replicas` from the
/// configuration file: it uses the defaults for the other fields, and logs a
/// warning for each one that the file sets to another value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// HTTP API.
    pub bind_addr: SocketAddr,

    /// gRPC, between the nodes.
    pub grpc_addr: SocketAddr,

    /// Metadata, Raft log, vector index and audit log.
    pub db_path: PathBuf,

    /// gRPC addresses of the other coordinators.
    pub peers: Vec<String>,

    /// Number of volumes that store each value.
    #[serde(default = "default_replicas")]
    pub replicas: usize,

    /// Raft election timeout, in milliseconds.
    #[serde(default = "default_election_timeout")]
    pub election_timeout_ms: u64,

    /// Interval between the heartbeats of the Raft leader, in milliseconds.
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_ms: u64,

    /// Reserved: nothing reads it, because the coordinators do not take Raft
    /// snapshots yet. Kept for compatibility.
    #[serde(default = "default_snapshot_threshold")]
    pub snapshot_threshold: u64,

    /// Reserved for the virtual shards planned for v2.2.0. Placement does not
    /// use shards: it ranks the live volumes with HRW on each key. Kept for
    /// compatibility.
    #[serde(default = "default_num_shards")]
    pub num_shards: u64,

    /// PEM certificate. With `tls_key_path`, [`crate::Coordinator`] serves
    /// HTTP and gRPC over TLS. `minikv-coord` cannot enable TLS.
    #[serde(default)]
    pub tls_cert_path: Option<String>,

    /// PEM private key: see `tls_cert_path`.
    #[serde(default)]
    pub tls_key_path: Option<String>,
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
            bind_addr: "0.0.0.0:8000".parse().unwrap(),
            grpc_addr: "0.0.0.0:8001".parse().unwrap(),
            db_path: PathBuf::from("./coord-data"),
            peers: vec![],
            replicas: default_replicas(),
            election_timeout_ms: default_election_timeout(),
            heartbeat_interval_ms: default_heartbeat_interval(),
            snapshot_threshold: default_snapshot_threshold(),
            num_shards: default_num_shards(),
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

/// Settings of a volume server, in the configuration file format.
///
/// Not read: `minikv-volume` takes its settings from its command line, and
/// nothing else uses this struct. The volume server always syncs its WAL with
/// [`WalSyncPolicy::Always`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub bind_addr: SocketAddr,

    pub grpc_addr: SocketAddr,

    pub data_path: PathBuf,

    pub wal_path: PathBuf,

    pub coordinators: Vec<String>,

    #[serde(default = "default_max_blob_size")]
    pub max_blob_size: u64,

    #[serde(default = "default_compaction_interval")]
    pub compaction_interval_secs: u64,

    #[serde(default = "default_compaction_threshold")]
    pub compaction_threshold: usize,

    #[serde(default = "default_volume_heartbeat")]
    pub heartbeat_interval_secs: u64,

    #[serde(default = "default_true")]
    pub enable_bloom: bool,

    #[serde(default = "default_true")]
    pub enable_snapshots: bool,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WalSyncPolicy {
    /// Flushes and fsyncs the WAL after every write.
    #[default]
    Always,
    /// Flushes the WAL to the operating system after every write, without
    /// fsync: a machine crash can lose the last writes. Despite the name,
    /// nothing runs periodically.
    Interval,
    /// Leaves each write in the WAL's memory buffer until the buffer fills or
    /// the WAL is flushed: a crash of the process can lose the last writes.
    Never,
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:6000".parse().unwrap(),
            grpc_addr: "0.0.0.0:6001".parse().unwrap(),
            data_path: PathBuf::from("./vol-data"),
            wal_path: PathBuf::from("./vol-wal"),
            coordinators: vec!["http://localhost:8000".to_string()],
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

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub request_timeout: Duration,

    pub connect_timeout: Duration,

    pub max_concurrent_requests: usize,

    pub max_retries: usize,

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
    #[allow(clippy::result_large_err)]
    pub fn from_file(path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)
            .map_err(|e| crate::Error::Other(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    #[allow(clippy::result_large_err)]
    pub fn to_file(&self, path: impl AsRef<std::path::Path>) -> crate::Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| crate::Error::Other(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    #[allow(clippy::result_large_err)]
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
