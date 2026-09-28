//! # minikv
//!
//! A distributed key-value store with:
//! - Raft consensus between the coordinators for the metadata
//! - Two-phase commit between the leader and the volume servers for writes
//! - Placement by rendezvous hashing (HRW) on each key
//! - Write-ahead log (WAL), checksummed segments and Bloom filters on the
//!   volume servers
//! - gRPC between the nodes, HTTP (REST and S3-compatible) for clients
//!
//! ## Architecture

#![allow(clippy::result_large_err)]
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │         Coordinator Cluster             │
//! │  (Raft consensus for metadata)          │
//! │   - Leader: handles writes              │
//! │   - Followers: replicate state          │
//! └───────────┬─────────────────────────────┘
//!             │ gRPC
//!   ┌─────────┴──────────┬──────────────┐
//!   │                    │              │
//! ┌─▼──────────┐   ┌─────▼──────┐   ┌──▼───────────┐
//! │ Volume 1   │   │ Volume 2   │   │ Volume 3     │
//! │ segments   │   │ segments   │   │ segments     │
//! │  + WAL     │   │  + WAL     │   │  + WAL       │
//! └────────────┘   └────────────┘   └──────────────┘
//! ```
//!
//! Each key is stored on the `replicas` live volumes that rank highest for it:
//! the weight of a volume is BLAKE3 of the key followed by the volume id (see
//! [`common::hrw_hash`]). Reads use the replicas recorded in the key metadata.
//! There are no shards: `num_shards` in the configuration is reserved for the
//! virtual shards planned for v2.2.0.
//!
//! ## Usage
//!
//! ### Start a coordinator
//! ```bash
//! minikv-coord serve \
//!   --id coord-1 \
//!   --bind 0.0.0.0:5000 \
//!   --grpc 0.0.0.0:5001 \
//!   --db ./coord-data \
//!   --peers coord-2:5001,coord-3:5001
//! ```
//!
//! `--peers` lists the gRPC addresses of the other coordinators.
//!
//! ### Start a volume server
//! ```bash
//! minikv-volume serve \
//!   --id vol-1 \
//!   --bind 0.0.0.0:6000 \
//!   --grpc 0.0.0.0:6001 \
//!   --advertise vol-1:6001 \
//!   --data ./vol-data \
//!   --wal ./vol-wal \
//!   --coordinators http://coord-1:5000,http://coord-2:5000,http://coord-3:5000
//! ```
//!
//! `--coordinators` lists the HTTP addresses of every coordinator: the volume
//! sends its heartbeats to each of them. `--advertise` is the gRPC address
//! the coordinators use to reach the volume (by default, the `--grpc` address,
//! with 127.0.0.1 in place of 0.0.0.0).
//!
//! ### Use the CLI
//! ```bash
//! # Put a blob
//! minikv put my-key --file ./data.bin --coordinator http://localhost:5000
//!
//! # Get a blob
//! minikv get my-key --output ./out.bin
//!
//! # Delete
//! minikv delete my-key
//! ```
//!
//! The CLI uses `http://localhost:5000` when `--coordinator` is not given. The
//! cluster operations (`verify`, `repair`, `compact`, `rebalance`, `upgrade`
//! and `stream`) are not implemented yet: they exit with status 1 and name the
//! release planned for them.

pub mod common;
pub mod coordinator;
pub mod ops;
pub mod volume;

pub use common::{Config, Error, Result};
pub use coordinator::Coordinator;
pub use volume::VolumeServer;

pub mod proto {
    tonic::include_proto!("minikv");
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const BUILD_INFO: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("CARGO_PKG_NAME"), ")");
