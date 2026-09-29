//! Cross-datacenter replication primitives: vector clocks, conflict
//! resolution, and a manager that queues replication events.
//!
//! Nothing in minikv replicates across datacenters: [`init_replication`] has
//! no caller, so `/admin/replication/status` reports replication as not
//! configured, and nothing reads the channel that
//! [`ReplicationManager::queue_event`] feeds, so no event reaches a remote
//! datacenter.

use crate::common::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

pub type DatacenterId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// This datacenter's ID
    pub local_dc: DatacenterId,

    pub remote_dcs: Vec<RemoteDatacenter>,

    #[serde(default)]
    pub conflict_resolution: ConflictResolution,

    #[serde(default = "default_true")]
    pub async_replication: bool,

    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    #[serde(default = "default_replication_interval")]
    pub replication_interval_ms: u64,

    #[serde(default = "default_max_lag")]
    pub max_lag_secs: u64,
}

fn default_true() -> bool {
    true
}

fn default_batch_size() -> usize {
    100
}

fn default_replication_interval() -> u64 {
    1000
}

fn default_max_lag() -> u64 {
    60
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            local_dc: "dc1".to_string(),
            remote_dcs: vec![],
            conflict_resolution: ConflictResolution::default(),
            async_replication: true,
            batch_size: 100,
            replication_interval_ms: 1000,
            max_lag_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDatacenter {
    pub id: DatacenterId,

    pub name: String,

    pub endpoints: Vec<String>,

    #[serde(default)]
    pub priority: u32,

    #[serde(default)]
    pub region: String,

    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    #[default]
    LastWriteWins,

    VectorClock,

    LocalFirst,

    /// The event of the datacenter `dc1` wins; between two other
    /// datacenters, the latest event wins.
    PrimaryFirst,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VectorClock {
    pub clocks: HashMap<DatacenterId, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    pub fn increment(&mut self, dc: &DatacenterId) {
        *self.clocks.entry(dc.clone()).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (dc, &ts) in &other.clocks {
            let entry = self.clocks.entry(dc.clone()).or_insert(0);
            *entry = (*entry).max(ts);
        }
    }

    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        let self_dominates = self.dominates(other);
        let other_dominates = other.dominates(self);
        !self_dominates && !other_dominates
    }

    pub fn dominates(&self, other: &VectorClock) -> bool {
        let mut dominated = false;
        for (dc, &ts) in &other.clocks {
            let self_ts = self.clocks.get(dc).copied().unwrap_or(0);
            if self_ts < ts {
                return false;
            }
            if self_ts > ts {
                dominated = true;
            }
        }
        dominated
    }

    pub fn get(&self, dc: &DatacenterId) -> u64 {
        self.clocks.get(dc).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationEvent {
    pub id: String,

    pub source_dc: DatacenterId,

    pub event_type: ReplicationEventType,

    pub key: String,

    pub value: Option<Vec<u8>>,

    pub timestamp: DateTime<Utc>,

    pub vector_clock: Option<VectorClock>,

    pub tenant: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationEventType {
    Put,
    Delete,
    Snapshot,
}

/// The replication state of one remote datacenter, as recorded with
/// [`ReplicationManager::update_status`]. Nothing in minikv records it: a
/// remote datacenter starts unhealthy, with `last_error` saying that it was
/// never contacted, and stays so.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    pub dc_id: DatacenterId,

    pub last_replicated_id: Option<String>,

    pub last_replicated_at: Option<DateTime<Utc>>,

    /// Not measured: 0 unless [`ReplicationManager::update_status`] sets it.
    pub lag_secs: u64,

    /// Not counted: 0 unless [`ReplicationManager::update_status`] sets it.
    pub pending_events: usize,

    pub healthy: bool,

    pub last_error: Option<String>,
}

/// `last_error` of a remote datacenter that was never contacted.
const NEVER_CONTACTED: &str = "never contacted: minikv sends no event to remote datacenters";

pub struct ReplicationManager {
    config: ReplicationConfig,

    pending_events: Arc<RwLock<Vec<ReplicationEvent>>>,

    status: Arc<RwLock<HashMap<DatacenterId, ReplicationStatus>>>,

    event_tx: mpsc::Sender<ReplicationEvent>,

    shutdown: Arc<RwLock<bool>>,
}

impl ReplicationManager {
    pub fn new(config: ReplicationConfig) -> (Self, mpsc::Receiver<ReplicationEvent>) {
        let (event_tx, event_rx) = mpsc::channel(10000);

        let mut status = HashMap::new();
        for dc in &config.remote_dcs {
            status.insert(
                dc.id.clone(),
                ReplicationStatus {
                    dc_id: dc.id.clone(),
                    last_replicated_id: None,
                    last_replicated_at: None,
                    lag_secs: 0,
                    pending_events: 0,
                    healthy: false,
                    last_error: Some(NEVER_CONTACTED.to_string()),
                },
            );
        }

        let manager = Self {
            config,
            pending_events: Arc::new(RwLock::new(Vec::new())),
            status: Arc::new(RwLock::new(status)),
            event_tx,
            shutdown: Arc::new(RwLock::new(false)),
        };

        (manager, event_rx)
    }

    /// Sends `event` to the channel returned by [`ReplicationManager::new`],
    /// and keeps a copy in memory. Nothing in minikv reads either of them.
    pub async fn queue_event(&self, event: ReplicationEvent) -> Result<()> {
        self.event_tx
            .send(event.clone())
            .await
            .map_err(|e| Error::Other(format!("Failed to queue replication event: {}", e)))?;

        let mut pending = self.pending_events.write().unwrap();
        pending.push(event);

        Ok(())
    }

    pub fn create_put_event(
        &self,
        key: &str,
        value: Vec<u8>,
        tenant: Option<String>,
    ) -> ReplicationEvent {
        ReplicationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            source_dc: self.config.local_dc.clone(),
            event_type: ReplicationEventType::Put,
            key: key.to_string(),
            value: Some(value),
            timestamp: Utc::now(),
            vector_clock: None,
            tenant,
        }
    }

    pub fn create_delete_event(&self, key: &str, tenant: Option<String>) -> ReplicationEvent {
        ReplicationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            source_dc: self.config.local_dc.clone(),
            event_type: ReplicationEventType::Delete,
            key: key.to_string(),
            value: None,
            timestamp: Utc::now(),
            vector_clock: None,
            tenant,
        }
    }

    pub fn get_status(&self) -> Vec<ReplicationStatus> {
        self.status.read().unwrap().values().cloned().collect()
    }

    pub fn get_dc_status(&self, dc_id: &str) -> Option<ReplicationStatus> {
        self.status.read().unwrap().get(dc_id).cloned()
    }

    pub fn update_status(&self, dc_id: &str, update: impl FnOnce(&mut ReplicationStatus)) {
        if let Some(status) = self.status.write().unwrap().get_mut(dc_id) {
            update(status);
        }
    }

    pub fn resolve_conflict<'a>(
        &self,
        local: &'a ReplicationEvent,
        remote: &'a ReplicationEvent,
    ) -> &'a ReplicationEvent {
        match self.config.conflict_resolution {
            ConflictResolution::LastWriteWins => {
                if local.timestamp >= remote.timestamp {
                    local
                } else {
                    remote
                }
            }
            ConflictResolution::VectorClock => {
                if let (Some(local_vc), Some(remote_vc)) =
                    (&local.vector_clock, &remote.vector_clock)
                {
                    if local_vc.dominates(remote_vc) {
                        local
                    } else if remote_vc.dominates(local_vc) {
                        remote
                    } else if local.timestamp >= remote.timestamp {
                        local
                    } else {
                        remote
                    }
                } else if local.timestamp >= remote.timestamp {
                    local
                } else {
                    remote
                }
            }
            ConflictResolution::LocalFirst => local,
            ConflictResolution::PrimaryFirst => {
                if local.source_dc == "dc1" {
                    local
                } else if remote.source_dc == "dc1" {
                    remote
                } else if local.timestamp >= remote.timestamp {
                    local
                } else {
                    remote
                }
            }
        }
    }

    pub fn config(&self) -> &ReplicationConfig {
        &self.config
    }

    /// `true` when every remote datacenter is healthy and within
    /// `max_lag_secs`. Remote datacenters start unhealthy: only
    /// [`ReplicationManager::update_status`] changes that.
    pub fn is_healthy(&self) -> bool {
        self.status
            .read()
            .unwrap()
            .values()
            .all(|s| s.healthy && s.lag_secs <= self.config.max_lag_secs)
    }

    /// Sets a flag that nothing reads: the manager runs no task to stop.
    pub fn shutdown(&self) {
        *self.shutdown.write().unwrap() = true;
    }
}

pub static REPLICATION_MANAGER: once_cell::sync::Lazy<RwLock<Option<ReplicationManager>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

pub fn init_replication(config: ReplicationConfig) -> mpsc::Receiver<ReplicationEvent> {
    let (manager, rx) = ReplicationManager::new(config);
    *REPLICATION_MANAGER.write().unwrap() = Some(manager);
    rx
}

pub fn get_replication_manager(
) -> Option<std::sync::RwLockReadGuard<'static, Option<ReplicationManager>>> {
    let guard = REPLICATION_MANAGER.read().unwrap();
    if guard.is_some() {
        Some(guard)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_datacenters_start_unhealthy() {
        let config = ReplicationConfig {
            remote_dcs: vec![RemoteDatacenter {
                id: "dc2".to_string(),
                name: "Second".to_string(),
                endpoints: vec!["http://dc2:5000".to_string()],
                priority: 0,
                region: String::new(),
                read_only: false,
            }],
            ..Default::default()
        };
        let (manager, _events) = ReplicationManager::new(config);

        let status = manager.get_dc_status("dc2").unwrap();
        assert!(!status.healthy);
        assert_eq!(status.last_error.as_deref(), Some(NEVER_CONTACTED));
        assert!(!manager.is_healthy());

        let (alone, _events) = ReplicationManager::new(ReplicationConfig::default());
        assert!(alone.is_healthy());
    }

    #[test]
    fn test_vector_clock_increment() {
        let mut vc = VectorClock::new();
        vc.increment(&"dc1".to_string());
        vc.increment(&"dc1".to_string());
        vc.increment(&"dc2".to_string());

        assert_eq!(vc.get(&"dc1".to_string()), 2);
        assert_eq!(vc.get(&"dc2".to_string()), 1);
        assert_eq!(vc.get(&"dc3".to_string()), 0);
    }

    #[test]
    fn test_vector_clock_merge() {
        let mut vc1 = VectorClock::new();
        vc1.clocks.insert("dc1".to_string(), 3);
        vc1.clocks.insert("dc2".to_string(), 1);

        let mut vc2 = VectorClock::new();
        vc2.clocks.insert("dc1".to_string(), 2);
        vc2.clocks.insert("dc2".to_string(), 4);
        vc2.clocks.insert("dc3".to_string(), 1);

        vc1.merge(&vc2);

        assert_eq!(vc1.get(&"dc1".to_string()), 3);
        assert_eq!(vc1.get(&"dc2".to_string()), 4);
        assert_eq!(vc1.get(&"dc3".to_string()), 1);
    }

    #[test]
    fn test_vector_clock_dominates() {
        let mut vc1 = VectorClock::new();
        vc1.clocks.insert("dc1".to_string(), 3);
        vc1.clocks.insert("dc2".to_string(), 2);

        let mut vc2 = VectorClock::new();
        vc2.clocks.insert("dc1".to_string(), 2);
        vc2.clocks.insert("dc2".to_string(), 1);

        assert!(vc1.dominates(&vc2));
        assert!(!vc2.dominates(&vc1));
    }

    #[test]
    fn test_vector_clock_concurrent() {
        let mut vc1 = VectorClock::new();
        vc1.clocks.insert("dc1".to_string(), 3);
        vc1.clocks.insert("dc2".to_string(), 1);

        let mut vc2 = VectorClock::new();
        vc2.clocks.insert("dc1".to_string(), 2);
        vc2.clocks.insert("dc2".to_string(), 4);

        assert!(vc1.is_concurrent(&vc2));
        assert!(vc2.is_concurrent(&vc1));
    }
}
