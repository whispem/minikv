pub mod storage;
pub use storage::{KVStore, MemStore, Storage};
pub mod audit;
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

pub mod admin_ui;
pub mod backup;
pub mod cdc;
pub mod plugin;
pub mod replication;

pub mod geo;
pub mod io_uring;
pub mod k8s_operator;
pub mod tiering;
pub mod timeseries;
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
pub use hash::{blake3_hash, hrw_hash, select_replicas, shard_key, Blake3Hasher};
// Deprecated since 2.0.1, still exported for compatibility.
#[allow(deprecated)]
pub use hash::{blob_prefix, ConsistentHashRing};
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

pub use admin_ui::{admin_dashboard, create_admin_ui_router, DashboardStats};
pub use backup::{
    init_backup, BackupConfig, BackupDestination, BackupManager, BackupManifest, BackupProgress,
    BackupStatus, BackupType, RestoreConfig, BACKUP_MANAGER,
};
pub use cdc::{
    init_cdc, CDCConfig, CDCEvent, CDCManager, CDCMetadata, CDCOperation, SinkConfig, CDC_MANAGER,
};
pub use plugin::{
    get_plugin_manager, Plugin, PluginConfig, PluginInfo, PluginManager, PluginState, PluginType,
    PluginVersion, PLUGIN_MANAGER,
};
pub use replication::{
    get_replication_manager, init_replication, ConflictResolution, ReplicationConfig,
    ReplicationEvent, ReplicationEventType, ReplicationManager, ReplicationStatus, VectorClock,
    REPLICATION_MANAGER,
};

pub use geo::{GeoConfig, GeoLocation, GeoRouter, RegionConfig, RoutingStrategy};
pub use io_uring::{IoUring, IoUringConfig, IoUringStats};
pub use k8s_operator::{MiniKVClusterSpec, MiniKVController};
