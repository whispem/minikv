//! Replica placement.
//!
//! minikv places each key with rendezvous hashing (HRW, highest random weight)
//! on the key itself: every healthy volume gets the weight BLAKE3(key followed
//! by the volume id), and the key goes to the `replicas` volumes with the
//! highest weights (see [`hrw_hash`](crate::common::hrw_hash)). The choice
//! depends only on the key and on the set of healthy volumes, not on the order
//! of the volume list. Reads do not recompute it: they use the replicas
//! recorded in the key metadata.
//!
//! There are no shards. `num_shards` is still accepted by
//! [`PlacementManager::new`] and in the configuration, and is reserved for the
//! virtual shards planned for v2.2.0. The shard methods below are not wired
//! into placement.

#[allow(deprecated)]
use crate::common::ConsistentHashRing;
use crate::common::{select_replicas, shard_key, Result};
use crate::coordinator::metadata::VolumeMetadata;

/// Chooses the volumes that store a key, with HRW on the key.
pub struct PlacementManager {
    #[allow(deprecated)]
    ring: ConsistentHashRing,
    replicas: usize,
    num_shards: u64,
}

impl PlacementManager {
    /// `replicas` is the replication factor. `num_shards` is reserved: it does
    /// not change placement.
    #[allow(deprecated)]
    pub fn new(num_shards: u64, replicas: usize) -> Self {
        Self {
            ring: ConsistentHashRing::new(num_shards),
            replicas,
            num_shards,
        }
    }

    /// The first `replicas` healthy volumes that HRW ranks for `key`. Fails
    /// when fewer than `replicas` volumes are healthy.
    ///
    /// The coordinator does not call it: it writes with
    /// [`select_available`](Self::select_available), which accepts fewer
    /// replicas.
    pub fn select_volumes(&self, key: &str, volumes: &[VolumeMetadata]) -> Result<Vec<String>> {
        if volumes.is_empty() {
            return Err(crate::Error::NoHealthyVolumes);
        }

        let healthy: Vec<String> = volumes
            .iter()
            .filter(|v| v.state.is_healthy())
            .map(|v| v.volume_id.clone())
            .collect();

        if healthy.is_empty() {
            return Err(crate::Error::NoHealthyVolumes);
        }

        let selected = select_replicas(key, &healthy, self.replicas);

        if selected.len() < self.replicas {
            return Err(crate::Error::InsufficientReplicas {
                needed: self.replicas,
                available: selected.len(),
            });
        }

        Ok(selected)
    }

    pub fn replicas(&self) -> usize {
        self.replicas
    }

    /// The first `replicas` healthy volumes that HRW ranks for `key`, or all
    /// the healthy volumes when there are fewer. This is the placement the
    /// coordinator uses for every write.
    pub fn select_available(&self, key: &str, volumes: &[VolumeMetadata]) -> Vec<String> {
        let healthy: Vec<String> = volumes
            .iter()
            .filter(|v| v.state.is_healthy())
            .map(|v| v.volume_id.clone())
            .collect();
        select_replicas(key, &healthy, self.replicas.min(healthy.len()))
    }

    /// The shard of `key` among `num_shards`. Placement does not use it.
    #[deprecated(
        since = "2.0.1",
        note = "not wired into placement, which uses HRW on the key; \
                virtual shards are planned for v2.2.0"
    )]
    pub fn get_shard(&self, key: &str) -> u64 {
        shard_key(key, self.num_shards)
    }

    /// Fills an internal shard table that nothing reads. No data moves.
    #[deprecated(
        since = "2.0.1",
        note = "moves nothing: it fills a shard table that placement never reads; \
                rebalancing is planned for v2.2.0"
    )]
    #[allow(deprecated)]
    pub fn rebalance(&mut self, volumes: &[VolumeMetadata]) {
        let available: Vec<String> = volumes
            .iter()
            .filter(|v| v.state.is_healthy())
            .map(|v| v.volume_id.clone())
            .collect();

        self.ring.rebalance(&available, self.replicas);
    }

    /// The volumes of `shard` in the internal shard table, which only
    /// [`rebalance`](Self::rebalance) fills.
    #[deprecated(
        since = "2.0.1",
        note = "not wired into placement, which uses HRW on the key; \
                virtual shards are planned for v2.2.0"
    )]
    #[allow(deprecated)]
    pub fn get_shard_volumes(&self, shard: u64) -> Option<Vec<String>> {
        self.ring.get_shard_nodes(shard).map(|nodes| nodes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::NodeState;

    fn mock_volume(id: &str, state: NodeState) -> VolumeMetadata {
        VolumeMetadata {
            volume_id: id.to_string(),
            address: format!("http://localhost:{}", id),
            grpc_address: format!("http://localhost:{}", id),
            state,
            shards: vec![],
            total_keys: 0,
            total_bytes: 0,
            free_bytes: 0,
            last_heartbeat: 0,
        }
    }

    #[test]
    fn test_select_volumes() {
        let manager = PlacementManager::new(256, 3);

        let volumes = vec![
            mock_volume("vol-1", NodeState::Alive),
            mock_volume("vol-2", NodeState::Alive),
            mock_volume("vol-3", NodeState::Alive),
            mock_volume("vol-4", NodeState::Alive),
        ];

        let selected = manager.select_volumes("test-key", &volumes).unwrap();
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_insufficient_replicas() {
        let manager = PlacementManager::new(256, 3);

        let volumes = vec![
            mock_volume("vol-1", NodeState::Alive),
            mock_volume("vol-2", NodeState::Alive),
        ];

        let result = manager.select_volumes("test-key", &volumes);
        assert!(result.is_err());
    }

    #[test]
    fn test_select_available_degrades_to_live_volumes() {
        let manager = PlacementManager::new(256, 3);

        let volumes = vec![
            mock_volume("vol-1", NodeState::Alive),
            mock_volume("vol-2", NodeState::Dead),
            mock_volume("vol-3", NodeState::Alive),
        ];

        let selected = manager.select_available("test-key", &volumes);
        assert_eq!(selected.len(), 2);
        assert!(!selected.contains(&"vol-2".to_string()));
        assert_eq!(selected, manager.select_available("test-key", &volumes));
    }

    #[test]
    fn test_no_healthy_volumes() {
        let manager = PlacementManager::new(256, 3);

        let volumes = vec![
            mock_volume("vol-1", NodeState::Dead),
            mock_volume("vol-2", NodeState::Dead),
        ];

        let result = manager.select_volumes("test-key", &volumes);
        assert!(result.is_err());
    }

    /// HRW computed from its definition, independently of `hrw_hash`: the
    /// `n` volumes with the highest first 8 bytes (little endian) of
    /// BLAKE3(key followed by volume id).
    fn reference_hrw(key: &str, volumes: &[&str], n: usize) -> Vec<String> {
        let mut ranked: Vec<(u64, &str)> = volumes
            .iter()
            .map(|id| {
                let digest = blake3::hash(format!("{}{}", key, id).as_bytes());
                let weight = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
                (weight, *id)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        ranked
            .into_iter()
            .take(n)
            .map(|(_, id)| id.to_string())
            .collect()
    }

    #[test]
    fn placement_is_hrw_on_the_key_whatever_the_volume_order() {
        let manager = PlacementManager::new(256, 3);
        let mut volumes = vec![
            mock_volume("vol-a", NodeState::Alive),
            mock_volume("vol-b", NodeState::Alive),
            mock_volume("vol-c", NodeState::Dead),
            mock_volume("vol-d", NodeState::Alive),
            mock_volume("vol-e", NodeState::Alive),
        ];
        let healthy = ["vol-a", "vol-b", "vol-d", "vol-e"];
        let keys: Vec<String> = (0..1000).map(|i| format!("key-{}", i)).collect();
        let expected: Vec<Vec<String>> = keys
            .iter()
            .map(|key| reference_hrw(key, &healthy, 3))
            .collect();

        // The given order, then 20 shuffles from a fixed seed.
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut first_choices = std::collections::BTreeSet::new();
        for round in 0..=20 {
            if round > 0 {
                for i in (1..volumes.len()).rev() {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    volumes.swap(i, (seed % (i as u64 + 1)) as usize);
                }
            }
            for (key, expected) in keys.iter().zip(&expected) {
                let selected = manager.select_available(key, &volumes);
                assert_eq!(&selected, expected, "key {} in round {}", key, round);
                first_choices.insert(selected[0].clone());
            }
        }

        // Every healthy volume is the first choice of some keys, and the dead
        // one never is.
        assert_eq!(first_choices.into_iter().collect::<Vec<_>>(), healthy);
    }
}
