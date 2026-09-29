//! Types of the `MiniKVCluster` Kubernetes resource.
//!
//! The operator itself is not implemented: minikv has no Kubernetes client,
//! so [`MiniKVController::run`], [`MiniKVController::reconcile`] and
//! [`MiniKVController::handle_delete`] fail. This crate builds no operator
//! binary.

use crate::common::Result;
use crate::ops::NotImplemented;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiniKVClusterSpec {
    pub coordinators: CoordinatorSpec,

    pub volumes: VolumeSpec,

    #[serde(default)]
    pub security: SecuritySpec,

    #[serde(default)]
    pub observability: ObservabilitySpec,

    #[serde(default)]
    pub autoscaling: AutoscalingSpec,

    #[serde(default)]
    pub backup: BackupSpec,

    #[serde(default)]
    pub geo: GeoSpec,

    #[serde(default)]
    pub timeseries: TimeseriesSpec,

    #[serde(default)]
    pub tiering: TieringSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorSpec {
    pub replicas: u32,

    #[serde(default = "default_coordinator_image")]
    pub image: String,

    #[serde(default)]
    pub resources: ResourceRequirements,

    #[serde(default)]
    pub storage: StorageSpec,
}

fn default_coordinator_image() -> String {
    "ghcr.io/whispem/minikv-coord:latest".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSpec {
    pub replicas: u32,

    #[serde(default = "default_volume_image")]
    pub image: String,

    #[serde(default = "default_replication_factor")]
    pub replication_factor: u32,

    #[serde(default)]
    pub resources: ResourceRequirements,

    #[serde(default)]
    pub storage: StorageSpec,
}

fn default_volume_image() -> String {
    "ghcr.io/whispem/minikv-volume:latest".to_string()
}

fn default_replication_factor() -> u32 {
    3
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceRequirements {
    #[serde(default)]
    pub requests: ResourceList,
    #[serde(default)]
    pub limits: ResourceList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceList {
    #[serde(default = "default_cpu_request")]
    pub cpu: String,
    #[serde(default = "default_memory_request")]
    pub memory: String,
}

impl Default for ResourceList {
    fn default() -> Self {
        Self {
            cpu: default_cpu_request(),
            memory: default_memory_request(),
        }
    }
}

fn default_cpu_request() -> String {
    "100m".to_string()
}

fn default_memory_request() -> String {
    "256Mi".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSpec {
    #[serde(default = "default_storage_size")]
    pub size: String,
    #[serde(default)]
    pub storage_class_name: String,
}

impl Default for StorageSpec {
    fn default() -> Self {
        Self {
            size: default_storage_size(),
            storage_class_name: String::new(),
        }
    }
}

fn default_storage_size() -> String {
    "10Gi".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecuritySpec {
    #[serde(default)]
    pub tls: TlsSpec,
    #[serde(default)]
    pub authentication: AuthSpec,
    #[serde(default)]
    pub encryption: EncryptionSpec,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub secret_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub admin_secret_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionSpec {
    #[serde(default)]
    pub at_rest: bool,
    #[serde(default)]
    pub key_secret_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObservabilitySpec {
    #[serde(default)]
    pub metrics: MetricsSpec,
    #[serde(default)]
    pub tracing: TracingSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSpec {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_metrics_port")]
    pub port: u16,
}

impl Default for MetricsSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            port: default_metrics_port(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_metrics_port() -> u16 {
    9090
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TracingSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoscalingSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_min_replicas")]
    pub min_replicas: u32,
    #[serde(default = "default_max_replicas")]
    pub max_replicas: u32,
    #[serde(default = "default_cpu_target")]
    pub target_cpu_utilization: u32,
    #[serde(default = "default_memory_target")]
    pub target_memory_utilization: u32,
    #[serde(default = "default_scale_down_stabilization")]
    pub scale_down_stabilization: u32,
}

impl Default for AutoscalingSpec {
    fn default() -> Self {
        Self {
            enabled: false,
            min_replicas: default_min_replicas(),
            max_replicas: default_max_replicas(),
            target_cpu_utilization: default_cpu_target(),
            target_memory_utilization: default_memory_target(),
            scale_down_stabilization: default_scale_down_stabilization(),
        }
    }
}

fn default_min_replicas() -> u32 {
    3
}
fn default_max_replicas() -> u32 {
    10
}
fn default_cpu_target() -> u32 {
    70
}
fn default_memory_target() -> u32 {
    80
}
fn default_scale_down_stabilization() -> u32 {
    300
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_backup_schedule")]
    pub schedule: String,
    #[serde(default = "default_backup_retention")]
    pub retention: u32,
    #[serde(default)]
    pub destination: BackupDestinationSpec,
}

fn default_backup_schedule() -> String {
    "0 2 * * *".to_string()
}

fn default_backup_retention() -> u32 {
    7
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDestinationSpec {
    #[serde(default = "default_backup_type")]
    pub r#type: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub secret_name: String,
}

fn default_backup_type() -> String {
    "s3".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub zone: String,
    #[serde(default)]
    pub remote_regions: Vec<RemoteRegionSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRegionSpec {
    pub name: String,
    pub endpoint: String,
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeseriesSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default)]
    pub downsample_rules: Vec<DownsampleRule>,
}

fn default_retention_days() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownsampleRule {
    pub after: String,
    pub resolution: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TieringSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub hot_tier: TierConfig,
    #[serde(default)]
    pub warm_tier: TierConfig,
    #[serde(default)]
    pub cold_tier: TierConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TierConfig {
    #[serde(default)]
    pub max_age: String,
    #[serde(default)]
    pub storage_class: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiniKVClusterStatus {
    pub phase: ClusterPhase,

    #[serde(default)]
    pub conditions: Vec<ClusterCondition>,

    #[serde(default)]
    pub coordinator_status: ComponentStatus,

    #[serde(default)]
    pub volume_status: VolumeStatus,

    #[serde(default)]
    pub endpoints: ClusterEndpoints,

    #[serde(default)]
    pub version: String,

    #[serde(default)]
    pub last_backup: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClusterPhase {
    #[default]
    Pending,
    Creating,
    Running,
    Updating,
    Failed,
    Deleting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterCondition {
    pub r#type: String,
    pub status: String,
    pub last_transition_time: DateTime<Utc>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub ready: u32,
    pub total: u32,
    #[serde(default)]
    pub leader: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeStatus {
    pub ready: u32,
    pub total: u32,
    #[serde(default)]
    pub total_storage: String,
    #[serde(default)]
    pub used_storage: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterEndpoints {
    #[serde(default)]
    pub http: String,
    #[serde(default)]
    pub grpc: String,
    #[serde(default)]
    pub metrics: String,
}

/// The controller of the `MiniKVCluster` resources. It is not implemented:
/// every method but [`MiniKVController::new`] fails.
pub struct MiniKVController {
    #[allow(dead_code)] // For the controller that minikv does not implement.
    config: ControllerConfig,
}

#[derive(Debug, Clone)]
pub struct ControllerConfig {
    /// Namespace to watch (empty = all namespaces)
    pub watch_namespace: String,

    pub reconcile_interval_secs: u64,

    pub leader_election: bool,

    pub metrics_bind_address: String,

    pub health_probe_bind_address: String,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            watch_namespace: String::new(),
            reconcile_interval_secs: 30,
            leader_election: true,
            metrics_bind_address: ":8080".to_string(),
            health_probe_bind_address: ":8081".to_string(),
        }
    }
}

impl MiniKVController {
    pub fn new(config: ControllerConfig) -> Self {
        Self { config }
    }

    /// Fails: minikv has no Kubernetes client, so it cannot watch the
    /// `MiniKVCluster` resources.
    pub async fn run(&self) -> Result<()> {
        Err(NotImplemented::K8S_OPERATOR.into())
    }

    /// Fails: minikv has no Kubernetes client, so it can neither read the
    /// resource nor create the StatefulSets, Services and ConfigMaps of a
    /// cluster.
    pub async fn reconcile(&self, _name: &str, _namespace: &str) -> Result<ReconcileAction> {
        Err(NotImplemented::K8S_OPERATOR.into())
    }

    /// Fails: minikv has no Kubernetes client, so it cannot delete the
    /// resources of a cluster.
    pub async fn handle_delete(&self, _name: &str, _namespace: &str) -> Result<()> {
        Err(NotImplemented::K8S_OPERATOR.into())
    }
}

#[derive(Debug, Clone)]
pub enum ReconcileAction {
    Continue,
    Skip,
    RequeueAfter(std::time::Duration),
    Requeue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_spec() {
        let spec = MiniKVClusterSpec {
            coordinators: CoordinatorSpec {
                replicas: 3,
                image: default_coordinator_image(),
                resources: ResourceRequirements::default(),
                storage: StorageSpec::default(),
            },
            volumes: VolumeSpec {
                replicas: 3,
                image: default_volume_image(),
                replication_factor: 3,
                resources: ResourceRequirements::default(),
                storage: StorageSpec::default(),
            },
            security: SecuritySpec::default(),
            observability: ObservabilitySpec::default(),
            autoscaling: AutoscalingSpec::default(),
            backup: BackupSpec::default(),
            geo: GeoSpec::default(),
            timeseries: TimeseriesSpec::default(),
            tiering: TieringSpec::default(),
        };

        assert_eq!(spec.coordinators.replicas, 3);
        assert_eq!(spec.volumes.replication_factor, 3);
        assert!(!spec.autoscaling.enabled);
    }

    #[tokio::test]
    async fn the_controller_fails_instead_of_reporting_success() {
        let controller = MiniKVController::new(ControllerConfig::default());
        let message = "kubernetes operator is not implemented (roadmap: unscheduled)";

        assert_eq!(controller.run().await.unwrap_err().to_string(), message);
        assert_eq!(
            controller
                .reconcile("my-minikv", "default")
                .await
                .unwrap_err()
                .to_string(),
            message
        );
        assert_eq!(
            controller
                .handle_delete("my-minikv", "default")
                .await
                .unwrap_err()
                .to_string(),
            message
        );
    }

    #[test]
    fn test_cluster_phases() {
        assert_eq!(ClusterPhase::default(), ClusterPhase::Pending);
    }

    #[test]
    fn test_vector_clock_serialize() {
        let status = MiniKVClusterStatus {
            phase: ClusterPhase::Running,
            ..Default::default()
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Running"));
    }
}
