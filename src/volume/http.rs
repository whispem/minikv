//! Volume HTTP API implementation
//!
//! This module exposes the external HTTP API for volume operations.
//! Security features (TLS, authentication) and cross-datacenter replication are planned for future releases.

use crate::volume::blob::BlobStore;

pub struct Location {
    pub size: usize,
    pub blake3: [u8; 32],
}

pub fn get_location(_store: &BlobStore) -> Result<Location, String> {
    Ok(Location {
        size: 0,
        blake3: [0; 32],
    })
}
