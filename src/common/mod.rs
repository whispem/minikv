//! Common utilities and types shared across minikv

pub mod config;
pub mod error;
pub mod hash;
pub mod metrics;
pub mod raft;
pub mod ratelimit;
pub mod tracing_middleware;
pub mod utils;

pub use config::{Config, CoordinatorConfig, NodeRole, RuntimeConfig, VolumeConfig, WalSyncPolicy};
pub use error::{Error, Result};
pub use hash::{
    blake3_hash, blob_prefix, hrw_hash, select_replicas, shard_key, Blake3Hasher,
    ConsistentHashRing,
};
pub use metrics::{Counter, Gauge, Histogram, MetricsRegistry, METRICS};
pub use ratelimit::{RateLimitConfig, RateLimitResult, RateLimitStats, RateLimiter};
pub use tracing_middleware::{
    generate_request_id, request_id_middleware, request_tracing_middleware, REQUEST_ID_HEADER,
};
pub use utils::{
    crc32, decode_key, encode_key, format_bytes, parse_duration, timestamp_now, NodeState,
};
