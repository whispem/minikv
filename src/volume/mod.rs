//! Volume server implementation
//!
//! Handles blob storage with:
//! - Write-ahead log (WAL) for durability
//! - Segmented append-only storage
//! - Automatic compaction
//! - Bloom filters for fast negative lookups
//! - Index snapshots for fast restarts

pub mod blob;
pub mod compaction;
pub mod grpc;
pub mod http;
pub mod index;
pub mod server;
pub mod wal;

pub use server::VolumeServer;
