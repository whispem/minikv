//! Authentication and Authorization module (v0.6.0)
//!
//! This module provides:
//! - API key generation and validation
//! - JWT token support for stateless authentication
//! - Role-based access control (RBAC)
//! - Tenant isolation for multi-tenancy

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Default JWT secret (should be overridden in production via config)
const DEFAULT_JWT_SECRET: &[u8] = b"minikv-default-secret-change-in-production";

/// API key prefix for easy identification
const API_KEY_PREFIX: &str = "mkv_";

/// API key length (excluding prefix)
const API_KEY_LENGTH: usize = 32;

/// JWT token expiration (24 hours by default)
const JWT_EXPIRATION_HOURS: u64 = 24;

/// Role defining access levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Role {
    /// Full access to all operations including admin
    Admin,
    /// Read and write access to data
    ReadWrite,
    /// Read-only access to data (default)
    #[default]
    ReadOnly,
}

impl Role {
    /// Check if this role can perform write operations
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Admin | Role::ReadWrite)
    }

    /// Check if this role can perform admin operations
    pub fn can_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }

    /// Check if this role can read data
    pub fn can_read(&self) -> bool {
        true // All roles can read
    }
}

/// Represents an API key with associated metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique identifier for the key
    pub id: String,
    /// Human-readable name/description
    pub name: String,
    /// The hashed key (never store plaintext)
    #[serde(skip_serializing)]
    pub key_hash: String,
    /// Tenant/namespace this key belongs to
    pub tenant: String,
    /// Role assigned to this key
    pub role: Role,
    /// Creation timestamp (Unix epoch ms)
    pub created_at: u64,
    /// Expiration timestamp (Unix epoch ms), None = never expires
    pub expires_at: Option<u64>,
    /// Whether the key is active
    pub active: bool,
    /// Last used timestamp
    pub last_used_at: Option<u64>,
}

impl ApiKey {
    /// Check if the key is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            now >= expires_at
        } else {
            false
        }
    }

    /// Check if the key is valid (active and not expired)
    pub fn is_valid(&self) -> bool {
        self.active && !self.is_expired()
    }
}

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (key ID)
    pub sub: String,
    /// Tenant/namespace
    pub tenant: String,
    /// Role
    pub role: Role,
    /// Expiration time (Unix timestamp)
    pub exp: u64,
    /// Issued at (Unix timestamp)
    pub iat: u64,
}

/// Authentication context extracted from a valid request
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Key ID or JWT subject
    pub key_id: String,
    /// Tenant/namespace
    pub tenant: String,
    /// Role
    pub role: Role,
}

impl AuthContext {
    /// Check if this context allows write operations
    pub fn can_write(&self) -> bool {
        self.role.can_write()
    }

    /// Check if this context allows admin operations
    pub fn can_admin(&self) -> bool {
        self.role.can_admin()
    }
}

/// Result of authentication attempt
#[derive(Debug)]
pub enum AuthResult {
    /// Authentication successful
    Ok(AuthContext),
    /// No authentication provided
    Missing,
    /// Invalid credentials
    Invalid(String),
    /// Expired credentials
    Expired,
    /// Insufficient permissions
    Forbidden(String),
}

/// API Key store for managing keys
pub struct KeyStore {
    /// Map of key_id -> ApiKey
    keys: RwLock<HashMap<String, ApiKey>>,
    /// Map of key_hash -> key_id for fast lookup
    hash_to_id: RwLock<HashMap<String, String>>,
    /// JWT encoding key
    jwt_encoding_key: EncodingKey,
    /// JWT decoding key
    jwt_decoding_key: DecodingKey,
    /// Argon2 hasher
    argon2: Argon2<'static>,
}

impl KeyStore {
    /// Create a new key store with default JWT secret
    pub fn new() -> Self {
        Self::with_secret(DEFAULT_JWT_SECRET)
    }

    /// Create a new key store with custom JWT secret
    pub fn with_secret(secret: &[u8]) -> Self {
        Self {
            keys: RwLock::new(HashMap::new()),
            hash_to_id: RwLock::new(HashMap::new()),
            jwt_encoding_key: EncodingKey::from_secret(secret),
            jwt_decoding_key: DecodingKey::from_secret(secret),
            argon2: Argon2::default(),
        }
    }

    /// Generate a new API key
    /// Returns (key_id, plaintext_key) - the plaintext key is only shown once!
    pub fn generate_key(
        &self,
        name: &str,
        tenant: &str,
        role: Role,
        expires_in: Option<Duration>,
    ) -> Result<(String, String), AuthError> {
        // Generate random key
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; API_KEY_LENGTH] = rng.gen();
        let key_suffix = URL_SAFE_NO_PAD.encode(random_bytes);
        let plaintext_key = format!("{}{}", API_KEY_PREFIX, key_suffix);

        // Generate key ID
        let key_id = uuid::Uuid::new_v4().to_string();

        // Hash the key
        let salt = SaltString::generate(&mut OsRng);
        let key_hash = self
            .argon2
            .hash_password(plaintext_key.as_bytes(), &salt)
            .map_err(|e| AuthError::HashError(e.to_string()))?
            .to_string();

        // Calculate expiration
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let expires_at = expires_in.map(|d| now + d.as_millis() as u64);

        // Create API key record
        let api_key = ApiKey {
            id: key_id.clone(),
            name: name.to_string(),
            key_hash: key_hash.clone(),
            tenant: tenant.to_string(),
            role,
            created_at: now,
            expires_at,
            active: true,
            last_used_at: None,
        };

        // Store the key
        {
            let mut keys = self.keys.write().unwrap();
            keys.insert(key_id.clone(), api_key);
        }
        {
            let mut hash_to_id = self.hash_to_id.write().unwrap();
            hash_to_id.insert(key_hash, key_id.clone());
        }

        Ok((key_id, plaintext_key))
    }

    /// Validate an API key and return the auth context
    pub fn validate_key(&self, key: &str) -> AuthResult {
        // Check prefix
        if !key.starts_with(API_KEY_PREFIX) {
            return AuthResult::Invalid("Invalid key format".to_string());
        }

        // Find the key by trying to verify against all stored hashes
        let keys = self.keys.read().unwrap();
        for api_key in keys.values() {
            if let Ok(parsed_hash) = PasswordHash::new(&api_key.key_hash) {
                if self
                    .argon2
                    .verify_password(key.as_bytes(), &parsed_hash)
                    .is_ok()
                {
                    // Found matching key
                    if !api_key.active {
                        return AuthResult::Invalid("Key is disabled".to_string());
                    }
                    if api_key.is_expired() {
                        return AuthResult::Expired;
                    }

                    return AuthResult::Ok(AuthContext {
                        key_id: api_key.id.clone(),
                        tenant: api_key.tenant.clone(),
                        role: api_key.role,
                    });
                }
            }
        }

        AuthResult::Invalid("Invalid API key".to_string())
    }

    /// Generate a JWT token for an authenticated key
    pub fn generate_jwt(&self, auth: &AuthContext) -> Result<String, AuthError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: auth.key_id.clone(),
            tenant: auth.tenant.clone(),
            role: auth.role,
            exp: now + (JWT_EXPIRATION_HOURS * 3600),
            iat: now,
        };

        encode(&Header::default(), &claims, &self.jwt_encoding_key)
            .map_err(|e| AuthError::JwtError(e.to_string()))
    }

    /// Validate a JWT token
    pub fn validate_jwt(&self, token: &str) -> AuthResult {
        let validation = Validation::default();

        match decode::<Claims>(token, &self.jwt_decoding_key, &validation) {
            Ok(token_data) => {
                let claims = token_data.claims;

                // Check expiration
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                if now >= claims.exp {
                    return AuthResult::Expired;
                }

                AuthResult::Ok(AuthContext {
                    key_id: claims.sub,
                    tenant: claims.tenant,
                    role: claims.role,
                })
            }
            Err(e) => AuthResult::Invalid(format!("Invalid JWT: {}", e)),
        }
    }

    /// Authenticate from Authorization header value
    /// Supports: "Bearer <token>" and "ApiKey <key>"
    pub fn authenticate(&self, auth_header: &str) -> AuthResult {
        let parts: Vec<&str> = auth_header.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return AuthResult::Invalid("Invalid Authorization header format".to_string());
        }

        match parts[0].to_lowercase().as_str() {
            "bearer" => self.validate_jwt(parts[1]),
            "apikey" => self.validate_key(parts[1]),
            _ => AuthResult::Invalid(format!("Unknown auth scheme: {}", parts[0])),
        }
    }

    /// Get an API key by ID (for admin purposes)
    pub fn get_key(&self, key_id: &str) -> Option<ApiKey> {
        let keys = self.keys.read().unwrap();
        keys.get(key_id).cloned()
    }

    /// List all API keys (for admin purposes)
    pub fn list_keys(&self) -> Vec<ApiKey> {
        let keys = self.keys.read().unwrap();
        keys.values().cloned().collect()
    }

    /// List keys for a specific tenant
    pub fn list_keys_for_tenant(&self, tenant: &str) -> Vec<ApiKey> {
        let keys = self.keys.read().unwrap();
        keys.values()
            .filter(|k| k.tenant == tenant)
            .cloned()
            .collect()
    }

    /// Revoke (disable) an API key
    pub fn revoke_key(&self, key_id: &str) -> Result<(), AuthError> {
        let mut keys = self.keys.write().unwrap();
        if let Some(key) = keys.get_mut(key_id) {
            key.active = false;
            Ok(())
        } else {
            Err(AuthError::KeyNotFound(key_id.to_string()))
        }
    }

    /// Delete an API key permanently
    pub fn delete_key(&self, key_id: &str) -> Result<(), AuthError> {
        let mut keys = self.keys.write().unwrap();
        if let Some(key) = keys.remove(key_id) {
            let mut hash_to_id = self.hash_to_id.write().unwrap();
            hash_to_id.remove(&key.key_hash);
            Ok(())
        } else {
            Err(AuthError::KeyNotFound(key_id.to_string()))
        }
    }

    /// Update last_used_at timestamp for a key
    pub fn touch_key(&self, key_id: &str) {
        let mut keys = self.keys.write().unwrap();
        if let Some(key) = keys.get_mut(key_id) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            key.last_used_at = Some(now);
        }
    }
}

impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Authentication errors
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Hash error: {0}")]
    HashError(String),
    #[error("JWT error: {0}")]
    JwtError(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
}

/// Global key store instance
pub static KEY_STORE: Lazy<Arc<KeyStore>> = Lazy::new(|| Arc::new(KeyStore::new()));

/// Configuration for authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Whether authentication is enabled
    pub enabled: bool,
    /// JWT secret (base64 encoded)
    pub jwt_secret: Option<String>,
    /// Whether to require auth for read operations
    pub require_auth_for_reads: bool,
    /// List of paths that don't require authentication
    pub public_paths: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jwt_secret: None,
            require_auth_for_reads: false,
            public_paths: vec![
                "/health".to_string(),
                "/health/ready".to_string(),
                "/health/live".to_string(),
                "/metrics".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_key() {
        let store = KeyStore::new();

        // Generate a key
        let (key_id, plaintext) = store
            .generate_key("test-key", "default", Role::ReadWrite, None)
            .unwrap();

        assert!(!key_id.is_empty());
        assert!(plaintext.starts_with(API_KEY_PREFIX));

        // Validate the key
        match store.validate_key(&plaintext) {
            AuthResult::Ok(ctx) => {
                assert_eq!(ctx.key_id, key_id);
                assert_eq!(ctx.tenant, "default");
                assert_eq!(ctx.role, Role::ReadWrite);
            }
            _ => panic!("Expected valid key"),
        }
    }

    #[test]
    fn test_invalid_key() {
        let store = KeyStore::new();

        match store.validate_key("mkv_invalid_key_here") {
            AuthResult::Invalid(_) => {}
            _ => panic!("Expected invalid key"),
        }
    }

    #[test]
    fn test_jwt_generation_and_validation() {
        let store = KeyStore::new();

        let ctx = AuthContext {
            key_id: "test-key".to_string(),
            tenant: "default".to_string(),
            role: Role::Admin,
        };

        // Generate JWT
        let token = store.generate_jwt(&ctx).unwrap();
        assert!(!token.is_empty());

        // Validate JWT
        match store.validate_jwt(&token) {
            AuthResult::Ok(validated_ctx) => {
                assert_eq!(validated_ctx.key_id, ctx.key_id);
                assert_eq!(validated_ctx.tenant, ctx.tenant);
                assert_eq!(validated_ctx.role, ctx.role);
            }
            _ => panic!("Expected valid JWT"),
        }
    }

    #[test]
    fn test_revoke_key() {
        let store = KeyStore::new();

        let (key_id, plaintext) = store
            .generate_key("test-key", "default", Role::ReadWrite, None)
            .unwrap();

        // Revoke the key
        store.revoke_key(&key_id).unwrap();

        // Validate should fail
        match store.validate_key(&plaintext) {
            AuthResult::Invalid(msg) => {
                assert!(msg.contains("disabled"));
            }
            _ => panic!("Expected disabled key"),
        }
    }

    #[test]
    fn test_roles() {
        assert!(Role::Admin.can_read());
        assert!(Role::Admin.can_write());
        assert!(Role::Admin.can_admin());

        assert!(Role::ReadWrite.can_read());
        assert!(Role::ReadWrite.can_write());
        assert!(!Role::ReadWrite.can_admin());

        assert!(Role::ReadOnly.can_read());
        assert!(!Role::ReadOnly.can_write());
        assert!(!Role::ReadOnly.can_admin());
    }

    #[test]
    fn test_authenticate_header() {
        let store = KeyStore::new();

        let (_, plaintext) = store
            .generate_key("test-key", "default", Role::ReadWrite, None)
            .unwrap();

        // Test ApiKey scheme
        let header = format!("ApiKey {}", plaintext);
        match store.authenticate(&header) {
            AuthResult::Ok(_) => {}
            _ => panic!("Expected valid auth"),
        }

        // Test Bearer scheme with JWT
        let ctx = AuthContext {
            key_id: "test".to_string(),
            tenant: "default".to_string(),
            role: Role::Admin,
        };
        let token = store.generate_jwt(&ctx).unwrap();
        let header = format!("Bearer {}", token);
        match store.authenticate(&header) {
            AuthResult::Ok(_) => {}
            _ => panic!("Expected valid auth"),
        }
    }
}
