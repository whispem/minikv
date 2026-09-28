//! Structured audit logging for admin and sensitive actions.

use chrono::{DateTime, Utc};
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

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

pub struct AuditLogger {
    file: Option<Mutex<File>>,
    to_stdout: bool,
}

/// The file [`AUDIT_LOGGER`] appends to, when set before its first use.
static LOG_PATH: OnceCell<PathBuf> = OnceCell::new();

/// Writes to `audit.log` in the working directory, unless
/// [`set_audit_log_path`] chose another file first. Every entry also goes to
/// standard output.
pub static AUDIT_LOGGER: Lazy<AuditLogger> = Lazy::new(|| {
    let path = LOG_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("audit.log"));
    AuditLogger::new(&path.to_string_lossy(), true)
});

/// Chooses the file of [`AUDIT_LOGGER`]. The coordinator uses `audit.log` in
/// its data directory. Returns `false`, and changes nothing, when a path was
/// already chosen or when the logger has already written.
pub fn set_audit_log_path(path: impl Into<PathBuf>) -> bool {
    Lazy::get(&AUDIT_LOGGER).is_none() && LOG_PATH.set(path.into()).is_ok()
}

impl AuditLogger {
    pub fn new(path: &str, to_stdout: bool) -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(Mutex::new);
        Self { file, to_stdout }
    }

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
