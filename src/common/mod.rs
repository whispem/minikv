pub mod storage;
pub use storage::{KVStore, MemStore, Storage};
pub mod audit;
/// Common utilities and types shared across minikv
pub mod auth;
pub mod auth_middleware;
pub mod config;
pub mod encryption;
pub mod error;
pub mod hash;
pub mod metrics;
pub mod quota;
pub mod raft;
pub mod ratelimit;
pub mod tracing_middleware;
pub mod utils;

pub use auth::{ApiKey, AuthConfig, AuthContext, AuthError, AuthResult, KeyStore, Role, KEY_STORE};
pub use auth_middleware::{
    auth_middleware, get_tenant_from_request, is_admin_request, require_admin_middleware,
    require_write_middleware, AuthExtension, AuthState,
};
pub use config::{Config, CoordinatorConfig, NodeRole, RuntimeConfig, VolumeConfig, WalSyncPolicy};
pub use encryption::{
    maybe_decrypt, maybe_encrypt, EncryptedData, EncryptionConfig, EncryptionError,
    EncryptionManager, EncryptionResult, EncryptionStatus, ENCRYPTION_MANAGER,
};
pub use error::{Error, Result};
pub use hash::{
    blake3_hash, blob_prefix, hrw_hash, select_replicas, shard_key, Blake3Hasher,
    ConsistentHashRing,
};
pub use metrics::{Counter, Gauge, Histogram, MetricsRegistry, METRICS};
pub use quota::{QuotaCheckResult, QuotaManager, TenantQuota, TenantUsage, QUOTA_MANAGER};
pub use ratelimit::{RateLimitConfig, RateLimitResult, RateLimitStats, RateLimiter};
pub use tracing_middleware::{
    generate_request_id, request_id_middleware, request_tracing_middleware, REQUEST_ID_HEADER,
};
pub use utils::{
    crc32, decode_key, encode_key, format_bytes, parse_duration, timestamp_now, NodeState,
};

pub use audit::{AuditEntry, AuditEventType, AuditLogger, AUDIT_LOGGER};
