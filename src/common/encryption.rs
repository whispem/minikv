//! AES-256-GCM encryption primitives, with keys derived by HKDF-SHA256.
//!
//! minikv does not encrypt data at rest: no code of the volumes, of the WAL
//! or of the metadata store calls this module, so values are stored as sent.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use once_cell::sync::Lazy;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::RwLock;

const KEY_SIZE: usize = 32;

const NONCE_SIZE: usize = 12;

const TAG_SIZE: usize = 16;

const ENCRYPTION_MAGIC: &[u8] = b"MKVENC01";

pub static ENCRYPTION_MANAGER: Lazy<RwLock<EncryptionManager>> =
    Lazy::new(|| RwLock::new(EncryptionManager::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub enabled: bool,
    pub master_key: Option<String>,
    /// Not read: [`EncryptionManager::initialize`] always derives two keys,
    /// with the contexts `minikv-data` and `minikv-wal`.
    pub key_contexts: Vec<String>,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            master_key: None,
            key_contexts: vec![
                "minikv-data".to_string(),
                "minikv-wal".to_string(),
                "minikv-index".to_string(),
            ],
        }
    }
}

pub type EncryptionResult<T> = std::result::Result<T, EncryptionError>;

#[derive(Debug, Clone)]
pub enum EncryptionError {
    NotEnabled,
    InvalidKey(String),
    EncryptionFailed(String),
    DecryptionFailed(String),
    InvalidFormat(String),
    KeyDerivationFailed(String),
}

impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionError::NotEnabled => write!(f, "Encryption is not enabled"),
            EncryptionError::InvalidKey(msg) => write!(f, "Invalid encryption key: {}", msg),
            EncryptionError::EncryptionFailed(msg) => write!(f, "Encryption failed: {}", msg),
            EncryptionError::DecryptionFailed(msg) => write!(f, "Decryption failed: {}", msg),
            EncryptionError::InvalidFormat(msg) => {
                write!(f, "Invalid encrypted data format: {}", msg)
            }
            EncryptionError::KeyDerivationFailed(msg) => {
                write!(f, "Key derivation failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for EncryptionError {}

#[derive(Debug, Clone)]
pub struct EncryptedData {
    pub nonce: [u8; NONCE_SIZE],
    pub ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// Serialize to bytes: MAGIC || NONCE || CIPHERTEXT
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes =
            Vec::with_capacity(ENCRYPTION_MAGIC.len() + NONCE_SIZE + self.ciphertext.len());
        bytes.extend_from_slice(ENCRYPTION_MAGIC);
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> EncryptionResult<Self> {
        let min_size = ENCRYPTION_MAGIC.len() + NONCE_SIZE + TAG_SIZE;
        if bytes.len() < min_size {
            return Err(EncryptionError::InvalidFormat(format!(
                "Data too short: {} bytes, minimum {} bytes",
                bytes.len(),
                min_size
            )));
        }

        if &bytes[..ENCRYPTION_MAGIC.len()] != ENCRYPTION_MAGIC {
            return Err(EncryptionError::InvalidFormat(
                "Invalid magic bytes - data may not be encrypted".to_string(),
            ));
        }

        let nonce_start = ENCRYPTION_MAGIC.len();
        let ciphertext_start = nonce_start + NONCE_SIZE;

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&bytes[nonce_start..ciphertext_start]);

        let ciphertext = bytes[ciphertext_start..].to_vec();

        Ok(Self { nonce, ciphertext })
    }

    pub fn is_encrypted(bytes: &[u8]) -> bool {
        bytes.len() >= ENCRYPTION_MAGIC.len()
            && &bytes[..ENCRYPTION_MAGIC.len()] == ENCRYPTION_MAGIC
    }
}

pub struct EncryptionManager {
    config: EncryptionConfig,
    data_key: Option<[u8; KEY_SIZE]>,
    wal_key: Option<[u8; KEY_SIZE]>,
    data_cipher: Option<Aes256Gcm>,
    wal_cipher: Option<Aes256Gcm>,
}

impl EncryptionManager {
    pub fn new() -> Self {
        Self {
            config: EncryptionConfig::default(),
            data_key: None,
            wal_key: None,
            data_cipher: None,
            wal_cipher: None,
        }
    }

    pub fn initialize(&mut self, master_key: &str) -> EncryptionResult<()> {
        let key_bytes = BASE64
            .decode(master_key)
            .map_err(|e| EncryptionError::InvalidKey(format!("Invalid base64: {}", e)))?;

        if key_bytes.len() < 32 {
            return Err(EncryptionError::InvalidKey(format!(
                "Master key too short: {} bytes, minimum 32 bytes",
                key_bytes.len()
            )));
        }

        self.data_key = Some(Self::derive_key(&key_bytes, b"minikv-data")?);
        self.wal_key = Some(Self::derive_key(&key_bytes, b"minikv-wal")?);

        if let Some(key) = &self.data_key {
            self.data_cipher = Some(Aes256Gcm::new_from_slice(key).map_err(|e| {
                EncryptionError::InvalidKey(format!("Failed to create cipher: {}", e))
            })?);
        }

        if let Some(key) = &self.wal_key {
            self.wal_cipher = Some(Aes256Gcm::new_from_slice(key).map_err(|e| {
                EncryptionError::InvalidKey(format!("Failed to create WAL cipher: {}", e))
            })?);
        }

        self.config.enabled = true;
        self.config.master_key = Some(master_key.to_string());

        Ok(())
    }

    fn derive_key(master_key: &[u8], context: &[u8]) -> EncryptionResult<[u8; KEY_SIZE]> {
        use hkdf::Hkdf;

        let hkdf = Hkdf::<Sha256>::new(None, master_key);
        let mut output = [0u8; KEY_SIZE];
        hkdf.expand(context, &mut output)
            .map_err(|e| EncryptionError::KeyDerivationFailed(format!("{}", e)))?;

        Ok(output)
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.data_cipher.is_some()
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> EncryptionResult<EncryptedData> {
        let cipher = self
            .data_cipher
            .as_ref()
            .ok_or(EncryptionError::NotEnabled)?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptionFailed(format!("{}", e)))?;

        Ok(EncryptedData {
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    pub fn decrypt(&self, encrypted: &EncryptedData) -> EncryptionResult<Vec<u8>> {
        let cipher = self
            .data_cipher
            .as_ref()
            .ok_or(EncryptionError::NotEnabled)?;

        let nonce = Nonce::from_slice(&encrypted.nonce);

        cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|e| EncryptionError::DecryptionFailed(format!("{}", e)))
    }

    pub fn encrypt_bytes(&self, plaintext: &[u8]) -> EncryptionResult<Vec<u8>> {
        if !self.is_enabled() {
            return Ok(plaintext.to_vec());
        }
        let encrypted = self.encrypt(plaintext)?;
        Ok(encrypted.to_bytes())
    }

    pub fn decrypt_bytes(&self, data: &[u8]) -> EncryptionResult<Vec<u8>> {
        if !self.is_enabled() {
            return Ok(data.to_vec());
        }

        if !EncryptedData::is_encrypted(data) {
            return Ok(data.to_vec());
        }

        let encrypted = EncryptedData::from_bytes(data)?;
        self.decrypt(&encrypted)
    }

    pub fn encrypt_wal(&self, plaintext: &[u8]) -> EncryptionResult<EncryptedData> {
        let cipher = self
            .wal_cipher
            .as_ref()
            .ok_or(EncryptionError::NotEnabled)?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| EncryptionError::EncryptionFailed(format!("{}", e)))?;

        Ok(EncryptedData {
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    pub fn decrypt_wal(&self, encrypted: &EncryptedData) -> EncryptionResult<Vec<u8>> {
        let cipher = self
            .wal_cipher
            .as_ref()
            .ok_or(EncryptionError::NotEnabled)?;

        let nonce = Nonce::from_slice(&encrypted.nonce);

        cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|e| EncryptionError::DecryptionFailed(format!("{}", e)))
    }

    pub fn generate_master_key() -> String {
        let mut key = [0u8; KEY_SIZE];
        rand::thread_rng().fill_bytes(&mut key);
        BASE64.encode(key)
    }

    pub fn status(&self) -> EncryptionStatus {
        EncryptionStatus {
            enabled: self.is_enabled(),
            algorithm: if self.is_enabled() {
                Some("AES-256-GCM".to_string())
            } else {
                None
            },
            key_derivation: if self.is_enabled() {
                Some("HKDF-SHA256".to_string())
            } else {
                None
            },
        }
    }
}

impl Default for EncryptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionStatus {
    pub enabled: bool,
    pub algorithm: Option<String>,
    pub key_derivation: Option<String>,
}

/// Encrypts `data` with [`ENCRYPTION_MANAGER`] when it is enabled. When
/// encryption fails, returns `data` unencrypted, without telling.
#[deprecated(
    since = "2.0.1",
    note = "returns the data unencrypted when encryption fails; use EncryptionManager::encrypt_bytes"
)]
pub fn maybe_encrypt(data: &[u8]) -> Vec<u8> {
    let manager = ENCRYPTION_MANAGER.read().unwrap();
    manager
        .encrypt_bytes(data)
        .unwrap_or_else(|_| data.to_vec())
}

/// Decrypts `data` with [`ENCRYPTION_MANAGER`] when it is enabled. When
/// decryption fails, returns `data` still encrypted, without telling.
#[deprecated(
    since = "2.0.1",
    note = "returns the data still encrypted when decryption fails; use EncryptionManager::decrypt_bytes"
)]
pub fn maybe_decrypt(data: &[u8]) -> Vec<u8> {
    let manager = ENCRYPTION_MANAGER.read().unwrap();
    manager
        .decrypt_bytes(data)
        .unwrap_or_else(|_| data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_key() -> String {
        EncryptionManager::generate_master_key()
    }

    #[test]
    fn test_encryption_disabled_by_default() {
        let manager = EncryptionManager::new();
        assert!(!manager.is_enabled());
    }

    #[test]
    fn test_initialize_encryption() {
        let mut manager = EncryptionManager::new();
        let key = get_test_key();
        assert!(manager.initialize(&key).is_ok());
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_encrypt_decrypt() {
        let mut manager = EncryptionManager::new();
        let key = get_test_key();
        manager.initialize(&key).unwrap();

        let plaintext = b"Hello, MiniKV!";
        let encrypted = manager.encrypt(plaintext).unwrap();
        let decrypted = manager.decrypt(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_bytes() {
        let mut manager = EncryptionManager::new();
        let key = get_test_key();
        manager.initialize(&key).unwrap();

        let plaintext = b"Test data for encryption";
        let encrypted = manager.encrypt_bytes(plaintext).unwrap();
        let decrypted = manager.decrypt_bytes(&encrypted).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypted_data_format() {
        let mut manager = EncryptionManager::new();
        let key = get_test_key();
        manager.initialize(&key).unwrap();

        let plaintext = b"Format test";
        let encrypted = manager.encrypt(plaintext).unwrap();
        let bytes = encrypted.to_bytes();

        assert!(EncryptedData::is_encrypted(&bytes));

        let parsed = EncryptedData::from_bytes(&bytes).unwrap();
        assert_eq!(encrypted.nonce, parsed.nonce);
        assert_eq!(encrypted.ciphertext, parsed.ciphertext);
    }

    #[test]
    fn test_wal_encryption() {
        let mut manager = EncryptionManager::new();
        let key = get_test_key();
        manager.initialize(&key).unwrap();

        let wal_entry = b"WAL entry data";
        let encrypted = manager.encrypt_wal(wal_entry).unwrap();
        let decrypted = manager.decrypt_wal(&encrypted).unwrap();

        assert_eq!(wal_entry.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_different_keys_for_data_and_wal() {
        let mut manager = EncryptionManager::new();
        let key = get_test_key();
        manager.initialize(&key).unwrap();

        assert_ne!(manager.data_key, manager.wal_key);
    }

    #[test]
    fn test_invalid_key() {
        let mut manager = EncryptionManager::new();

        let result = manager.initialize("dG9vIHNob3J0"); // "too short" in base64
        assert!(result.is_err());

        let result = manager.initialize("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_passthrough_when_disabled() {
        let manager = EncryptionManager::new();

        let data = b"plain data";
        let result = manager.encrypt_bytes(data).unwrap();
        assert_eq!(data.as_slice(), result.as_slice());

        let result = manager.decrypt_bytes(data).unwrap();
        assert_eq!(data.as_slice(), result.as_slice());
    }

    #[test]
    fn test_generate_master_key() {
        let key1 = EncryptionManager::generate_master_key();
        let key2 = EncryptionManager::generate_master_key();

        assert_ne!(key1, key2);

        assert!(BASE64.decode(&key1).is_ok());
        assert!(BASE64.decode(&key2).is_ok());

        assert_eq!(BASE64.decode(&key1).unwrap().len(), KEY_SIZE);
    }
}
