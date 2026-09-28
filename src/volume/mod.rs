//! Volume server implementation
//!
//! Handles blob storage with:
//! - Write-ahead log (WAL) for durability
//! - Segmented append-only storage
//! - Bloom filters for fast negative lookups
//! - An index rebuilt from the segments and the WAL at every start
//!
//! There is no compaction: the segments and the WAL only grow.

pub mod blob;
pub mod compaction;
pub mod grpc;
pub mod http;
pub mod index;
pub mod server;
pub mod wal;

pub use server::VolumeServer;
