//! Audit logging module for MiniKV v0.6.0+
//!
//! Provides structured audit logs for admin and sensitive actions.
//! Logs to file and/or stdout. Integrate hooks in key management, auth, and data modification endpoints.

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

/// Audit log event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    AuthSuccess,
    AuthFailure,
    ApiKeyCreated,
    ApiKeyRevoked,
    ApiKeyDeleted,
    RoleChanged,
    DataPut,
    DataDelete,
    ConfigChanged,
    QuotaExceeded,
    System,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
    pub event: AuditEventType,
    pub actor: String,          // user/key id or system
    pub target: Option<String>, // affected resource/key
    pub message: String,
    pub meta: Option<serde_json::Value>,
}

/// Audit logger (singleton)
pub struct AuditLogger {
    file: Option<Mutex<File>>,
    to_stdout: bool,
}

pub static AUDIT_LOGGER: Lazy<AuditLogger> = Lazy::new(|| AuditLogger::new("audit.log", true));

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(path: &str, to_stdout: bool) -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(Mutex::new);
        Self { file, to_stdout }
    }

    /// Log an audit entry
    pub fn log(&self, entry: AuditEntry) {
        let line = serde_json::to_string(&entry).unwrap_or_else(|_| "{}".to_string());
        if let Some(file) = &self.file {
            if let Ok(mut f) = file.lock() {
                let _ = writeln!(f, "{}", line);
            }
        }
        if self.to_stdout {
            println!("[AUDIT] {}", line);
        }
    }

    /// Convenience for logging an event
    pub fn log_event(
        &self,
        event: AuditEventType,
        actor: impl Into<String>,
        target: Option<String>,
        message: impl Into<String>,
        meta: Option<serde_json::Value>,
    ) {
        let entry = AuditEntry {
            timestamp: Utc::now(),
            event,
            actor: actor.into(),
            target,
            message: message.into(),
            meta,
        };
        self.log(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_stdout() {
        let logger = AuditLogger::new("/dev/null", true);
        logger.log_event(
            AuditEventType::ApiKeyCreated,
            "admin",
            Some("key123".to_string()),
            "API key created",
            None,
        );
    }
}
