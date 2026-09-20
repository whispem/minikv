use crate::common::{blake3_hash, NodeState};
use crate::coordinator::metadata::{KeyMetadata, KeyState, MetadataStore, VolumeMetadata};
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::{RaftNode, RaftRole};
use crate::coordinator::state::Command;
use crate::coordinator::volume_client::{ClientError, VolumeClient};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const VOLUME_TIMEOUT_MS: u64 = 5_000;
const LEADER_WAIT: Duration = Duration::from_secs(3);
const VOLUME_WAIT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const READ_ATTEMPTS: usize = 3;

#[derive(Debug)]
pub enum ObjectError {
    NotLeader(Option<String>),
    NotFound,
    NoVolumes,
    Volume(String),
    Consensus(String),
    Metadata(String),
}

impl std::fmt::Display for ObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectError::NotLeader(Some(leader)) => {
                write!(
                    f,
                    "this coordinator is not the leader, the leader is {}",
                    leader
                )
            }
            ObjectError::NotLeader(None) => write!(f, "no leader is elected right now"),
            ObjectError::NotFound => write!(f, "not found"),
            ObjectError::NoVolumes => write!(f, "no live volume"),
            ObjectError::Volume(e) => write!(f, "volume error: {}", e),
            ObjectError::Consensus(e) => write!(f, "consensus error: {}", e),
            ObjectError::Metadata(e) => write!(f, "metadata error: {}", e),
        }
    }
}

impl std::error::Error for ObjectError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeHeartbeat {
    pub volume_id: String,
    pub grpc_address: String,
    #[serde(default)]
    pub http_address: String,
    #[serde(default)]
    pub total_keys: u64,
    #[serde(default)]
    pub total_bytes: u64,
    #[serde(default)]
    pub free_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct StoredObject {
    pub replicas: Vec<String>,
    pub size: u64,
    pub blake3: String,
}

#[derive(Clone)]
pub struct ObjectStore {
    metadata: Arc<MetadataStore>,
    placement: Arc<Mutex<PlacementManager>>,
    raft: Arc<RaftNode>,
    clients: Arc<Mutex<HashMap<String, VolumeClient>>>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn blob_key(meta: &KeyMetadata) -> &str {
    if meta.blob_id.is_empty() {
        &meta.key
    } else {
        &meta.blob_id
    }
}

fn is_live(volume: &VolumeMetadata, now: u64) -> bool {
    volume.state.is_healthy() && now.saturating_sub(volume.last_heartbeat) <= VOLUME_TIMEOUT_MS
}

fn consensus_error(error: crate::Error) -> ObjectError {
    match error {
        crate::Error::NotLeader(leader) if leader != "unknown" => {
            ObjectError::NotLeader(Some(leader))
        }
        crate::Error::NotLeader(_) => ObjectError::NotLeader(None),
        other => ObjectError::Consensus(other.to_string()),
    }
}

fn describe_failures(
    targets: &[VolumeMetadata],
    results: &[Result<(), ClientError>],
) -> Option<String> {
    let failures: Vec<String> = targets
        .iter()
        .zip(results)
        .filter_map(|(volume, result)| {
            result
                .as_ref()
                .err()
                .map(|e| format!("{}: {}", volume.volume_id, e))
        })
        .collect();
    if failures.is_empty() {
        None
    } else {
        Some(failures.join("; "))
    }
}

impl ObjectStore {
    pub fn new(
        metadata: Arc<MetadataStore>,
        placement: Arc<Mutex<PlacementManager>>,
        raft: Arc<RaftNode>,
    ) -> Self {
        Self {
            metadata,
            placement,
            raft,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn metadata(&self) -> &Arc<MetadataStore> {
        &self.metadata
    }

    pub fn raft(&self) -> &Arc<RaftNode> {
        &self.raft
    }

    pub fn record_heartbeat(&self, heartbeat: VolumeHeartbeat) -> crate::Result<()> {
        let now = now_ms();
        let previous = self.metadata.get_volume(&heartbeat.volume_id)?;
        match &previous {
            None => tracing::info!(
                "Volume {} joined at {}",
                heartbeat.volume_id,
                heartbeat.grpc_address
            ),
            Some(old) if old.grpc_address != heartbeat.grpc_address => tracing::info!(
                "Volume {} moved from {} to {}",
                heartbeat.volume_id,
                old.grpc_address,
                heartbeat.grpc_address
            ),
            Some(old) if !is_live(old, now) => {
                tracing::info!("Volume {} is back online", heartbeat.volume_id)
            }
            Some(_) => {}
        }
        self.metadata.put_volume(&VolumeMetadata {
            volume_id: heartbeat.volume_id,
            address: heartbeat.http_address,
            grpc_address: heartbeat.grpc_address,
            state: NodeState::Alive,
            shards: previous.map(|v| v.shards).unwrap_or_default(),
            total_keys: heartbeat.total_keys,
            total_bytes: heartbeat.total_bytes,
            free_bytes: heartbeat.free_bytes,
            last_heartbeat: now,
        })
    }

    pub fn live_volumes(&self) -> Vec<VolumeMetadata> {
        let now = now_ms();
        self.metadata
            .list_volumes()
            .unwrap_or_default()
            .into_iter()
            .filter(|v| is_live(v, now))
            .collect()
    }

    fn client(&self, addr: &str) -> Result<VolumeClient, ClientError> {
        let mut clients = self.clients.lock().unwrap();
        if let Some(client) = clients.get(addr) {
            return Ok(client.clone());
        }
        let client = VolumeClient::lazy(addr)?;
        clients.insert(addr.to_string(), client.clone());
        Ok(client)
    }

    async fn ensure_leader(&self) -> Result<(), ObjectError> {
        let deadline = Instant::now() + LEADER_WAIT;
        loop {
            if self.raft.get_role() == RaftRole::Leader {
                return Ok(());
            }
            let leader = self.raft.get_leader();
            if let Some(leader) = &leader {
                if leader != self.raft.node_id() {
                    return Err(ObjectError::NotLeader(Some(leader.clone())));
                }
            }
            if Instant::now() >= deadline {
                return Err(ObjectError::NotLeader(leader));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn wait_for_volumes(&self) -> Vec<VolumeMetadata> {
        let deadline = Instant::now() + VOLUME_WAIT;
        loop {
            let live = self.live_volumes();
            if !live.is_empty() || Instant::now() >= deadline {
                return live;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    pub async fn put(&self, key: &str, data: Vec<u8>) -> Result<StoredObject, ObjectError> {
        self.ensure_leader().await?;
        let live = self.wait_for_volumes().await;
        if live.is_empty() {
            return Err(ObjectError::NoVolumes);
        }
        let (targets, wanted): (Vec<VolumeMetadata>, usize) = {
            let placement = self.placement.lock().unwrap();
            let targets = placement
                .select_available(key, &live)
                .into_iter()
                .filter_map(|id| live.iter().find(|v| v.volume_id == id).cloned())
                .collect();
            (targets, placement.replicas())
        };
        if targets.len() < wanted {
            tracing::warn!(
                "Writing {} on {} replica(s) instead of {}: not enough live volumes",
                key,
                targets.len(),
                wanted
            );
        }

        let upload_id = uuid::Uuid::new_v4().simple().to_string();
        let blob_id = format!("{}#{}", key, upload_id);
        let checksum = blake3_hash(&data);
        let size = data.len() as u64;

        let prepared = join_all(targets.iter().map(|volume| {
            let client = self.client(&volume.grpc_address);
            let blob_id = blob_id.clone();
            let upload_id = upload_id.clone();
            let data = data.clone();
            let checksum = checksum.clone();
            async move {
                let mut client = client?;
                let response = client.prepare(blob_id, upload_id, data, checksum).await?;
                if response.ok {
                    Ok(())
                } else {
                    Err(ClientError::from(response.error))
                }
            }
        }))
        .await;
        if let Some(failures) = describe_failures(&targets, &prepared) {
            self.abort(&targets, &upload_id).await;
            return Err(ObjectError::Volume(format!(
                "prepare failed ({})",
                failures
            )));
        }

        let committed = join_all(targets.iter().map(|volume| {
            let client = self.client(&volume.grpc_address);
            let blob_id = blob_id.clone();
            let upload_id = upload_id.clone();
            async move {
                let mut client = client?;
                let response = client.commit(upload_id, blob_id).await?;
                if response.ok {
                    Ok(())
                } else {
                    Err(ClientError::from(response.error))
                }
            }
        }))
        .await;
        let replicas: Vec<String> = targets.iter().map(|v| v.volume_id.clone()).collect();
        if let Some(failures) = describe_failures(&targets, &committed) {
            self.abort(&targets, &upload_id).await;
            self.delete_blob_from(&replicas, &blob_id).await;
            return Err(ObjectError::Volume(format!("commit failed ({})", failures)));
        }

        let previous = self.metadata.get_key(key).ok().flatten();
        let now = now_ms();
        let meta = KeyMetadata {
            key: key.to_string(),
            blob_id: blob_id.clone(),
            replicas: replicas.clone(),
            size,
            blake3: checksum.clone(),
            created_at: previous.as_ref().map(|p| p.created_at).unwrap_or(now),
            updated_at: now,
            state: KeyState::Active,
        };
        let command = Command::PutKey(meta)
            .encode()
            .map_err(|e| ObjectError::Metadata(e.to_string()))?;
        self.raft.propose(command).await.map_err(consensus_error)?;

        if let Some(previous) = previous {
            if blob_key(&previous) != blob_id {
                let store = self.clone();
                tokio::spawn(async move {
                    store
                        .delete_blob_from(&previous.replicas, blob_key(&previous))
                        .await;
                });
            }
        }

        Ok(StoredObject {
            replicas,
            size,
            blake3: checksum,
        })
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>, ObjectError> {
        self.raft.read_barrier().await.map_err(consensus_error)?;
        let mut last_error = ObjectError::NotFound;
        for _ in 0..READ_ATTEMPTS {
            let meta = match self.metadata.get_key(key) {
                Ok(Some(meta)) if meta.state == KeyState::Active => meta,
                Ok(_) => return Err(ObjectError::NotFound),
                Err(e) => return Err(ObjectError::Metadata(e.to_string())),
            };
            match self.read_replicas(&meta).await {
                Ok(data) => return Ok(data),
                Err(e) => last_error = e,
            }
            let unchanged = matches!(
                self.metadata.get_key(key),
                Ok(Some(ref current)) if blob_key(current) == blob_key(&meta)
            );
            if unchanged {
                break;
            }
        }
        Err(last_error)
    }

    async fn read_replicas(&self, meta: &KeyMetadata) -> Result<Vec<u8>, ObjectError> {
        if meta.replicas.is_empty() {
            return Err(ObjectError::Volume(
                "no replica is recorded for this key".to_string(),
            ));
        }
        let now = now_ms();
        let registry = self.metadata.list_volumes().unwrap_or_default();
        let live: HashSet<&str> = registry
            .iter()
            .filter(|v| is_live(v, now))
            .map(|v| v.volume_id.as_str())
            .collect();
        let mut replicas = meta.replicas.clone();
        replicas.sort_by_key(|id| !live.contains(id.as_str()));

        let mut failures = Vec::new();
        for volume_id in replicas {
            let Some(volume) = registry.iter().find(|v| v.volume_id == volume_id) else {
                failures.push(format!("{}: unknown volume", volume_id));
                continue;
            };
            let result = match self.client(&volume.grpc_address) {
                Ok(mut client) => client.pull(blob_key(meta).to_string()).await,
                Err(e) => Err(e),
            };
            match result {
                Ok(Some(data)) if blake3_hash(&data) == meta.blake3 => return Ok(data),
                Ok(Some(_)) => failures.push(format!("{}: checksum mismatch", volume_id)),
                Ok(None) => failures.push(format!("{}: blob missing", volume_id)),
                Err(e) => failures.push(format!("{}: {}", volume_id, e)),
            }
        }
        Err(ObjectError::Volume(failures.join("; ")))
    }

    pub async fn delete(&self, key: &str) -> Result<(), ObjectError> {
        self.ensure_leader().await?;
        let meta = match self.metadata.get_key(key) {
            Ok(Some(meta)) => meta,
            Ok(None) => return Err(ObjectError::NotFound),
            Err(e) => return Err(ObjectError::Metadata(e.to_string())),
        };
        let command = Command::DeleteKey(key.to_string())
            .encode()
            .map_err(|e| ObjectError::Metadata(e.to_string()))?;
        self.raft.propose(command).await.map_err(consensus_error)?;
        self.delete_blob_from(&meta.replicas, blob_key(&meta)).await;
        Ok(())
    }

    async fn abort(&self, targets: &[VolumeMetadata], upload_id: &str) {
        join_all(targets.iter().map(|volume| {
            let client = self.client(&volume.grpc_address);
            let upload_id = upload_id.to_string();
            async move {
                if let Ok(mut client) = client {
                    let _ = client.abort(upload_id).await;
                }
            }
        }))
        .await;
    }

    async fn delete_blob_from(&self, volume_ids: &[String], blob: &str) {
        let registry = self.metadata.list_volumes().unwrap_or_default();
        let deletions = volume_ids
            .iter()
            .filter_map(|id| registry.iter().find(|v| &v.volume_id == id))
            .map(|volume| {
                let client = self.client(&volume.grpc_address);
                let blob = blob.to_string();
                let volume_id = volume.volume_id.clone();
                async move {
                    let outcome = match client {
                        Ok(mut client) => client.delete(blob).await,
                        Err(e) => Err(e),
                    };
                    match outcome {
                        Ok(response) if response.ok => {}
                        Ok(response) => {
                            tracing::warn!(
                                "Cannot delete blob on {}: {}",
                                volume_id,
                                response.error
                            )
                        }
                        Err(e) => tracing::warn!("Cannot delete blob on {}: {}", volume_id, e),
                    }
                }
            });
        join_all(deletions).await;
    }
}
