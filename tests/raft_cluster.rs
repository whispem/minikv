//! Raft log replication and a change of leader, driven by direct calls.
//!
//! Nothing is elected: the test sets the leaders itself with `become_leader`
//! and `step_down`, and gives the `AppendEntries` request to the followers.
//! `tests/distributed_cluster.rs` runs real coordinators, which elect a leader
//! and replace it when it stops.

use minikv::common::raft::LogEntry;
use minikv::coordinator::raft_node::{RaftNode, RaftRole};
use std::sync::Arc;

#[tokio::test]
async fn raft_election_and_replication() {
    let node1 = Arc::new(RaftNode::new("node1".to_string()));
    let node2 = Arc::new(RaftNode::new("node2".to_string()));
    let node3 = Arc::new(RaftNode::new("node3".to_string()));

    node1.become_leader();
    assert!(node1.is_leader());
    assert_eq!(node1.get_role(), RaftRole::Leader);

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

    node1.step_down(node1.get_term() + 1, Some("node2".to_string()));
    node2.become_leader();
    assert!(node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Leader);
}
