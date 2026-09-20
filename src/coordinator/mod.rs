//! Coordinator implementation with Raft consensus
//!
//! The coordinator is responsible for:
//! - Metadata management (key -> replicas mapping)
//! - Placement decisions (HRW + sharding)
//! - Write orchestration (2PC with volumes)
//! - Health monitoring
//! - Consensus via Raft

pub mod grpc;
pub mod http;
pub mod metadata;
pub mod objects;
pub mod placement;
pub mod raft_node;
pub mod raft_rpc_client;
pub mod raft_storage;
pub mod server;
pub mod state;
pub mod volume_client;

pub use server::Coordinator;
