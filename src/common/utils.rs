//! Utility functions for minikv

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Percent-encoding set for keys (includes /, %, and control chars)
const KEY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b'/')
    .add(b'%')
    .add(b' ')
    .add(b'?')
    .add(b'#')
    .add(b'&');

/// Encode a key for URL/filesystem usage
pub fn encode_key(key: &str) -> String {
    utf8_percent_encode(key, KEY_ENCODE_SET).to_string()
}

/// Decode a percent-encoded key
pub fn decode_key(encoded: &str) -> crate::Result<String> {
    percent_decode_str(encoded)
        .decode_utf8()
        .map(|s| s.to_string())
        .map_err(|e| crate::Error::Other(format!("Failed to decode key: {}", e)))
}

/// Format bytes as human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}

/// Parse duration string (e.g., "30s", "5m", "1h", "7d")
pub fn parse_duration(s: &str) -> crate::Result<std::time::Duration> {
    let s = s.trim();
    if s.is_empty() {
        return Err(crate::Error::InvalidConfig("empty duration".into()));
    }

    let (num_str, unit) = if s.ends_with("ms") {
        (&s[..s.len() - 2], "ms")
    } else {
        let unit = s.chars().last().unwrap();
        (&s[..s.len() - 1], &s[s.len() - 1..])
    };

    let num: u64 = num_str
        .parse()
        .map_err(|_| crate::Error::InvalidConfig(format!("invalid duration: {}", s)))?;

    let duration = match unit {
        "ms" => std::time::Duration::from_millis(num),
        "s" => std::time::Duration::from_secs(num),
        "m" => std::time::Duration::from_secs(num * 60),
        "h" => std::time::Duration::from_secs(num * 3600),
        "d" => std::time::Duration::from_secs(num * 86400),
        _ => {
            return Err(crate::Error::InvalidConfig(format!(
                "unknown duration unit: {}",
                unit
            )))
        }
    };

    Ok(duration)
}

/// Get current Unix timestamp (seconds)
pub fn timestamp_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Get current Unix timestamp (milliseconds)
pub fn timestamp_now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Node health state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
    Alive,
    Suspect,
    Dead,
    Draining,
}

impl NodeState {
    /// Is this node healthy enough to serve requests?
    pub fn is_healthy(&self) -> bool {
        matches!(self, NodeState::Alive)
    }

    /// Can this node accept new writes?
    pub fn can_write(&self) -> bool {
        matches!(self, NodeState::Alive)
    }

    /// Can this node serve reads?
    pub fn can_read(&self) -> bool {
        matches!(self, NodeState::Alive | NodeState::Draining)
    }
}

impl std::fmt::Display for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeState::Alive => write!(f, "alive"),
            NodeState::Suspect => write!(f, "suspect"),
            NodeState::Dead => write!(f, "dead"),
            NodeState::Draining => write!(f, "draining"),
        }
    }
}

/// Retry with exponential backoff
pub async fn retry_with_backoff<F, Fut, T>(
    mut f: F,
    max_retries: usize,
    initial_delay: std::time::Duration,
) -> crate::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = crate::Result<T>>,
{
    let mut delay = initial_delay;

    for attempt in 0..max_retries {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempt < max_retries - 1 => {
                tracing::warn!(
                    "Retry attempt {} failed: {}, retrying in {:?}",
                    attempt + 1,
                    e,
                    delay
                );
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(e) => return Err(e),
        }
    }

    Err(crate::Error::Internal("Max retries exceeded".into()))
}

/// Generate a unique upload ID for 2PC
pub fn generate_upload_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = timestamp_now_millis();
    format!("{}-{}", timestamp, counter)
}

/// Calculate CRC32 checksum
pub fn crc32(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}

/// Validate key (must be non-empty, reasonable length)
pub fn validate_key(key: &str) -> crate::Result<()> {
    if key.is_empty() {
        return Err(crate::Error::InvalidConfig("key cannot be empty".into()));
    }

    if key.len() > 1024 {
        return Err(crate::Error::InvalidConfig(
            "key too long (max 1024 bytes)".into(),
        ));
    }

    if key.chars().any(|c| c.is_control()) {
        return Err(crate::Error::InvalidConfig(
            "key contains invalid characters".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_key() {
        let key = "my/path/to/file.txt";
        let encoded = encode_key(key);
        assert!(encoded.contains("%2F"));

        let decoded = decode_key(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0.00 B");
        assert_eq!(format_bytes(1023), "1023.00 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(
            parse_duration("500ms").unwrap(),
            std::time::Duration::from_millis(500)
        );
        assert_eq!(
            parse_duration("30s").unwrap(),
            std::time::Duration::from_secs(30)
        );
        assert_eq!(
            parse_duration("5m").unwrap(),
            std::time::Duration::from_secs(300)
        );
        assert_eq!(
            parse_duration("1h").unwrap(),
            std::time::Duration::from_secs(3600)
        );
        assert_eq!(
            parse_duration("7d").unwrap(),
            std::time::Duration::from_secs(604800)
        );
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("10x").is_err());
    }

    #[test]
    fn test_node_state() {
        assert!(NodeState::Alive.is_healthy());
        assert!(NodeState::Alive.can_write());
        assert!(NodeState::Alive.can_read());

        assert!(!NodeState::Dead.is_healthy());
        assert!(!NodeState::Dead.can_write());
        assert!(!NodeState::Dead.can_read());

        assert!(!NodeState::Draining.can_write());
        assert!(NodeState::Draining.can_read());
    }

    #[test]
    fn test_generate_upload_id() {
        let id1 = generate_upload_id();
        let id2 = generate_upload_id();
        assert_ne!(id1, id2);
        assert!(id1.contains('-'));
    }

    #[test]
    fn test_validate_key() {
        assert!(validate_key("normal-key").is_ok());
        assert!(validate_key("path/to/key").is_ok());
        assert!(validate_key("").is_err());
        assert!(validate_key(&"x".repeat(2000)).is_err());
    }
}
