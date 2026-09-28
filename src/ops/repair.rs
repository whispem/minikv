//! Cluster repair and rebalancing.
//!
//! Not implemented yet: both are planned for 2.2.0. Until then, the functions
//! of this module return an error and change nothing.

use super::NotImplemented;
use crate::common::Result;

/// Copies the blobs of under-replicated keys to more volumes until each key
/// has `replicas` copies. With `dry_run`, only reports what it would copy.
///
/// Not implemented: always returns an error that names the planned release.
pub async fn repair_cluster(
    _coordinator_url: &str,
    _replicas: usize,
    _dry_run: bool,
) -> Result<RepairReport> {
    Err(NotImplemented::REPAIR.into())
}

/// Moves blobs from the fullest volumes to the emptiest ones.
///
/// Not implemented: always returns an error that names the planned release.
pub async fn auto_rebalance_cluster(_coordinator_url: &str) -> Result<()> {
    Err(NotImplemented::REBALANCE.into())
}

/// Result of a cluster repair. Nothing produces it yet.
#[derive(Debug, serde::Serialize)]
pub struct RepairReport {
    pub keys_checked: usize,
    pub keys_repaired: usize,
    pub bytes_copied: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn repair_and_rebalance_fail_instead_of_reporting() {
        let repair = repair_cluster("http://127.0.0.1:1", 3, true).await;
        assert_eq!(
            repair.unwrap_err().to_string(),
            "repair is not implemented (roadmap: 2.2.0)"
        );
        let rebalance = auto_rebalance_cluster("http://127.0.0.1:1").await;
        assert_eq!(
            rebalance.unwrap_err().to_string(),
            "rebalance is not implemented (roadmap: 2.2.0)"
        );
    }
}
