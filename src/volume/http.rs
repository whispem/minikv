//! Unused. The volume server's HTTP API is a single route, `/health`, defined
//! in [`crate::volume::server`]. This module only keeps a deprecated function
//! for compatibility.

use crate::volume::blob::BlobStore;

/// What [`get_location`] was meant to return.
#[deprecated(since = "2.0.1", note = "only used by the deprecated `get_location`")]
pub struct Location {
    pub size: usize,
    pub blake3: [u8; 32],
}

/// Always fails. It takes no key, so it cannot locate anything; it used to
/// answer size 0 and a zero BLAKE3 for any call.
#[deprecated(
    since = "2.0.1",
    note = "never implemented: it takes no key; use `BlobStore::get`"
)]
#[allow(deprecated)]
pub fn get_location(_store: &BlobStore) -> Result<Location, String> {
    Err("get_location is not implemented: it takes no key".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::WalSyncPolicy;

    #[test]
    #[allow(deprecated)]
    fn get_location_fails_instead_of_inventing_a_location() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(
            &dir.path().join("data"),
            &dir.path().join("wal"),
            WalSyncPolicy::Always,
        )
        .unwrap();
        assert!(get_location(&store).is_err());
    }
}
