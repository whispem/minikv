//! Placement strategy using HRW hashing and sharding
//!
//! This module implements horizontal scaling via sharding and flexible replica sets.
//! Keys are assigned to shards using HRW (Highest Random Weight) hashing, and replicas are selected for fault tolerance.

use crate::common::{select_replicas, shard_key, ConsistentHashRing, Result};
use crate::coordinator::metadata::VolumeMetadata;

/// PlacementManager handles sharding and replica selection for distributed writes.
pub struct PlacementManager {
    /// Consistent hash ring for shard assignment
    ring: ConsistentHashRing,
    /// Number of replicas per key
    replicas: usize,
    /// Total number of shards in the cluster
    num_shards: u64,
}

impl PlacementManager {
    pub fn new(num_shards: u64, replicas: usize) -> Self {
        Self {
            ring: ConsistentHashRing::new(num_shards),
            replicas,
            num_shards,
        }
    }

    /// Select volumes for a key.
    /// Uses HRW hashing to assign the key to a shard and select healthy replicas.
    pub fn select_volumes(&self, key: &str, volumes: &[VolumeMetadata]) -> Result<Vec<String>> {
        if volumes.is_empty() {
            return Err(crate::Error::NoHealthyVolumes);
        }

        // Filter healthy volumes
        let healthy: Vec<String> = volumes
            .iter()
            .filter(|v| v.state.is_healthy())
            .map(|v| v.volume_id.clone())
            .collect();

        if healthy.is_empty() {
            return Err(crate::Error::NoHealthyVolumes);
        }

        // Use HRW to select replicas
        let selected = select_replicas(key, &healthy, self.replicas);

        if selected.len() < self.replicas {
            return Err(crate::Error::InsufficientReplicas {
                needed: self.replicas,
                available: selected.len(),
            });
        }

        Ok(selected)
    }

    /// Get shard for key
    pub fn get_shard(&self, key: &str) -> u64 {
        shard_key(key, self.num_shards)
    }

    /// Rebalance shards across volumes
    pub fn rebalance(&mut self, volumes: &[VolumeMetadata]) {
        let available: Vec<String> = volumes
            .iter()
            .filter(|v| v.state.is_healthy())
            .map(|v| v.volume_id.clone())
            .collect();

        self.ring.rebalance(&available, self.replicas);
    }

    /// Get volumes for a specific shard
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
    fn test_no_healthy_volumes() {
        let manager = PlacementManager::new(256, 3);

        let volumes = vec![
            mock_volume("vol-1", NodeState::Dead),
            mock_volume("vol-2", NodeState::Dead),
        ];

        let result = manager.select_volumes("test-key", &volumes);
        assert!(result.is_err());
    }
}
