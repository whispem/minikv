//! Cluster verification.
//!
//! Not implemented yet: a read-only verification is planned for 2.1.0. Until
//! then, [`verify_cluster`] returns an error and reports nothing.

use super::NotImplemented;
use crate::common::Result;

/// Checks that every replica of every key holds its blob and, when `deep` is
/// set, that the blob's BLAKE3 hash matches the key metadata.
///
/// Not implemented: always returns an error that names the planned release.
pub async fn verify_cluster(
    _coordinator_url: &str,
    _deep: bool,
    _concurrency: usize,
) -> Result<VerifyReport> {
    Err(NotImplemented::VERIFY.into())
}

/// Prepares a rolling upgrade of the cluster.
///
/// Not implemented, and no release plans it: always returns an error.
pub async fn prepare_seamless_upgrade(_coordinator_url: &str) -> Result<()> {
    Err(NotImplemented::UPGRADE.into())
}

/// Result of a cluster verification. Nothing produces it yet.
#[derive(Debug, serde::Serialize)]
pub struct VerifyReport {
    pub total_keys: usize,
    pub healthy: usize,
    pub under_replicated: usize,
    pub corrupted: usize,
    pub orphaned: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verify_and_upgrade_fail_instead_of_reporting() {
        let verify = verify_cluster("http://127.0.0.1:1", true, 4).await;
        assert_eq!(
            verify.unwrap_err().to_string(),
            "verify is not implemented (roadmap: 2.1.0)"
        );
        let upgrade = prepare_seamless_upgrade("http://127.0.0.1:1").await;
        assert_eq!(
            upgrade.unwrap_err().to_string(),
            "upgrade is not implemented (roadmap: unscheduled)"
        );
    }
}
