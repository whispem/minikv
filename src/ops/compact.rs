//! Cluster compaction and blob streaming.
//!
//! Not implemented yet: compaction is planned for 2.2.0, and no release plans
//! streaming. Until then, the functions of this module return an error and do
//! nothing.

use super::NotImplemented;
use crate::common::Result;

/// Compacts the volume segments to reclaim the space of deleted and replaced
/// blobs. `shard` is reserved: minikv does not place keys by shard yet.
///
/// Not implemented: always returns an error that names the planned release.
pub async fn compact_cluster(_coordinator_url: &str, _shard: Option<u64>) -> Result<CompactReport> {
    Err(NotImplemented::COMPACT.into())
}

/// Streams a large blob out of a volume.
///
/// Not implemented, and no release plans it: always returns an error.
pub async fn stream_large_blob(_volume_id: &str, _key: &str) -> Result<()> {
    Err(NotImplemented::STREAM.into())
}

/// Result of a cluster compaction. Nothing produces it yet.
#[derive(Debug, serde::Serialize)]
pub struct CompactReport {
    pub volumes_compacted: usize,
    pub bytes_freed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn compact_and_stream_fail_instead_of_reporting() {
        let compact = compact_cluster("http://127.0.0.1:1", Some(0)).await;
        assert_eq!(
            compact.unwrap_err().to_string(),
            "compact is not implemented (roadmap: 2.2.0)"
        );
        let stream = stream_large_blob("vol-1", "key").await;
        assert_eq!(
            stream.unwrap_err().to_string(),
            "stream is not implemented (roadmap: unscheduled)"
        );
    }
}
