//! Common utilities and types shared across minikv

pub mod config;
pub mod error;
pub mod hash;
pub mod raft;
pub mod utils;

pub use config::{Config, CoordinatorConfig, NodeRole, RuntimeConfig, VolumeConfig, WalSyncPolicy};
pub use error::{Error, Result};
pub use hash::{
    blake3_hash, blob_prefix, hrw_hash, select_replicas, shard_key, Blake3Hasher,
    ConsistentHashRing,
};
pub use utils::{
    crc32, decode_key, encode_key, format_bytes, parse_duration, timestamp_now, NodeState,
};
