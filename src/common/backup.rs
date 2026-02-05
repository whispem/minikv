//! Backup and restore with full/incremental snapshots.

use crate::common::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::RwLock;

/// Backup type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupType {
    /// Full backup - complete snapshot
    Full,
    /// Incremental backup - only changes since last backup
    Incremental,
}

/// Backup status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    /// Backup is in progress
    InProgress,
    /// Backup completed successfully
    Completed,
    /// Backup failed
    Failed,
    /// Backup was cancelled
    Cancelled,
}

/// Backup manifest - metadata about a backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Unique backup ID
    pub id: String,

    /// Backup type
    pub backup_type: BackupType,

    /// Status
    pub status: BackupStatus,

    /// Start time
    pub started_at: DateTime<Utc>,

    /// End time
    pub completed_at: Option<DateTime<Utc>>,

    /// Total size in bytes
    pub size_bytes: u64,

    /// Number of keys backed up
    pub key_count: u64,

    /// Checksum of the backup
    pub checksum: String,

    /// Parent backup ID (for incremental backups)
    pub parent_id: Option<String>,

    /// WAL sequence number at backup time
    pub wal_sequence: u64,

    /// Data files included
    pub data_files: Vec<BackupFile>,

    /// Backup configuration used
    pub config: BackupConfig,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Information about a single file in the backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    /// Relative path within backup
    pub path: String,

    /// File size in bytes
    pub size: u64,

    /// SHA256 checksum
    pub checksum: String,

    /// Whether file is encrypted
    pub encrypted: bool,

    /// Whether file is compressed
    pub compressed: bool,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Backup destination
    pub destination: BackupDestination,

    /// Enable compression
    #[serde(default = "default_true")]
    pub compress: bool,

    /// Enable encryption
    #[serde(default)]
    pub encrypt: bool,

    /// Encryption key (if encrypting)
    #[serde(skip_serializing)]
    pub encryption_key: Option<String>,

    /// Include WAL files
    #[serde(default = "default_true")]
    pub include_wal: bool,

    /// Parallel workers for backup
    #[serde(default = "default_workers")]
    pub parallel_workers: usize,

    /// Chunk size for data files (bytes)
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

/// Backup destination
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackupDestination {
    /// Local filesystem
    Local { path: String },

    /// S3-compatible storage
    S3 {
        bucket: String,
        prefix: Option<String>,
        endpoint: Option<String>,
        region: Option<String>,
    },
}

/// Restore configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreConfig {
    /// Backup ID to restore from
    pub backup_id: String,

    /// Source location
    pub source: BackupDestination,

    /// Target data directory
    pub target_path: String,

    /// Decryption key (if backup is encrypted)
    #[serde(skip_serializing)]
    pub decryption_key: Option<String>,

    /// Restore to specific point in time (if available)
    pub point_in_time: Option<DateTime<Utc>>,

    /// Parallel workers for restore
    #[serde(default = "default_workers")]
    pub parallel_workers: usize,

    /// Verify checksums during restore
    #[serde(default = "default_true")]
    pub verify_checksums: bool,
}

/// Progress information for backup/restore operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupProgress {
    /// Operation ID
    pub id: String,

    /// Current phase
    pub phase: String,

    /// Total bytes to process
    pub total_bytes: u64,

    /// Bytes processed so far
    pub processed_bytes: u64,

    /// Percentage complete
    pub percent_complete: f64,

    /// Estimated time remaining (seconds)
    pub eta_seconds: Option<u64>,

    /// Current rate (bytes/second)
    pub rate_bytes_per_sec: u64,

    /// Errors encountered
    pub errors: Vec<String>,
}

/// Backup manager - coordinates backup and restore operations
pub struct BackupManager {
    /// Active backups
    active_backups: Arc<RwLock<HashMap<String, BackupProgress>>>,

    /// Completed backup manifests
    manifests: Arc<RwLock<Vec<BackupManifest>>>,

    /// Base backup path
    backup_path: PathBuf,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(backup_path: impl AsRef<Path>) -> Self {
        Self {
            active_backups: Arc::new(RwLock::new(HashMap::new())),
            manifests: Arc::new(RwLock::new(Vec::new())),
            backup_path: backup_path.as_ref().to_path_buf(),
        }
    }

    /// Start a new backup
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

        // Create backup directory
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

        // Create initial manifest
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

        // Save manifest
        let manifest_path = backup_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| Error::Other(format!("Failed to serialize manifest: {}", e)))?;
        tokio::fs::write(&manifest_path, manifest_json)
            .await
            .map_err(|e| Error::Other(format!("Failed to write manifest: {}", e)))?;

        self.manifests.write().await.push(manifest);

        Ok(backup_id)
    }

    /// Update backup progress
    pub async fn update_progress(&self, backup_id: &str, update: impl FnOnce(&mut BackupProgress)) {
        if let Some(progress) = self.active_backups.write().await.get_mut(backup_id) {
            update(progress);
        }
    }

    /// Complete a backup
    pub async fn complete_backup(
        &self,
        backup_id: &str,
        size_bytes: u64,
        key_count: u64,
        checksum: String,
        data_files: Vec<BackupFile>,
    ) -> Result<()> {
        // Update manifest
        let mut manifests = self.manifests.write().await;
        if let Some(manifest) = manifests.iter_mut().find(|m| m.id == backup_id) {
            manifest.status = BackupStatus::Completed;
            manifest.completed_at = Some(Utc::now());
            manifest.size_bytes = size_bytes;
            manifest.key_count = key_count;
            manifest.checksum = checksum;
            manifest.data_files = data_files;

            // Save updated manifest
            let manifest_path = self.backup_path.join(backup_id).join("manifest.json");
            let manifest_json = serde_json::to_string_pretty(manifest)
                .map_err(|e| Error::Other(format!("Failed to serialize manifest: {}", e)))?;
            tokio::fs::write(&manifest_path, manifest_json)
                .await
                .map_err(|e| Error::Other(format!("Failed to write manifest: {}", e)))?;
        }

        // Remove from active backups
        self.active_backups.write().await.remove(backup_id);

        Ok(())
    }

    /// Fail a backup
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

    /// Get backup progress
    pub async fn get_progress(&self, backup_id: &str) -> Option<BackupProgress> {
        self.active_backups.read().await.get(backup_id).cloned()
    }

    /// List all backups
    pub async fn list_backups(&self) -> Vec<BackupManifest> {
        self.manifests.read().await.clone()
    }

    /// Get a specific backup manifest
    pub async fn get_backup(&self, backup_id: &str) -> Option<BackupManifest> {
        self.manifests
            .read()
            .await
            .iter()
            .find(|m| m.id == backup_id)
            .cloned()
    }

    /// Delete a backup
    pub async fn delete_backup(&self, backup_id: &str) -> Result<()> {
        // Remove from disk
        let backup_dir = self.backup_path.join(backup_id);
        if backup_dir.exists() {
            tokio::fs::remove_dir_all(&backup_dir)
                .await
                .map_err(|e| Error::Other(format!("Failed to delete backup: {}", e)))?;
        }

        // Remove from manifests
        self.manifests.write().await.retain(|m| m.id != backup_id);

        Ok(())
    }

    /// Start a restore operation
    pub async fn start_restore(&self, config: RestoreConfig) -> Result<String> {
        let restore_id = format!("restore-{}", Utc::now().format("%Y%m%d-%H%M%S"));

        let progress = BackupProgress {
            id: restore_id.clone(),
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
            .insert(restore_id.clone(), progress);

        // Verify backup exists
        let backup_dir = self.backup_path.join(&config.backup_id);
        if !backup_dir.exists() {
            return Err(Error::Other(format!(
                "Backup {} not found",
                config.backup_id
            )));
        }

        // Load manifest
        let manifest_path = backup_dir.join("manifest.json");
        let manifest_json = tokio::fs::read_to_string(&manifest_path)
            .await
            .map_err(|e| Error::Other(format!("Failed to read manifest: {}", e)))?;
        let manifest: BackupManifest = serde_json::from_str(&manifest_json)
            .map_err(|e| Error::Other(format!("Failed to parse manifest: {}", e)))?;

        if manifest.status != BackupStatus::Completed {
            return Err(Error::Other(format!(
                "Cannot restore from backup with status {:?}",
                manifest.status
            )));
        }

        // Verify checksum if required
        if config.verify_checksums {
            self.update_progress(&restore_id, |p| {
                p.phase = "verifying checksums".to_string();
            })
            .await;

            if manifest.data_files.is_empty() {
                return Err(Error::Other(
                    "Backup manifest has no data_files to verify".to_string(),
                ));
            }

            let total_bytes: u64 = manifest.data_files.iter().map(|f| f.size).sum();
            self.update_progress(&restore_id, |p| {
                p.total_bytes = total_bytes;
                p.processed_bytes = 0;
                p.percent_complete = 0.0;
                p.rate_bytes_per_sec = 0;
                p.eta_seconds = None;
            })
            .await;

            let mut verified_files = Vec::with_capacity(manifest.data_files.len());
            let mut processed_bytes = 0u64;

            for file in &manifest.data_files {
                let file_path = backup_dir.join(&file.path);
                if !file_path.exists() {
                    return Err(Error::NotFound(format!(
                        "Backup file missing: {}",
                        file.path
                    )));
                }

                let metadata = tokio::fs::metadata(&file_path)
                    .await
                    .map_err(|e| Error::Other(format!("Failed to read metadata: {}", e)))?;
                let actual_size = metadata.len();
                if actual_size != file.size {
                    return Err(Error::Corrupted(format!(
                        "File size mismatch for {}: expected {}, got {}",
                        file.path, file.size, actual_size
                    )));
                }

                let (actual_checksum, bytes_read) = compute_sha256_file(&file_path).await?;
                let expected_checksum = file.checksum.to_lowercase();
                let actual_checksum_lc = actual_checksum.to_lowercase();

                if expected_checksum != actual_checksum_lc {
                    return Err(Error::ChecksumMismatch {
                        expected: expected_checksum,
                        actual: actual_checksum_lc,
                    });
                }

                processed_bytes = processed_bytes.saturating_add(bytes_read);
                let percent = if total_bytes == 0 {
                    100.0
                } else {
                    (processed_bytes as f64 / total_bytes as f64) * 100.0
                };

                self.update_progress(&restore_id, |p| {
                    p.processed_bytes = processed_bytes;
                    p.percent_complete = percent.min(100.0);
                })
                .await;

                verified_files.push(BackupFile {
                    path: file.path.clone(),
                    size: file.size,
                    checksum: actual_checksum,
                    encrypted: file.encrypted,
                    compressed: file.compressed,
                });
            }

            if !manifest.checksum.trim().is_empty() {
                let computed_manifest_checksum = compute_manifest_checksum(&verified_files);
                let expected_manifest_checksum = manifest.checksum.to_lowercase();
                let actual_manifest_checksum = computed_manifest_checksum.to_lowercase();
                if expected_manifest_checksum != actual_manifest_checksum {
                    return Err(Error::ChecksumMismatch {
                        expected: expected_manifest_checksum,
                        actual: actual_manifest_checksum,
                    });
                }
            }
        }

        Ok(restore_id)
    }

    /// Load manifests from disk on startup
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

async fn compute_sha256_file(path: &Path) -> Result<(String, u64)> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| Error::Other(format!("Failed to open file {:?}: {}", path, e)))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    let mut total_read = 0u64;

    loop {
        let read = file
            .read(&mut buf)
            .await
            .map_err(|e| Error::Other(format!("Failed to read file {:?}: {}", path, e)))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
        total_read = total_read.saturating_add(read as u64);
    }

    let digest = hasher.finalize();
    Ok((hex::encode(digest), total_read))
}

fn compute_manifest_checksum(files: &[BackupFile]) -> String {
    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));

    let mut hasher = Sha256::new();
    for file in sorted {
        let path_bytes = file.path.as_bytes();
        hasher.update((path_bytes.len() as u64).to_le_bytes());
        hasher.update(path_bytes);
        hasher.update(file.size.to_le_bytes());
        hasher.update([file.encrypted as u8]);
        hasher.update([file.compressed as u8]);
        hasher.update(file.checksum.as_bytes());
    }
    hex::encode(hasher.finalize())
}

impl BackupType {
    fn as_str(self) -> &'static str {
        match self {
            BackupType::Full => "full",
            BackupType::Incremental => "incremental",
        }
    }
}

/// Global backup manager instance
pub static BACKUP_MANAGER: once_cell::sync::Lazy<RwLock<Option<BackupManager>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

/// Initialize the global backup manager
pub async fn init_backup(backup_path: impl AsRef<Path>) -> Result<()> {
    let manager = BackupManager::new(backup_path);
    manager.load_manifests().await?;
    *BACKUP_MANAGER.write().await = Some(manager);
    Ok(())
}

/// Get the global backup manager
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

    #[tokio::test]
    async fn test_restore_verifies_checksums() {
        let temp_dir = TempDir::new().unwrap();
        let manager = BackupManager::new(temp_dir.path());

        let backup_id = "backup-full-test".to_string();
        let backup_dir = temp_dir.path().join(&backup_id);
        tokio::fs::create_dir_all(backup_dir.join("data"))
            .await
            .unwrap();

        let file_path = backup_dir.join("data").join("chunk-0001.bin");
        tokio::fs::write(&file_path, b"minikv-test-data")
            .await
            .unwrap();

        let (checksum, size) = compute_sha256_file(&file_path).await.unwrap();
        let data_files = vec![BackupFile {
            path: "data/chunk-0001.bin".to_string(),
            size,
            checksum,
            encrypted: false,
            compressed: false,
        }];

        let manifest = BackupManifest {
            id: backup_id.clone(),
            backup_type: BackupType::Full,
            status: BackupStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            size_bytes: size,
            key_count: 1,
            checksum: compute_manifest_checksum(&data_files),
            parent_id: None,
            wal_sequence: 0,
            data_files: data_files.clone(),
            config: BackupConfig::default(),
            metadata: HashMap::new(),
        };

        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap();
        tokio::fs::write(backup_dir.join("manifest.json"), manifest_json)
            .await
            .unwrap();

        let restore_id = manager
            .start_restore(RestoreConfig {
                backup_id,
                source: BackupDestination::Local {
                    path: temp_dir.path().to_string_lossy().to_string(),
                },
                target_path: temp_dir
                    .path()
                    .join("restore-target")
                    .to_string_lossy()
                    .to_string(),
                decryption_key: None,
                point_in_time: None,
                parallel_workers: 1,
                verify_checksums: true,
            })
            .await
            .unwrap();

        assert!(restore_id.starts_with("restore-"));
    }
}
