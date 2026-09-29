//! Cluster operations: verify, repair, compact, rebalance, upgrade and stream.
//!
//! None of these operations is implemented yet. Each function returns an error
//! that names the feature and the release planned for it, and the coordinator
//! answers `501 Not Implemented` on the matching admin routes.
//!
//! [`NotImplemented`] lists every feature that minikv announces but does not
//! implement, including the HTTP ones (backups, restores, TTL) and the library
//! ones (Kubernetes operator, io_uring, Kafka sink, downsampling, tiering
//! compression), so that the `501` bodies, the CLI messages and the library
//! errors always agree.

pub mod compact;
pub mod repair;
pub mod verify;

pub use compact::{compact_cluster, stream_large_blob};
pub use repair::{auto_rebalance_cluster, repair_cluster};
pub use verify::{prepare_seamless_upgrade, verify_cluster};

/// The `roadmap` value of a feature that no release plans yet.
pub const UNSCHEDULED: &str = "unscheduled";

/// A feature that minikv does not implement yet, and the release planned for it.
///
/// Its `Display` form is the message shown to users, for example
/// `verify is not implemented (roadmap: 2.1.0)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotImplemented {
    /// Name of the feature, as it appears in `501` bodies.
    pub feature: &'static str,
    /// Planned release, or [`UNSCHEDULED`].
    pub roadmap: &'static str,
}

impl NotImplemented {
    /// Read-only cluster verification.
    pub const VERIFY: Self = Self {
        feature: "verify",
        roadmap: "2.1.0",
    };
    /// Re-replication of under-replicated keys.
    pub const REPAIR: Self = Self {
        feature: "repair",
        roadmap: "2.2.0",
    };
    /// Compaction of the volume segments.
    pub const COMPACT: Self = Self {
        feature: "compact",
        roadmap: "2.2.0",
    };
    /// Moving data between volumes.
    pub const REBALANCE: Self = Self {
        feature: "rebalance",
        roadmap: "2.2.0",
    };
    /// Adding or removing nodes at runtime.
    pub const SCALE: Self = Self {
        feature: "scale",
        roadmap: "2.2.0",
    };
    /// Rolling upgrade of the cluster.
    pub const UPGRADE: Self = Self {
        feature: "upgrade",
        roadmap: UNSCHEDULED,
    };
    /// Streaming a large blob from a volume.
    pub const STREAM: Self = Self {
        feature: "stream",
        roadmap: UNSCHEDULED,
    };
    /// Cluster backups.
    pub const BACKUP: Self = Self {
        feature: "backup",
        roadmap: UNSCHEDULED,
    };
    /// Restoring a cluster from a backup.
    pub const RESTORE: Self = Self {
        feature: "restore",
        roadmap: UNSCHEDULED,
    };
    /// Expiration of objects through the `X-Minikv-TTL` header.
    pub const TTL: Self = Self {
        feature: "ttl",
        roadmap: UNSCHEDULED,
    };
    /// The controller of the `MiniKVCluster` Kubernetes resources.
    pub const K8S_OPERATOR: Self = Self {
        feature: "kubernetes operator",
        roadmap: UNSCHEDULED,
    };
    /// Asynchronous I/O through Linux io_uring.
    pub const IO_URING: Self = Self {
        feature: "io_uring",
        roadmap: UNSCHEDULED,
    };
    /// Sending change data capture events to Kafka.
    pub const KAFKA_SINK: Self = Self {
        feature: "kafka sink",
        roadmap: UNSCHEDULED,
    };
    /// Downsampling of the time series.
    pub const DOWNSAMPLING: Self = Self {
        feature: "downsampling",
        roadmap: UNSCHEDULED,
    };
    /// Compression of the data in the warm, cold and archive tiers.
    pub const TIERING_COMPRESSION: Self = Self {
        feature: "tiering compression",
        roadmap: UNSCHEDULED,
    };
}

impl std::fmt::Display for NotImplemented {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is not implemented (roadmap: {})",
            self.feature, self.roadmap
        )
    }
}

impl std::error::Error for NotImplemented {}

impl From<NotImplemented> for crate::Error {
    fn from(feature: NotImplemented) -> Self {
        crate::Error::Other(feature.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_names_the_feature_and_the_roadmap() {
        assert_eq!(
            NotImplemented::VERIFY.to_string(),
            "verify is not implemented (roadmap: 2.1.0)"
        );
        assert_eq!(
            NotImplemented::UPGRADE.to_string(),
            "upgrade is not implemented (roadmap: unscheduled)"
        );
    }

    #[test]
    fn converts_into_the_crate_error_with_the_same_message() {
        let error: crate::Error = NotImplemented::REPAIR.into();
        assert_eq!(
            error.to_string(),
            "repair is not implemented (roadmap: 2.2.0)"
        );
    }
}
