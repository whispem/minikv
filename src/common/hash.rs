//! Hashing utilities: BLAKE3 checksums, and the rendezvous hashing (HRW) that
//! decides where each key is stored.
//!
//! minikv has no shards: placement ranks the live volumes for each key with
//! [`hrw_hash`]. [`shard_key`] and [`ConsistentHashRing`] are not wired into
//! placement; the virtual shards they were written for are planned for v2.2.0.

use blake3::Hasher;
use std::collections::HashMap;

/// BLAKE3 digest of `data`, as 64 lowercase hexadecimal characters.
pub fn blake3_hash(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    format!("{}", hash)
}

/// Incremental BLAKE3, for data that arrives in pieces.
pub struct Blake3Hasher {
    hasher: Hasher,
}

impl Blake3Hasher {
    pub fn new() -> Self {
        Self {
            hasher: Hasher::new(),
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    pub fn finalize(&self) -> String {
        let hash = self.hasher.finalize();
        format!("{}", hash)
    }
}

impl Default for Blake3Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps `key` to one of `num_shards` shards: the first 8 bytes of BLAKE3(key),
/// read as a little-endian integer, modulo `num_shards`.
///
/// Placement does not use it: keys are placed with [`hrw_hash`] on the key
/// itself. It is kept for the virtual shards planned for v2.2.0.
pub fn shard_key(key: &str, num_shards: u64) -> u64 {
    let hash = blake3::hash(key.as_bytes());
    let hash_u64 = u64::from_le_bytes(hash.as_bytes()[0..8].try_into().unwrap());
    hash_u64 % num_shards
}

/// Ranks `nodes` for `key` with rendezvous hashing (HRW, highest random
/// weight), from the first choice to the last.
///
/// The weight of a node is the first 8 bytes, read as a little-endian integer,
/// of BLAKE3 of `key` followed by the node id. The ranking depends only on the
/// key and on the set of nodes, not on their order in `nodes`: equal weights,
/// which would take a 64-bit collision, are ordered by node id. Adding or
/// removing a node changes the first `n` choices only for the keys that rank
/// this node among their first `n`.
pub fn hrw_hash(key: &str, nodes: &[String]) -> Vec<String> {
    let mut weights: Vec<(String, u64)> = nodes
        .iter()
        .map(|node| {
            let combined = format!("{}{}", key, node);
            let hash = blake3::hash(combined.as_bytes());
            let weight = u64::from_le_bytes(hash.as_bytes()[0..8].try_into().unwrap());
            (node.clone(), weight)
        })
        .collect();

    // Highest weight first. Equal weights by node id, so that the order of
    // `nodes` never matters.
    weights.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    weights.into_iter().map(|(node, _)| node).collect()
}

/// The first `n` nodes that [`hrw_hash`] ranks for `key`. With the live
/// volumes and the replication factor, these are the volumes that store the
/// key.
pub fn select_replicas(key: &str, nodes: &[String], n: usize) -> Vec<String> {
    let sorted = hrw_hash(key, nodes);
    sorted.into_iter().take(n).collect()
}

/// Two hexadecimal directory names derived from BLAKE3(key).
#[deprecated(
    since = "2.0.1",
    note = "never used: volumes store blobs in segment files, not in per-key directories"
)]
pub fn blob_prefix(key: &str) -> (String, String) {
    let hash = blake3::hash(key.as_bytes());
    let bytes = hash.as_bytes();
    (format!("{:02x}", bytes[0]), format!("{:02x}", bytes[1]))
}

/// A table from shard number to nodes, filled by
/// [`assign_shard`](Self::assign_shard) or [`rebalance`](Self::rebalance).
///
/// Despite its name, it is a plain map, not a hash ring. Nothing in minikv
/// fills or reads it: placement uses [`hrw_hash`] on the key.
#[deprecated(
    since = "2.0.1",
    note = "not wired into placement, which uses HRW on the key (see `select_replicas`); \
            virtual shards are planned for v2.2.0"
)]
pub struct ConsistentHashRing {
    pub num_shards: u64,
    shard_to_nodes: HashMap<u64, Vec<String>>,
}

#[allow(deprecated)]
impl ConsistentHashRing {
    pub fn new(num_shards: u64) -> Self {
        Self {
            num_shards,
            shard_to_nodes: HashMap::new(),
        }
    }

    pub fn assign_shard(&mut self, shard: u64, nodes: Vec<String>) {
        self.shard_to_nodes.insert(shard, nodes);
    }

    /// The nodes assigned to the shard of `key`, as computed by [`shard_key`].
    pub fn get_nodes(&self, key: &str) -> Option<&[String]> {
        let shard = shard_key(key, self.num_shards);
        self.shard_to_nodes.get(&shard).map(|v| v.as_slice())
    }

    pub fn get_shard_nodes(&self, shard: u64) -> Option<&[String]> {
        self.shard_to_nodes.get(&shard).map(|v| v.as_slice())
    }

    /// Assigns every shard to the first `replicas` nodes that [`hrw_hash`]
    /// ranks for the name `shard-N`. Nothing moves: this only fills the table.
    pub fn rebalance(&mut self, available_nodes: &[String], replicas: usize) {
        for shard in 0..self.num_shards {
            let shard_key = format!("shard-{}", shard);
            let nodes = select_replicas(&shard_key, available_nodes, replicas);
            self.shard_to_nodes.insert(shard, nodes);
        }
    }

    pub fn shards_for_node(&self, node: &str) -> Vec<u64> {
        self.shard_to_nodes
            .iter()
            .filter_map(|(shard, nodes)| {
                if nodes.contains(&node.to_string()) {
                    Some(*shard)
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blake3_hash() {
        let data = b"hello world";
        let hash = blake3_hash(data);
        assert_eq!(hash.len(), 64); // BLAKE3 produces 32 bytes = 64 hex chars
    }

    #[test]
    fn test_shard_key_deterministic() {
        let key = "test-key";
        let shard1 = shard_key(key, 256);
        let shard2 = shard_key(key, 256);
        assert_eq!(shard1, shard2);
    }

    #[test]
    fn test_hrw_hash_consistent() {
        let key = "my-key";
        let nodes = vec![
            "node1".to_string(),
            "node2".to_string(),
            "node3".to_string(),
        ];

        let sorted1 = hrw_hash(key, &nodes);
        let sorted2 = hrw_hash(key, &nodes);

        assert_eq!(sorted1, sorted2);
        assert_eq!(sorted1.len(), 3);
    }

    #[test]
    fn test_hrw_hash_different_keys() {
        let nodes = vec![
            "node1".to_string(),
            "node2".to_string(),
            "node3".to_string(),
        ];

        let sorted1 = hrw_hash("key1", &nodes);
        let sorted2 = hrw_hash("key2", &nodes);

        assert_ne!(sorted1, sorted2);
    }

    #[test]
    fn test_select_replicas() {
        let key = "test-key";
        let nodes = vec![
            "node1".to_string(),
            "node2".to_string(),
            "node3".to_string(),
            "node4".to_string(),
        ];

        let replicas = select_replicas(key, &nodes, 2);
        assert_eq!(replicas.len(), 2);
    }

    #[test]
    #[allow(deprecated)]
    fn test_blob_prefix() {
        let key = "my-blob-key";
        let (aa, bb) = blob_prefix(key);
        assert_eq!(aa.len(), 2);
        assert_eq!(bb.len(), 2);
    }

    #[test]
    #[allow(deprecated)]
    fn test_consistent_hash_ring() {
        let mut ring = ConsistentHashRing::new(256);
        let nodes = vec!["node1".to_string(), "node2".to_string()];

        ring.assign_shard(0, nodes.clone());
        ring.assign_shard(1, nodes.clone());

        assert_eq!(ring.get_shard_nodes(0), Some(nodes.as_slice()));
        let mut found_key = None;
        for i in 0..10000 {
            let candidate = format!("key-{}", i);
            if shard_key(&candidate, 256) == 0 {
                found_key = Some(candidate);
                break;
            }
        }
        let key = found_key.expect("No key found for shard 0");
        assert_eq!(ring.get_nodes(&key), Some(nodes.as_slice()));
    }

    #[test]
    #[allow(deprecated)]
    fn test_rebalance() {
        let mut ring = ConsistentHashRing::new(4);
        let nodes = vec![
            "node1".to_string(),
            "node2".to_string(),
            "node3".to_string(),
        ];

        ring.rebalance(&nodes, 2);

        for shard in 0..4 {
            let assigned = ring.get_shard_nodes(shard).unwrap();
            assert_eq!(assigned.len(), 2);
        }
    }
}
