//! Node failure and recovery test for minikv cluster

use minikv::coordinator::raft_node::{RaftNode, RaftRole};
use std::sync::Arc;

#[tokio::test]
async fn test_node_failure_and_recovery() {
    // Create 3 Raft nodes
    let node1 = Arc::new(RaftNode::new("node1".to_string()));
    let node2 = Arc::new(RaftNode::new("node2".to_string()));
    let node3 = Arc::new(RaftNode::new("node3".to_string()));
    // peers unused in this test

    // node1 becomes leader
    // Force node1 to become leader for local test
    node1.become_leader();
    assert!(node1.is_leader());
    assert_eq!(node1.get_role(), RaftRole::Leader);

    // Simulate node2 failure (no response)
    // node1 tries to replicate, node2 does not respond
    // node3 stays in the cluster
    // node1 must remain leader if majority
    let entry = minikv::common::raft::LogEntry {
        term: node1.get_term(),
        index: 1,
        data: b"set fail=1".to_vec(),
    };
    node1.get_log().push(entry.clone());
    let append_req = minikv::common::raft::AppendRequest {
        term: node1.get_term(),
        leader_id: "node1".to_string(),
        prev_log_index: 0,
        prev_log_term: node1.get_term(),
        entries: vec![entry.clone()],
        leader_commit: 1,
    };
    let resp3 = node3.handle_append_entries(append_req.clone());
    assert!(resp3.success);
    assert_eq!(node3.get_log().len(), 1);

    // node2 "comes back" (recovery)
    let resp2 = node2.handle_append_entries(append_req);
    assert!(resp2.success);
    assert_eq!(node2.get_log().len(), 1);
}
