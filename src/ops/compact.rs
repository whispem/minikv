//! Cluster-wide compaction

#![allow(dead_code)]

use crate::common::Result;

pub async fn compact_cluster(
    _coordinator_url: &str,
    _shard: Option<u64>,
) -> Result<CompactReport> {
    tracing::info!("Starting cluster compaction");

    // TODO: Implement compaction logic:
    // 1. Trigger compaction on all volumes (or specific shard)
    // 2. Wait for completion
    // 3. Report stats

    Ok(CompactReport {
        volumes_compacted: 0,
        bytes_freed: 0,
    })
}

#[derive(Debug)]
pub struct CompactReport {
    pub volumes_compacted: usize,
    pub bytes_freed: u64,
}
