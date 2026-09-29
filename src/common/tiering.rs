//! Bookkeeping for data tiering (hot, warm, cold and archive).
//!
//! [`TieringManager`] records which tier each tracked key belongs to, and
//! evaluates the policies that would move keys between tiers. Nothing in
//! minikv uses it, and no tier stores data: [`TieringManager::apply_change`]
//! only updates the records, and [`compress`] fails for every algorithm but
//! `None`. Of the configuration, only `policies` is read.

use crate::common::{Error, Result};
use crate::ops::NotImplemented;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub hot: TierConfig,

    #[serde(default)]
    pub warm: TierConfig,

    #[serde(default)]
    pub cold: TierConfig,

    pub archive: Option<ArchiveConfig>,

    #[serde(default)]
    pub policies: Vec<TieringPolicy>,

    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_interval() -> u64 {
    3600 // 1 hour
}

impl Default for TieringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hot: TierConfig::default_hot(),
            warm: TierConfig::default_warm(),
            cold: TierConfig::default_cold(),
            archive: None,
            policies: vec![],
            interval_secs: default_interval(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    pub name: String,

    pub path: Option<PathBuf>,

    /// Maximum size in bytes (0 = unlimited)
    #[serde(default)]
    pub max_size_bytes: u64,

    /// Maximum number of items (0 = unlimited)
    #[serde(default)]
    pub max_items: u64,

    /// Not applied: no tier stores data, so nothing is compressed.
    #[serde(default)]
    pub compression: bool,

    /// Not applied: see `compression`.
    #[serde(default)]
    pub compression_algorithm: CompressionAlgorithm,

    #[serde(default)]
    pub cache_enabled: bool,

    #[serde(default)]
    pub cache_size_bytes: u64,
}

impl TierConfig {
    fn default_hot() -> Self {
        Self {
            name: "hot".to_string(),
            path: None,
            max_size_bytes: 1024 * 1024 * 1024, // 1 GB
            max_items: 0,
            compression: false,
            compression_algorithm: CompressionAlgorithm::None,
            cache_enabled: true,
            cache_size_bytes: 256 * 1024 * 1024, // 256 MB
        }
    }

    fn default_warm() -> Self {
        Self {
            name: "warm".to_string(),
            path: None,
            max_size_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
            max_items: 0,
            compression: true,
            compression_algorithm: CompressionAlgorithm::Lz4,
            cache_enabled: false,
            cache_size_bytes: 0,
        }
    }

    fn default_cold() -> Self {
        Self {
            name: "cold".to_string(),
            path: None,
            max_size_bytes: 100 * 1024 * 1024 * 1024, // 100 GB
            max_items: 0,
            compression: true,
            compression_algorithm: CompressionAlgorithm::Zstd,
            cache_enabled: false,
            cache_size_bytes: 0,
        }
    }
}

impl Default for TierConfig {
    fn default() -> Self {
        Self::default_hot()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    pub endpoint: String,

    pub bucket: String,

    pub access_key: Option<String>,

    pub secret_key: Option<String>,

    pub region: Option<String>,

    #[serde(default = "default_true")]
    pub compression: bool,

    #[serde(default = "default_storage_class")]
    pub storage_class: String,
}

fn default_true() -> bool {
    true
}

fn default_storage_class() -> String {
    "STANDARD".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CompressionAlgorithm {
    #[default]
    None,
    Lz4,
    Zstd,
    Snappy,
    Gzip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringPolicy {
    pub name: String,

    pub prefix: Option<String>,

    pub rules: Vec<TieringRule>,

    /// Priority (higher = evaluated first)
    #[serde(default)]
    pub priority: i32,

    /// Is policy enabled?
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringRule {
    pub condition: TieringCondition,

    pub target_tier: Tier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Hot,
    Warm,
    Cold,
    Archive,
}

impl Tier {
    pub fn temperature(&self) -> i32 {
        match self {
            Tier::Hot => 3,
            Tier::Warm => 2,
            Tier::Cold => 1,
            Tier::Archive => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TieringCondition {
    AccessCount {
        min_count: Option<u64>,
        max_count: Option<u64>,
        window_secs: u64,
    },

    Age {
        min_age_secs: Option<u64>,
        max_age_secs: Option<u64>,
    },

    Size {
        min_bytes: Option<u64>,
        max_bytes: Option<u64>,
    },

    LastAccess {
        min_since_access_secs: Option<u64>,
        max_since_access_secs: Option<u64>,
    },

    And {
        conditions: Vec<TieringCondition>,
    },

    Or {
        conditions: Vec<TieringCondition>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMetadata {
    pub key: String,

    pub tier: Tier,

    pub size_bytes: u64,

    pub created_at: DateTime<Utc>,

    pub modified_at: DateTime<Utc>,

    pub last_accessed_at: DateTime<Utc>,

    pub access_count: u64,

    #[serde(default)]
    pub access_history: Vec<DateTime<Utc>>,

    /// Is compressed?
    pub compressed: bool,

    pub original_size_bytes: u64,
}

impl ItemMetadata {
    pub fn new(key: String, size_bytes: u64, tier: Tier) -> Self {
        let now = Utc::now();
        Self {
            key,
            tier,
            size_bytes,
            created_at: now,
            modified_at: now,
            last_accessed_at: now,
            access_count: 0,
            access_history: vec![],
            compressed: false,
            original_size_bytes: size_bytes,
        }
    }

    pub fn record_access(&mut self) {
        let now = Utc::now();
        self.last_accessed_at = now;
        self.access_count += 1;

        self.access_history.push(now);
        if self.access_history.len() > 100 {
            self.access_history.remove(0);
        }
    }

    pub fn access_count_in_window(&self, window: Duration) -> u64 {
        let cutoff = Utc::now() - window;
        self.access_history.iter().filter(|&t| *t >= cutoff).count() as u64
    }

    pub fn age_secs(&self) -> u64 {
        (Utc::now() - self.created_at).num_seconds().max(0) as u64
    }

    pub fn since_last_access_secs(&self) -> u64 {
        (Utc::now() - self.last_accessed_at).num_seconds().max(0) as u64
    }
}

pub struct TieringManager {
    config: TieringConfig,

    metadata: Arc<RwLock<HashMap<String, ItemMetadata>>>,

    stats: Arc<RwLock<TierStats>>,

    pending_changes: Arc<RwLock<Vec<TierChange>>>,
}

#[derive(Debug, Clone)]
pub struct TierChange {
    pub key: String,
    pub from_tier: Tier,
    pub to_tier: Tier,
    pub reason: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TierStats {
    pub hot: TierStat,
    pub warm: TierStat,
    pub cold: TierStat,
    pub archive: TierStat,

    pub total_promotions: u64,

    pub total_demotions: u64,

    /// Size of the keys whose tier [`TieringManager::apply_change`] changed.
    /// No data is actually moved.
    pub bytes_moved: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TierStat {
    pub item_count: u64,

    pub size_bytes: u64,

    pub reads: u64,

    pub writes: u64,

    pub cache_hits: u64,

    pub cache_misses: u64,
}

impl TieringManager {
    pub fn new(config: TieringConfig) -> Self {
        Self {
            config,
            metadata: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(TierStats::default())),
            pending_changes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn track_item(&self, key: &str, size_bytes: u64, tier: Tier) {
        let metadata = ItemMetadata::new(key.to_string(), size_bytes, tier);

        let mut items = self.metadata.write().unwrap();
        items.insert(key.to_string(), metadata);

        let mut stats = self.stats.write().unwrap();
        let stat = self.get_tier_stat_mut(&mut stats, tier);
        stat.item_count += 1;
        stat.size_bytes += size_bytes;
        stat.writes += 1;
    }

    pub fn record_access(&self, key: &str) -> Option<Tier> {
        let mut items = self.metadata.write().unwrap();
        if let Some(metadata) = items.get_mut(key) {
            metadata.record_access();
            let tier = metadata.tier;

            let mut stats = self.stats.write().unwrap();
            self.get_tier_stat_mut(&mut stats, tier).reads += 1;

            return Some(tier);
        }
        None
    }

    pub fn untrack_item(&self, key: &str) {
        let mut items = self.metadata.write().unwrap();
        if let Some(metadata) = items.remove(key) {
            let mut stats = self.stats.write().unwrap();
            let stat = self.get_tier_stat_mut(&mut stats, metadata.tier);
            stat.item_count = stat.item_count.saturating_sub(1);
            stat.size_bytes = stat.size_bytes.saturating_sub(metadata.size_bytes);
        }
    }

    pub fn get_metadata(&self, key: &str) -> Option<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        items.get(key).cloned()
    }

    pub fn get_tier(&self, key: &str) -> Option<Tier> {
        let items = self.metadata.read().unwrap();
        items.get(key).map(|m| m.tier)
    }

    pub fn evaluate_policies(&self) -> Vec<TierChange> {
        let items = self.metadata.read().unwrap();
        let mut changes = Vec::new();

        let mut policies = self.config.policies.clone();
        policies.sort_by_key(|b| std::cmp::Reverse(b.priority));

        for (key, metadata) in items.iter() {
            for policy in &policies {
                if !policy.enabled {
                    continue;
                }

                if let Some(ref prefix) = policy.prefix {
                    if !key.starts_with(prefix) {
                        continue;
                    }
                }

                for rule in &policy.rules {
                    if self.evaluate_condition(&rule.condition, metadata) {
                        if rule.target_tier != metadata.tier {
                            changes.push(TierChange {
                                key: key.clone(),
                                from_tier: metadata.tier,
                                to_tier: rule.target_tier,
                                reason: format!("Policy: {}", policy.name),
                            });
                        }
                        break;
                    }
                }
            }
        }

        let mut pending = self.pending_changes.write().unwrap();
        *pending = changes.clone();

        changes
    }

    #[allow(clippy::only_used_in_recursion)]
    fn evaluate_condition(&self, condition: &TieringCondition, metadata: &ItemMetadata) -> bool {
        match condition {
            TieringCondition::AccessCount {
                min_count,
                max_count,
                window_secs,
            } => {
                let count = metadata.access_count_in_window(Duration::seconds(*window_secs as i64));

                if let Some(min) = min_count {
                    if count < *min {
                        return false;
                    }
                }
                if let Some(max) = max_count {
                    if count > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::Age {
                min_age_secs,
                max_age_secs,
            } => {
                let age = metadata.age_secs();

                if let Some(min) = min_age_secs {
                    if age < *min {
                        return false;
                    }
                }
                if let Some(max) = max_age_secs {
                    if age > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::Size {
                min_bytes,
                max_bytes,
            } => {
                if let Some(min) = min_bytes {
                    if metadata.size_bytes < *min {
                        return false;
                    }
                }
                if let Some(max) = max_bytes {
                    if metadata.size_bytes > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::LastAccess {
                min_since_access_secs,
                max_since_access_secs,
            } => {
                let since = metadata.since_last_access_secs();

                if let Some(min) = min_since_access_secs {
                    if since < *min {
                        return false;
                    }
                }
                if let Some(max) = max_since_access_secs {
                    if since > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::And { conditions } => conditions
                .iter()
                .all(|c| self.evaluate_condition(c, metadata)),

            TieringCondition::Or { conditions } => conditions
                .iter()
                .any(|c| self.evaluate_condition(c, metadata)),
        }
    }

    /// Records that `change.key` now belongs to `change.to_tier`, and updates
    /// the statistics. No data is moved: no tier stores data.
    pub fn apply_change(&self, change: &TierChange) -> Result<()> {
        let mut items = self.metadata.write().unwrap();
        if let Some(metadata) = items.get_mut(&change.key) {
            let size = metadata.size_bytes;

            let mut stats = self.stats.write().unwrap();

            let from_stat = self.get_tier_stat_mut(&mut stats, change.from_tier);
            from_stat.item_count = from_stat.item_count.saturating_sub(1);
            from_stat.size_bytes = from_stat.size_bytes.saturating_sub(size);

            let to_stat = self.get_tier_stat_mut(&mut stats, change.to_tier);
            to_stat.item_count += 1;
            to_stat.size_bytes += size;

            stats.bytes_moved += size;

            if change.to_tier.temperature() > change.from_tier.temperature() {
                stats.total_promotions += 1;
            } else {
                stats.total_demotions += 1;
            }

            metadata.tier = change.to_tier;

            Ok(())
        } else {
            Err(Error::Internal(format!("Key not found: {}", change.key)))
        }
    }

    pub fn get_stats(&self) -> TierStats {
        self.stats.read().unwrap().clone()
    }

    pub fn get_pending_changes(&self) -> Vec<TierChange> {
        self.pending_changes.read().unwrap().clone()
    }

    pub fn items_in_tier(&self, tier: Tier) -> Vec<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        items.values().filter(|m| m.tier == tier).cloned().collect()
    }

    pub fn hottest_items(&self, limit: usize) -> Vec<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        let mut sorted: Vec<_> = items.values().cloned().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.access_count));
        sorted.truncate(limit);
        sorted
    }

    pub fn coldest_items(&self, limit: usize) -> Vec<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        let mut sorted: Vec<_> = items.values().cloned().collect();
        sorted.sort_by_key(|a| a.last_accessed_at);
        sorted.truncate(limit);
        sorted
    }

    fn get_tier_stat_mut<'a>(&self, stats: &'a mut TierStats, tier: Tier) -> &'a mut TierStat {
        match tier {
            Tier::Hot => &mut stats.hot,
            Tier::Warm => &mut stats.warm,
            Tier::Cold => &mut stats.cold,
            Tier::Archive => &mut stats.archive,
        }
    }
}

/// Returns `data` unchanged for `None`. The other algorithms are not
/// implemented, and fail.
pub fn compress(data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>> {
    match algorithm {
        CompressionAlgorithm::None => Ok(data.to_vec()),
        CompressionAlgorithm::Lz4
        | CompressionAlgorithm::Zstd
        | CompressionAlgorithm::Snappy
        | CompressionAlgorithm::Gzip => Err(NotImplemented::TIERING_COMPRESSION.into()),
    }
}

/// Returns `data` unchanged for `None`. The other algorithms are not
/// implemented, and fail.
pub fn decompress(data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>> {
    match algorithm {
        CompressionAlgorithm::None => Ok(data.to_vec()),
        CompressionAlgorithm::Lz4
        | CompressionAlgorithm::Zstd
        | CompressionAlgorithm::Snappy
        | CompressionAlgorithm::Gzip => Err(NotImplemented::TIERING_COMPRESSION.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> TieringConfig {
        TieringConfig {
            enabled: true,
            policies: vec![
                TieringPolicy {
                    name: "hot-access".to_string(),
                    prefix: None,
                    priority: 10,
                    enabled: true,
                    rules: vec![TieringRule {
                        condition: TieringCondition::AccessCount {
                            min_count: Some(100),
                            max_count: None,
                            window_secs: 3600,
                        },
                        target_tier: Tier::Hot,
                    }],
                },
                TieringPolicy {
                    name: "cold-age".to_string(),
                    prefix: None,
                    priority: 5,
                    enabled: true,
                    rules: vec![TieringRule {
                        condition: TieringCondition::Age {
                            min_age_secs: Some(86400 * 30), // 30 days
                            max_age_secs: None,
                        },
                        target_tier: Tier::Cold,
                    }],
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn test_track_item() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 1000, Tier::Hot);
        manager.track_item("key2", 2000, Tier::Warm);

        let stats = manager.get_stats();
        assert_eq!(stats.hot.item_count, 1);
        assert_eq!(stats.hot.size_bytes, 1000);
        assert_eq!(stats.warm.item_count, 1);
        assert_eq!(stats.warm.size_bytes, 2000);
    }

    #[test]
    fn test_record_access() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 1000, Tier::Hot);

        for _ in 0..10 {
            manager.record_access("key1");
        }

        let metadata = manager.get_metadata("key1").unwrap();
        assert_eq!(metadata.access_count, 10);
    }

    #[test]
    fn test_tier_change() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 1000, Tier::Hot);

        let change = TierChange {
            key: "key1".to_string(),
            from_tier: Tier::Hot,
            to_tier: Tier::Warm,
            reason: "Test".to_string(),
        };

        manager.apply_change(&change).unwrap();

        let metadata = manager.get_metadata("key1").unwrap();
        assert_eq!(metadata.tier, Tier::Warm);

        let stats = manager.get_stats();
        assert_eq!(stats.hot.item_count, 0);
        assert_eq!(stats.warm.item_count, 1);
        assert_eq!(stats.total_demotions, 1);
    }

    #[test]
    fn only_the_none_algorithm_is_implemented() {
        let data = b"minikv";
        assert_eq!(compress(data, CompressionAlgorithm::None).unwrap(), data);
        assert_eq!(decompress(data, CompressionAlgorithm::None).unwrap(), data);

        for algorithm in [
            CompressionAlgorithm::Lz4,
            CompressionAlgorithm::Zstd,
            CompressionAlgorithm::Snappy,
            CompressionAlgorithm::Gzip,
        ] {
            for result in [compress(data, algorithm), decompress(data, algorithm)] {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    "tiering compression is not implemented (roadmap: unscheduled)",
                    "{:?}",
                    algorithm
                );
            }
        }
    }

    #[test]
    fn test_hottest_items() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 100, Tier::Hot);
        manager.track_item("key2", 100, Tier::Hot);
        manager.track_item("key3", 100, Tier::Hot);

        for _ in 0..50 {
            manager.record_access("key1");
        }
        for _ in 0..100 {
            manager.record_access("key2");
        }
        for _ in 0..10 {
            manager.record_access("key3");
        }

        let hottest = manager.hottest_items(2);
        assert_eq!(hottest.len(), 2);
        assert_eq!(hottest[0].key, "key2");
        assert_eq!(hottest[1].key, "key1");
    }
}
