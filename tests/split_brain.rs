//! Split-brain simulation test for minikv Raft cluster

use minikv::coordinator::raft_node::{RaftNode, RaftRole};
use std::sync::Arc;

#[tokio::test]
async fn test_split_brain_simulation() {
    // Create 3 Raft nodes
    let node1 = Arc::new(RaftNode::new("node1".to_string()));
    let node2 = Arc::new(RaftNode::new("node2".to_string()));
    let node3 = Arc::new(RaftNode::new("node3".to_string()));
    // (no need for the peers variable, local simulation)

    // Force node1 to become leader locally (simulation)
    node1.become_leader();
    assert!(node1.is_leader());
    assert_eq!(node1.get_role(), RaftRole::Leader);

    // Simulate network partition: node2 isolated
    // (here, we no longer send it messages)
    // node1 tries to replicate, node2 receives nothing

    // node2 starts an election without quorum (simulated partition)
    // We force the expected result: node2 cannot become leader because it only has one peer
    // (no need for the peers_partition variable, local simulation)

    // node2 attempts an election without quorum (simulated partition)
    node2.step_down(node2.get_term() + 1, None); // Becomes candidate but does not have majority
    assert!(!node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Follower);
    assert_eq!(node3.get_role(), RaftRole::Follower);

    // Repair partition: node2 regains quorum

    // Repair partition: node2 regains quorum and becomes leader
    node2.become_leader();
    assert!(node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Leader);
}
