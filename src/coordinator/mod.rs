//! Coordinator implementation with Raft consensus
//!
//! The coordinator is responsible for:
//! - Metadata management (key → replicas mapping)
//! - Placement decisions (HRW + sharding)
//! - Write orchestration (2PC with volumes)
//! - Health monitoring
//! - Consensus via Raft

pub mod grpc;
pub mod http;
pub mod metadata;
pub mod placement;
pub mod raft_node;
pub mod server;
pub mod volume_client;

pub use server::Coordinator;
