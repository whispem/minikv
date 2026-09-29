//! Bookkeeping for backups: one directory and one manifest per backup.
//!
//! It copies no data: [`BackupManager::start_backup`] creates the directories
//! and a manifest marked in progress, and the caller is expected to copy the
//! data, then call [`BackupManager::complete_backup`]. Restoring is not
//! implemented: [`BackupManager::start_restore`] fails. minikv never uses this
//! module: [`init_backup`] has no caller, and the backup and restore routes of
//! the coordinator answer `501 Not Implemented`.

use crate::common::{Error, Result};
use crate::ops::NotImplemented;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupType {
    Full,
    Incremental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub id: String,

    pub backup_type: BackupType,

    pub status: BackupStatus,

    pub started_at: DateTime<Utc>,

    pub completed_at: Option<DateTime<Utc>>,

    pub size_bytes: u64,

    pub key_count: u64,

    pub checksum: String,

    pub parent_id: Option<String>,

    pub wal_sequence: u64,

    pub data_files: Vec<BackupFile>,

    pub config: BackupConfig,

    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    pub path: String,

    pub size: u64,

    pub checksum: String,

    pub encrypted: bool,

    pub compressed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub destination: BackupDestination,

    #[serde(default = "default_true")]
    pub compress: bool,

    #[serde(default)]
    pub encrypt: bool,

    #[serde(skip_serializing)]
    pub encryption_key: Option<String>,

    #[serde(default = "default_true")]
    pub include_wal: bool,

    #[serde(default = "default_workers")]
    pub parallel_workers: usize,

    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
}

fn default_true() -> bool {
    true
}

fn default_workers() -> usize {
    4
}

fn default_chunk_size() -> usize {
    64 * 1024 * 1024 // 64 MB
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            destination: BackupDestination::Local {
                path: "./backups".to_string(),
            },
            compress: true,
            encrypt: false,
            encryption_key: None,
            include_wal: true,
            parallel_workers: 4,
            chunk_size: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackupDestination {
    Local {
        path: String,
    },

    S3 {
        bucket: String,
        prefix: Option<String>,
        endpoint: Option<String>,
        region: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreConfig {
    pub backup_id: String,

    pub source: BackupDestination,

    pub target_path: String,

    #[serde(skip_serializing)]
    pub decryption_key: Option<String>,

    pub point_in_time: Option<DateTime<Utc>>,

    #[serde(default = "default_workers")]
    pub parallel_workers: usize,

    #[serde(default = "default_true")]
    pub verify_checksums: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupProgress {
    pub id: String,

    pub phase: String,

    pub total_bytes: u64,

    pub processed_bytes: u64,

    pub percent_complete: f64,

    pub eta_seconds: Option<u64>,

    pub rate_bytes_per_sec: u64,

    pub errors: Vec<String>,
}

pub struct BackupManager {
    active_backups: Arc<RwLock<HashMap<String, BackupProgress>>>,

    manifests: Arc<RwLock<Vec<BackupManifest>>>,

    backup_path: PathBuf,
}

impl BackupManager {
    pub fn new(backup_path: impl AsRef<Path>) -> Self {
        Self {
            active_backups: Arc::new(RwLock::new(HashMap::new())),
            manifests: Arc::new(RwLock::new(Vec::new())),
            backup_path: backup_path.as_ref().to_path_buf(),
        }
    }

    pub async fn start_backup(
        &self,
        config: BackupConfig,
        backup_type: BackupType,
    ) -> Result<String> {
        let backup_id = format!(
            "backup-{}-{}",
            backup_type.as_str(),
            Utc::now().format("%Y%m%d-%H%M%S")
        );

        let progress = BackupProgress {
            id: backup_id.clone(),
            phase: "initializing".to_string(),
            total_bytes: 0,
            processed_bytes: 0,
            percent_complete: 0.0,
            eta_seconds: None,
            rate_bytes_per_sec: 0,
            errors: vec![],
        };

        self.active_backups
            .write()
            .await
            .insert(backup_id.clone(), progress);

        let backup_dir = self.backup_path.join(&backup_id);
        tokio::fs::create_dir_all(&backup_dir)
            .await
            .map_err(|e| Error::Other(format!("Failed to create backup directory: {}", e)))?;

        tokio::fs::create_dir_all(backup_dir.join("data"))
            .await
            .map_err(|e| Error::Other(format!("Failed to create data directory: {}", e)))?;

        tokio::fs::create_dir_all(backup_dir.join("metadata"))
            .await
            .map_err(|e| Error::Other(format!("Failed to create metadata directory: {}", e)))?;

        if config.include_wal {
            tokio::fs::create_dir_all(backup_dir.join("wal"))
                .await
                .map_err(|e| Error::Other(format!("Failed to create wal directory: {}", e)))?;
        }

        let manifest = BackupManifest {
            id: backup_id.clone(),
            backup_type,
            status: BackupStatus::InProgress,
            started_at: Utc::now(),
            completed_at: None,
            size_bytes: 0,
            key_count: 0,
            checksum: String::new(),
            parent_id: None,
            wal_sequence: 0,
            data_files: vec![],
            config,
            metadata: HashMap::new(),
        };

        let manifest_path = backup_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| Error::Other(format!("Failed to serialize manifest: {}", e)))?;
        tokio::fs::write(&manifest_path, manifest_json)
            .await
            .map_err(|e| Error::Other(format!("Failed to write manifest: {}", e)))?;

        self.manifests.write().await.push(manifest);

        Ok(backup_id)
    }

    pub async fn update_progress(&self, backup_id: &str, update: impl FnOnce(&mut BackupProgress)) {
        if let Some(progress) = self.active_backups.write().await.get_mut(backup_id) {
            update(progress);
        }
    }

    pub async fn complete_backup(
        &self,
        backup_id: &str,
        size_bytes: u64,
        key_count: u64,
        checksum: String,
        data_files: Vec<BackupFile>,
    ) -> Result<()> {
        let mut manifests = self.manifests.write().await;
        if let Some(manifest) = manifests.iter_mut().find(|m| m.id == backup_id) {
            manifest.status = BackupStatus::Completed;
            manifest.completed_at = Some(Utc::now());
            manifest.size_bytes = size_bytes;
            manifest.key_count = key_count;
            manifest.checksum = checksum;
            manifest.data_files = data_files;

            let manifest_path = self.backup_path.join(backup_id).join("manifest.json");
            let manifest_json = serde_json::to_string_pretty(manifest)
                .map_err(|e| Error::Other(format!("Failed to serialize manifest: {}", e)))?;
            tokio::fs::write(&manifest_path, manifest_json)
                .await
                .map_err(|e| Error::Other(format!("Failed to write manifest: {}", e)))?;
        }

        self.active_backups.write().await.remove(backup_id);

        Ok(())
    }

    pub async fn fail_backup(&self, backup_id: &str, error: &str) -> Result<()> {
        let mut manifests = self.manifests.write().await;
        if let Some(manifest) = manifests.iter_mut().find(|m| m.id == backup_id) {
            manifest.status = BackupStatus::Failed;
            manifest.completed_at = Some(Utc::now());
            manifest
                .metadata
                .insert("error".to_string(), error.to_string());
        }

        self.active_backups.write().await.remove(backup_id);

        Ok(())
    }

    pub async fn get_progress(&self, backup_id: &str) -> Option<BackupProgress> {
        self.active_backups.read().await.get(backup_id).cloned()
    }

    pub async fn list_backups(&self) -> Vec<BackupManifest> {
        self.manifests.read().await.clone()
    }

    pub async fn get_backup(&self, backup_id: &str) -> Option<BackupManifest> {
        self.manifests
            .read()
            .await
            .iter()
            .find(|m| m.id == backup_id)
            .cloned()
    }

    pub async fn delete_backup(&self, backup_id: &str) -> Result<()> {
        let backup_dir = self.backup_path.join(backup_id);
        if backup_dir.exists() {
            tokio::fs::remove_dir_all(&backup_dir)
                .await
                .map_err(|e| Error::Other(format!("Failed to delete backup: {}", e)))?;
        }

        self.manifests.write().await.retain(|m| m.id != backup_id);

        Ok(())
    }

    /// Fails: restoring is not implemented.
    pub async fn start_restore(&self, _config: RestoreConfig) -> Result<String> {
        Err(NotImplemented::RESTORE.into())
    }

    pub async fn load_manifests(&self) -> Result<()> {
        if !self.backup_path.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&self.backup_path)
            .await
            .map_err(|e| Error::Other(format!("Failed to read backup directory: {}", e)))?;

        let mut manifests = vec![];

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::Other(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("manifest.json");
                if manifest_path.exists() {
                    match tokio::fs::read_to_string(&manifest_path).await {
                        Ok(json) => match serde_json::from_str::<BackupManifest>(&json) {
                            Ok(manifest) => manifests.push(manifest),
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse manifest {:?}: {}",
                                    manifest_path,
                                    e
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!("Failed to read manifest {:?}: {}", manifest_path, e);
                        }
                    }
                }
            }
        }

        *self.manifests.write().await = manifests;

        Ok(())
    }
}

impl BackupType {
    fn as_str(self) -> &'static str {
        match self {
            BackupType::Full => "full",
            BackupType::Incremental => "incremental",
        }
    }
}

pub static BACKUP_MANAGER: once_cell::sync::Lazy<RwLock<Option<BackupManager>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

pub async fn init_backup(backup_path: impl AsRef<Path>) -> Result<()> {
    let manager = BackupManager::new(backup_path);
    manager.load_manifests().await?;
    *BACKUP_MANAGER.write().await = Some(manager);
    Ok(())
}

pub async fn get_backup_manager(
) -> Option<tokio::sync::RwLockReadGuard<'static, Option<BackupManager>>> {
    let guard = BACKUP_MANAGER.read().await;
    if guard.is_some() {
        Some(guard)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_backup_manager_create() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BackupManager::new(temp_dir.path());

        let config = BackupConfig::default();
        let backup_id = manager
            .start_backup(config, BackupType::Full)
            .await
            .unwrap();

        assert!(backup_id.starts_with("backup-full-"));

        let progress = manager.get_progress(&backup_id).await.unwrap();
        assert_eq!(progress.phase, "initializing");
    }

    #[tokio::test]
    async fn test_backup_complete() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BackupManager::new(temp_dir.path());

        let config = BackupConfig::default();
        let backup_id = manager
            .start_backup(config, BackupType::Full)
            .await
            .unwrap();

        manager
            .complete_backup(&backup_id, 1000, 100, "abc123".to_string(), vec![])
            .await
            .unwrap();

        let manifest = manager.get_backup(&backup_id).await.unwrap();
        assert_eq!(manifest.status, BackupStatus::Completed);
        assert_eq!(manifest.size_bytes, 1000);
        assert_eq!(manifest.key_count, 100);
    }

    #[tokio::test]
    async fn restoring_fails_instead_of_returning_an_id() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BackupManager::new(temp_dir.path());
        let backup_id = manager
            .start_backup(BackupConfig::default(), BackupType::Full)
            .await
            .unwrap();
        manager
            .complete_backup(&backup_id, 0, 0, String::new(), vec![])
            .await
            .unwrap();

        let error = manager
            .start_restore(RestoreConfig {
                backup_id,
                source: BackupDestination::Local {
                    path: temp_dir.path().display().to_string(),
                },
                target_path: temp_dir.path().join("restored").display().to_string(),
                decryption_key: None,
                point_in_time: None,
                parallel_workers: 1,
                verify_checksums: true,
            })
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "restore is not implemented (roadmap: unscheduled)"
        );
    }

    #[tokio::test]
    async fn test_list_backups() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BackupManager::new(temp_dir.path());

        let config = BackupConfig::default();
        manager
            .start_backup(config.clone(), BackupType::Full)
            .await
            .unwrap();
        manager
            .start_backup(config, BackupType::Incremental)
            .await
            .unwrap();

        let backups = manager.list_backups().await;
        assert_eq!(backups.len(), 2);
    }
}
