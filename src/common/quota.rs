//! Quota management module for MiniKV v0.6.0
//!
//! This module provides tenant-based resource quotas including:
//! - Storage limits (bytes)
//! - Object count limits
//! - Request rate limiting
//!
//! Quotas can be configured per-tenant and are enforced at the coordinator level.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Default quota configuration
const DEFAULT_STORAGE_LIMIT: u64 = 10 * 1024 * 1024 * 1024; // 10 GB
const DEFAULT_OBJECT_LIMIT: u64 = 1_000_000; // 1 million objects
const DEFAULT_RATE_LIMIT: u32 = 1000; // requests per second
const DEFAULT_RATE_WINDOW: Duration = Duration::from_secs(1);

/// Global quota manager instance
pub static QUOTA_MANAGER: Lazy<QuotaManager> = Lazy::new(QuotaManager::new);

/// Quota configuration for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    /// Tenant identifier
    pub tenant_id: String,
    /// Maximum storage in bytes (0 = unlimited)
    pub storage_limit: u64,
    /// Maximum number of objects (0 = unlimited)
    pub object_limit: u64,
    /// Maximum requests per rate window (0 = unlimited)
    pub rate_limit: u32,
    /// Whether the tenant is enabled
    pub enabled: bool,
    /// When the quota was created
    #[serde(skip)]
    pub created_at: Option<Instant>,
}

impl TenantQuota {
    /// Create a new tenant quota with default limits
    pub fn new(tenant_id: String) -> Self {
        Self {
            tenant_id,
            storage_limit: DEFAULT_STORAGE_LIMIT,
            object_limit: DEFAULT_OBJECT_LIMIT,
            rate_limit: DEFAULT_RATE_LIMIT,
            enabled: true,
            created_at: Some(Instant::now()),
        }
    }

    /// Create a quota with unlimited resources
    pub fn unlimited(tenant_id: String) -> Self {
        Self {
            tenant_id,
            storage_limit: 0,
            object_limit: 0,
            rate_limit: 0,
            enabled: true,
            created_at: Some(Instant::now()),
        }
    }

    /// Create a quota with custom limits
    pub fn with_limits(
        tenant_id: String,
        storage_limit: u64,
        object_limit: u64,
        rate_limit: u32,
    ) -> Self {
        Self {
            tenant_id,
            storage_limit,
            object_limit,
            rate_limit,
            enabled: true,
            created_at: Some(Instant::now()),
        }
    }
}

/// Current resource usage for a tenant
#[derive(Debug, Clone, Default)]
pub struct TenantUsage {
    /// Current storage used in bytes
    pub storage_used: u64,
    /// Current number of objects
    pub object_count: u64,
    /// Request timestamps for rate limiting
    pub request_times: Vec<Instant>,
}

impl TenantUsage {
    /// Check if adding storage would exceed the limit
    pub fn check_storage(&self, quota: &TenantQuota, additional_bytes: u64) -> bool {
        if quota.storage_limit == 0 {
            return true; // Unlimited
        }
        self.storage_used + additional_bytes <= quota.storage_limit
    }

    /// Check if adding an object would exceed the limit
    pub fn check_objects(&self, quota: &TenantQuota, additional_objects: u64) -> bool {
        if quota.object_limit == 0 {
            return true; // Unlimited
        }
        self.object_count + additional_objects <= quota.object_limit
    }

    /// Check if a request would exceed the rate limit
    pub fn check_rate(&mut self, quota: &TenantQuota) -> bool {
        if quota.rate_limit == 0 {
            return true; // Unlimited
        }

        let now = Instant::now();
        let window_start = now - DEFAULT_RATE_WINDOW;

        // Remove old request timestamps
        self.request_times.retain(|&t| t > window_start);

        // Check if under the limit
        (self.request_times.len() as u32) < quota.rate_limit
    }

    /// Record a request for rate limiting
    pub fn record_request(&mut self) {
        self.request_times.push(Instant::now());
    }

    /// Update storage usage
    pub fn add_storage(&mut self, bytes: u64) {
        self.storage_used = self.storage_used.saturating_add(bytes);
    }

    /// Remove storage usage
    pub fn remove_storage(&mut self, bytes: u64) {
        self.storage_used = self.storage_used.saturating_sub(bytes);
    }

    /// Increment object count
    pub fn add_objects(&mut self, count: u64) {
        self.object_count = self.object_count.saturating_add(count);
    }

    /// Decrement object count
    pub fn remove_objects(&mut self, count: u64) {
        self.object_count = self.object_count.saturating_sub(count);
    }
}

/// Result of a quota check
#[derive(Debug, Clone)]
pub enum QuotaCheckResult {
    /// Request is allowed
    Allowed,
    /// Tenant not found (treated as default quota)
    TenantNotFound,
    /// Storage limit exceeded
    StorageLimitExceeded {
        limit: u64,
        used: u64,
        requested: u64,
    },
    /// Object limit exceeded
    ObjectLimitExceeded { limit: u64, count: u64 },
    /// Rate limit exceeded
    RateLimitExceeded { limit: u32, window_secs: u64 },
    /// Tenant is disabled
    TenantDisabled,
}

impl QuotaCheckResult {
    /// Check if the result allows the operation
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            QuotaCheckResult::Allowed | QuotaCheckResult::TenantNotFound
        )
    }

    /// Convert to an error message if not allowed
    pub fn error_message(&self) -> Option<String> {
        match self {
            QuotaCheckResult::Allowed | QuotaCheckResult::TenantNotFound => None,
            QuotaCheckResult::StorageLimitExceeded {
                limit,
                used,
                requested,
            } => Some(format!(
                "Storage quota exceeded: limit={} bytes, used={} bytes, requested={} bytes",
                limit, used, requested
            )),
            QuotaCheckResult::ObjectLimitExceeded { limit, count } => Some(format!(
                "Object count quota exceeded: limit={}, current count={}",
                limit, count
            )),
            QuotaCheckResult::RateLimitExceeded { limit, window_secs } => Some(format!(
                "Rate limit exceeded: {} requests per {} second(s)",
                limit, window_secs
            )),
            QuotaCheckResult::TenantDisabled => Some("Tenant is disabled".to_string()),
        }
    }
}

/// Quota manager for tracking and enforcing tenant quotas
pub struct QuotaManager {
    /// Quota configurations per tenant
    quotas: RwLock<HashMap<String, TenantQuota>>,
    /// Current usage per tenant
    usage: RwLock<HashMap<String, TenantUsage>>,
    /// Default quota for tenants without explicit configuration
    default_quota: TenantQuota,
}

impl QuotaManager {
    /// Create a new quota manager
    pub fn new() -> Self {
        Self {
            quotas: RwLock::new(HashMap::new()),
            usage: RwLock::new(HashMap::new()),
            default_quota: TenantQuota::new("__default__".to_string()),
        }
    }

    /// Set the default quota for new tenants
    pub fn set_default_quota(&mut self, quota: TenantQuota) {
        self.default_quota = quota;
    }

    /// Create or update a tenant's quota
    pub fn set_quota(&self, quota: TenantQuota) {
        let mut quotas = self.quotas.write().unwrap();
        quotas.insert(quota.tenant_id.clone(), quota);
    }

    /// Get a tenant's quota configuration
    pub fn get_quota(&self, tenant_id: &str) -> Option<TenantQuota> {
        let quotas = self.quotas.read().unwrap();
        quotas.get(tenant_id).cloned()
    }

    /// Remove a tenant's quota configuration
    pub fn remove_quota(&self, tenant_id: &str) -> Option<TenantQuota> {
        let mut quotas = self.quotas.write().unwrap();
        quotas.remove(tenant_id)
    }

    /// Get a tenant's current usage
    pub fn get_usage(&self, tenant_id: &str) -> TenantUsage {
        let usage = self.usage.read().unwrap();
        usage.get(tenant_id).cloned().unwrap_or_default()
    }

    /// List all configured quotas
    pub fn list_quotas(&self) -> Vec<TenantQuota> {
        let quotas = self.quotas.read().unwrap();
        quotas.values().cloned().collect()
    }

    /// Check if a storage operation is allowed
    pub fn check_storage(&self, tenant_id: &str, additional_bytes: u64) -> QuotaCheckResult {
        let quotas = self.quotas.read().unwrap();
        let quota = quotas.get(tenant_id).unwrap_or(&self.default_quota);

        if !quota.enabled {
            return QuotaCheckResult::TenantDisabled;
        }

        let usage = self.usage.read().unwrap();
        let tenant_usage = usage.get(tenant_id).cloned().unwrap_or_default();

        if tenant_usage.check_storage(quota, additional_bytes) {
            QuotaCheckResult::Allowed
        } else {
            QuotaCheckResult::StorageLimitExceeded {
                limit: quota.storage_limit,
                used: tenant_usage.storage_used,
                requested: additional_bytes,
            }
        }
    }

    /// Check if adding an object is allowed
    pub fn check_objects(&self, tenant_id: &str) -> QuotaCheckResult {
        let quotas = self.quotas.read().unwrap();
        let quota = quotas.get(tenant_id).unwrap_or(&self.default_quota);

        if !quota.enabled {
            return QuotaCheckResult::TenantDisabled;
        }

        let usage = self.usage.read().unwrap();
        let tenant_usage = usage.get(tenant_id).cloned().unwrap_or_default();

        if tenant_usage.check_objects(quota, 1) {
            QuotaCheckResult::Allowed
        } else {
            QuotaCheckResult::ObjectLimitExceeded {
                limit: quota.object_limit,
                count: tenant_usage.object_count,
            }
        }
    }

    /// Check and record a request for rate limiting
    pub fn check_and_record_request(&self, tenant_id: &str) -> QuotaCheckResult {
        let quotas = self.quotas.read().unwrap();
        let quota = quotas.get(tenant_id).unwrap_or(&self.default_quota);

        if !quota.enabled {
            return QuotaCheckResult::TenantDisabled;
        }

        let mut usage = self.usage.write().unwrap();
        let tenant_usage = usage.entry(tenant_id.to_string()).or_default();

        if tenant_usage.check_rate(quota) {
            tenant_usage.record_request();
            QuotaCheckResult::Allowed
        } else {
            QuotaCheckResult::RateLimitExceeded {
                limit: quota.rate_limit,
                window_secs: DEFAULT_RATE_WINDOW.as_secs(),
            }
        }
    }

    /// Record storage being added for a tenant
    pub fn record_storage_add(&self, tenant_id: &str, bytes: u64) {
        let mut usage = self.usage.write().unwrap();
        let tenant_usage = usage.entry(tenant_id.to_string()).or_default();
        tenant_usage.add_storage(bytes);
        tenant_usage.add_objects(1);
    }

    /// Record storage being removed for a tenant
    pub fn record_storage_remove(&self, tenant_id: &str, bytes: u64) {
        let mut usage = self.usage.write().unwrap();
        if let Some(tenant_usage) = usage.get_mut(tenant_id) {
            tenant_usage.remove_storage(bytes);
            tenant_usage.remove_objects(1);
        }
    }

    /// Get usage statistics in Prometheus format
    pub fn to_prometheus(&self) -> String {
        let mut out = String::new();
        let usage = self.usage.read().unwrap();
        let quotas = self.quotas.read().unwrap();

        for (tenant_id, tenant_usage) in usage.iter() {
            let quota = quotas.get(tenant_id).unwrap_or(&self.default_quota);

            out += &format!(
                "minikv_tenant_storage_used_bytes{{tenant=\"{}\"}} {}\n",
                tenant_id, tenant_usage.storage_used
            );
            out += &format!(
                "minikv_tenant_storage_limit_bytes{{tenant=\"{}\"}} {}\n",
                tenant_id, quota.storage_limit
            );
            out += &format!(
                "minikv_tenant_object_count{{tenant=\"{}\"}} {}\n",
                tenant_id, tenant_usage.object_count
            );
            out += &format!(
                "minikv_tenant_object_limit{{tenant=\"{}\"}} {}\n",
                tenant_id, quota.object_limit
            );
        }

        out
    }
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_quota() {
        let manager = QuotaManager::new();
        let result = manager.check_storage("test_tenant", 1024);
        assert!(result.is_allowed());
    }

    #[test]
    fn test_storage_limit() {
        let manager = QuotaManager::new();

        // Set a small quota
        let quota = TenantQuota::with_limits("test_tenant".to_string(), 1024, 10, 100);
        manager.set_quota(quota);

        // Check within limit
        let result = manager.check_storage("test_tenant", 512);
        assert!(result.is_allowed());

        // Record some usage
        manager.record_storage_add("test_tenant", 512);

        // Check that would exceed
        let result = manager.check_storage("test_tenant", 1024);
        assert!(!result.is_allowed());
        assert!(matches!(
            result,
            QuotaCheckResult::StorageLimitExceeded { .. }
        ));
    }

    #[test]
    fn test_object_limit() {
        let manager = QuotaManager::new();

        // Set a quota with 2 object limit
        let quota = TenantQuota::with_limits("test_tenant".to_string(), 1024 * 1024, 2, 100);
        manager.set_quota(quota);

        // Add 2 objects
        manager.record_storage_add("test_tenant", 100);
        manager.record_storage_add("test_tenant", 100);

        // Third should fail
        let result = manager.check_objects("test_tenant");
        assert!(!result.is_allowed());
        assert!(matches!(
            result,
            QuotaCheckResult::ObjectLimitExceeded { .. }
        ));
    }

    #[test]
    fn test_disabled_tenant() {
        let manager = QuotaManager::new();

        let mut quota = TenantQuota::new("disabled_tenant".to_string());
        quota.enabled = false;
        manager.set_quota(quota);

        let result = manager.check_storage("disabled_tenant", 1);
        assert!(!result.is_allowed());
        assert!(matches!(result, QuotaCheckResult::TenantDisabled));
    }

    #[test]
    fn test_rate_limiting() {
        let manager = QuotaManager::new();

        // Set a quota with 3 requests per second limit
        let quota = TenantQuota::with_limits("rate_test".to_string(), 1024 * 1024, 1000, 3);
        manager.set_quota(quota);

        // First 3 requests should succeed
        assert!(manager.check_and_record_request("rate_test").is_allowed());
        assert!(manager.check_and_record_request("rate_test").is_allowed());
        assert!(manager.check_and_record_request("rate_test").is_allowed());

        // 4th should fail
        let result = manager.check_and_record_request("rate_test");
        assert!(!result.is_allowed());
        assert!(matches!(result, QuotaCheckResult::RateLimitExceeded { .. }));
    }

    #[test]
    fn test_unlimited_quota() {
        let manager = QuotaManager::new();

        let quota = TenantQuota::unlimited("unlimited_tenant".to_string());
        manager.set_quota(quota);

        // Should allow any amount
        let result = manager.check_storage("unlimited_tenant", u64::MAX / 2);
        assert!(result.is_allowed());
    }
}
