//! Error types for minikv

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    // === I/O Errors ===
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // === Storage Errors ===
    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Corrupted data: {0}")]
    Corrupted(String),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("WAL error: {0}")]
    Wal(String),

    // === Raft Errors ===
    #[error("Not leader: current leader is {0}")]
    NotLeader(String),

    #[error("Raft error: {0}")]
    Raft(String),

    #[error("Consensus timeout")]
    ConsensusTimeout,

    // === 2PC Errors ===
    #[error("Prepare failed on {node}: {reason}")]
    PrepareFailed { node: String, reason: String },

    #[error("Commit failed on {node}: {reason}")]
    CommitFailed { node: String, reason: String },

    // === Placement Errors ===
    #[error("No healthy volumes available")]
    NoHealthyVolumes,

    #[error("Insufficient replicas: need {needed}, have {available}")]
    InsufficientReplicas { needed: usize, available: usize },

    #[error("Shard not found: {0}")]
    ShardNotFound(u64),

    // === Network Errors ===
    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    // === Metadata Errors ===
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("Metadata corrupted: {0}")]
    MetadataCorrupted(String),

    // === Config Errors ===
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    // === Ops Errors ===
    #[error("Verify failed: {0}")]
    VerifyFailed(String),

    #[error("Repair failed: {0}")]
    RepairFailed(String),

    #[error("Compact failed: {0}")]
    CompactFailed(String),

    // === Generic ===
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Operation timeout: {0}")]
    Timeout(String),

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Is this a retryable error?
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::Timeout(_)
                | Error::ConnectionFailed(_)
                | Error::ConsensusTimeout
                | Error::NotLeader(_)
                | Error::NoHealthyVolumes
        )
    }

    /// Convert to gRPC status for RPC responses
    pub fn to_grpc_status(&self) -> tonic::Status {
        use tonic::Code;
        match self {
            Error::NotFound(_) => tonic::Status::new(Code::NotFound, self.to_string()),
            Error::NotLeader(leader) => {
                let mut status = tonic::Status::new(Code::FailedPrecondition, self.to_string());
                status
                    .metadata_mut()
                    .insert("leader", leader.parse().unwrap());
                status
            }
            Error::InvalidConfig(_) | Error::InsufficientReplicas { .. } => {
                tonic::Status::new(Code::InvalidArgument, self.to_string())
            }
            Error::ConsensusTimeout | Error::Timeout(_) => {
                tonic::Status::new(Code::DeadlineExceeded, self.to_string())
            }
            Error::Grpc(status) => status.clone(),
            _ => tonic::Status::new(Code::Internal, self.to_string()),
        }
    }

    /// Convert to HTTP status code
    pub fn to_http_status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode;
        match self {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::NotLeader(_) => StatusCode::TEMPORARY_REDIRECT,
            Error::InvalidConfig(_) => StatusCode::BAD_REQUEST,
            Error::Timeout(_) | Error::ConsensusTimeout => StatusCode::REQUEST_TIMEOUT,
            Error::NoHealthyVolumes | Error::InsufficientReplicas { .. } => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

// Implement From for common error types
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Other(s.to_string())
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}
