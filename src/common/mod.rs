//! Common utilities and types shared across minikv

pub mod config;
pub mod error;
pub mod hash;
pub mod utils;

pub use config::{Config, CoordinatorConfig, NodeRole, RuntimeConfig, VolumeConfig, WalSyncPolicy};
pub use error::{Error, Result};
pub use hash::{blake3_hash, hrw_hash, shard_key, select_replicas, ConsistentHashRing, Blake3Hasher, blob_prefix};
pub use utils::{decode_key, encode_key, format_bytes, parse_duration, timestamp_now, NodeState, crc32};
