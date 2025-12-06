//! Metadata store using RocksDB
//!
//! Stores:
//! - Key metadata (replicas, size, blake3, timestamps)
//! - Volume registry (node_id → address, state, shards)
//! - Cluster configuration

use crate::common::{NodeState, Result};
use rocksdb::{Options, DB};
use serde::{Deserialize, Serialize};
use std::path::Path;

const CF_KEYS: &str = "keys";
const CF_VOLUMES: &str = "volumes";
const CF_CONFIG: &str = "config";

/// Key metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub key: String,
    pub replicas: Vec<String>,
    pub size: u64,
    pub blake3: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub state: KeyState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyState {
    Active,
    Tombstone,
}

/// Volume metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMetadata {
    pub volume_id: String,
    pub address: String,
    pub grpc_address: String,
    pub state: NodeState,
    pub shards: Vec<u64>,
    pub total_keys: u64,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub last_heartbeat: u64,
}

/// Metadata store
pub struct MetadataStore {
    db: DB,
}

impl MetadataStore {
    /// Open or create metadata store
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open_cf(&opts, path, vec![CF_KEYS, CF_VOLUMES, CF_CONFIG])?;

        Ok(Self { db })
    }

    // === Key operations ===

    /// Put key metadata
    pub fn put_key(&self, meta: &KeyMetadata) -> Result<()> {
        let cf = self.db.cf_handle(CF_KEYS).unwrap();
        let value = bincode::serialize(meta)
            .map_err(|e| crate::Error::Internal(format!("Serialize error: {}", e)))?;
        self.db.put_cf(cf, meta.key.as_bytes(), value)?;
        Ok(())
    }

    /// Get key metadata
    pub fn get_key(&self, key: &str) -> Result<Option<KeyMetadata>> {
        let cf = self.db.cf_handle(CF_KEYS).unwrap();
        match self.db.get_cf(cf, key.as_bytes())? {
            Some(bytes) => {
                let meta: KeyMetadata = bincode::deserialize(&bytes)
                    .map_err(|e| crate::Error::MetadataCorrupted(e.to_string()))?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// Delete key metadata
    pub fn delete_key(&self, key: &str) -> Result<()> {
        let cf = self.db.cf_handle(CF_KEYS).unwrap();
        self.db.delete_cf(cf, key.as_bytes())?;
        Ok(())
    }

    /// List all keys (for ops commands)
    pub fn list_keys(&self) -> Result<Vec<String>> {
        let cf = self.db.cf_handle(CF_KEYS).unwrap();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut keys = Vec::new();
        for item in iter {
            let (key_bytes, _) = item?;
            let key = String::from_utf8(key_bytes.to_vec())
                .map_err(|_| crate::Error::MetadataCorrupted("Invalid UTF-8".into()))?;
            keys.push(key);
        }

        Ok(keys)
    }

    // === Volume operations ===

    /// Register or update volume
    pub fn put_volume(&self, meta: &VolumeMetadata) -> Result<()> {
        let cf = self.db.cf_handle(CF_VOLUMES).unwrap();
        let value = bincode::serialize(meta)
            .map_err(|e| crate::Error::Internal(format!("Serialize error: {}", e)))?;
        self.db.put_cf(cf, meta.volume_id.as_bytes(), value)?;
        Ok(())
    }

    /// Get volume metadata
    pub fn get_volume(&self, volume_id: &str) -> Result<Option<VolumeMetadata>> {
        let cf = self.db.cf_handle(CF_VOLUMES).unwrap();
        match self.db.get_cf(cf, volume_id.as_bytes())? {
            Some(bytes) => {
                let meta: VolumeMetadata = bincode::deserialize(&bytes)
                    .map_err(|e| crate::Error::MetadataCorrupted(e.to_string()))?;
                Ok(Some(meta))
            }
            None => Ok(None),
        }
    }

    /// List all volumes
    pub fn list_volumes(&self) -> Result<Vec<VolumeMetadata>> {
        let cf = self.db.cf_handle(CF_VOLUMES).unwrap();
        let iter = self.db.iterator_cf(cf, rocksdb::IteratorMode::Start);

        let mut volumes = Vec::new();
        for item in iter {
            let (_, value_bytes) = item?;
            let meta: VolumeMetadata = bincode::deserialize(&value_bytes)
                .map_err(|e| crate::Error::MetadataCorrupted(e.to_string()))?;
            volumes.push(meta);
        }

        Ok(volumes)
    }

    /// Get healthy volumes
    pub fn get_healthy_volumes(&self) -> Result<Vec<VolumeMetadata>> {
        Ok(self
            .list_volumes()?
            .into_iter()
            .filter(|v| v.state.is_healthy())
            .collect())
    }

    // === Config operations ===

    /// Put config value
    pub fn put_config(&self, key: &str, value: &[u8]) -> Result<()> {
        let cf = self.db.cf_handle(CF_CONFIG).unwrap();
        self.db.put_cf(cf, key.as_bytes(), value)?;
        Ok(())
    }

    /// Get config value
    pub fn get_config(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let cf = self.db.cf_handle(CF_CONFIG).unwrap();
        Ok(self.db.get_cf(cf, key.as_bytes())?)
    }

    /// Flush to disk
    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_metadata_store() {
        let dir = tempdir().unwrap();
        let store = MetadataStore::open(dir.path().join("test.db")).unwrap();

        // Put key
        let meta = KeyMetadata {
            key: "test-key".to_string(),
            replicas: vec!["vol-1".to_string(), "vol-2".to_string()],
            size: 1024,
            blake3: "abc123".to_string(),
            created_at: 1234567890,
            updated_at: 1234567890,
            state: KeyState::Active,
        };
        store.put_key(&meta).unwrap();

        // Get key
        let retrieved = store.get_key("test-key").unwrap().unwrap();
        assert_eq!(retrieved.key, "test-key");
        assert_eq!(retrieved.replicas.len(), 2);

        // Delete key
        store.delete_key("test-key").unwrap();
        assert!(store.get_key("test-key").unwrap().is_none());
    }

    #[test]
    fn test_volume_registry() {
        let dir = tempdir().unwrap();
        let store = MetadataStore::open(dir.path().join("test.db")).unwrap();

        let vol = VolumeMetadata {
            volume_id: "vol-1".to_string(),
            address: "http://localhost:6000".to_string(),
            grpc_address: "http://localhost:6001".to_string(),
            state: NodeState::Alive,
            shards: vec![0, 1, 2],
            total_keys: 100,
            total_bytes: 1024000,
            free_bytes: 5000000,
            last_heartbeat: 1234567890,
        };

        store.put_volume(&vol).unwrap();

        let retrieved = store.get_volume("vol-1").unwrap().unwrap();
        assert_eq!(retrieved.volume_id, "vol-1");
        assert_eq!(retrieved.shards.len(), 3);

        let volumes = store.list_volumes().unwrap();
        assert_eq!(volumes.len(), 1);
    }
}
