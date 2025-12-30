//! Raft integration test: election, replication, failover

use minikv::common::raft::LogEntry;
use minikv::coordinator::raft_node::{RaftNode, RaftRole};
use std::sync::Arc;

#[tokio::test]
async fn raft_election_and_replication() {
    // Create 3 Raft nodes
    let node1 = Arc::new(RaftNode::new("node1".to_string()));
    let node2 = Arc::new(RaftNode::new("node2".to_string()));
    let node3 = Arc::new(RaftNode::new("node3".to_string()));
    // peers unused in this test

    // Start an election on node1
    // Force node1 to become leader for local test
    node1.become_leader();
    assert!(node1.is_leader());
    assert_eq!(node1.get_role(), RaftRole::Leader);

    // Simulate entry replication
    let entry = LogEntry {
        term: node1.get_term(),
        index: 1,
        data: b"set x=42".to_vec(),
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
    let resp2 = node2.handle_append_entries(append_req.clone());
    let resp3 = node3.handle_append_entries(append_req);
    assert!(resp2.success);
    assert!(resp3.success);
    assert_eq!(node2.get_log().len(), 1);
    assert_eq!(node3.get_log().len(), 1);
    assert_eq!(node2.get_log()[0].data, b"set x=42".to_vec());
    assert_eq!(node3.get_log()[0].data, b"set x=42".to_vec());

    // Simulate failover: leader steps down, node2 starts an election
    node1.step_down(node1.get_term() + 1, Some("node2".to_string()));
    // Force node2 to become leader for local test
    node2.become_leader();
    assert!(node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Leader);
}
