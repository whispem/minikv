//! Role changes of Raft nodes, driven by direct calls.
//!
//! Despite its name, this test simulates no split brain: the three nodes
//! share no network, and the test sets their roles itself with `become_leader`
//! and `step_down`. `tests/distributed_cluster.rs` runs real coordinators,
//! which elect a leader and replace it when it stops.

use minikv::coordinator::raft_node::{RaftNode, RaftRole};
use std::sync::Arc;

#[tokio::test]
async fn test_split_brain_simulation() {
    let node1 = Arc::new(RaftNode::new("node1".to_string()));
    let node2 = Arc::new(RaftNode::new("node2".to_string()));
    let node3 = Arc::new(RaftNode::new("node3".to_string()));

    node1.become_leader();
    assert!(node1.is_leader());
    assert_eq!(node1.get_role(), RaftRole::Leader);

    // Moves node2 to a newer term, as a follower: no election takes place.
    node2.step_down(node2.get_term() + 1, None);
    assert!(!node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Follower);
    assert_eq!(node3.get_role(), RaftRole::Follower);

    node2.become_leader();
    assert!(node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Leader);
}
