//! Test d'intégration Raft : élection, réplication, failover

use minikv::coordinator::raft_node::{RaftNode, RaftRole};
use minikv::common::raft::LogEntry;
use std::sync::Arc;

#[tokio::test]
async fn raft_election_and_replication() {
    // Crée 3 noeuds Raft
    let node1 = Arc::new(RaftNode::new("node1".to_string()));
    let node2 = Arc::new(RaftNode::new("node2".to_string()));
    let node3 = Arc::new(RaftNode::new("node3".to_string()));
    let peers = vec!["node2".to_string(), "node3".to_string()];

    // Démarre une élection sur node1
    let is_leader = node1.start_election_and_collect_votes(peers.clone()).await;
    assert!(is_leader);
    assert_eq!(node1.get_role(), RaftRole::Leader);

    // Simule une réplication d'entrée
    let entry = LogEntry { term: node1.get_term(), index: 1, data: b"set x=42".to_vec() };
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

    // Simule le failover : le leader step_down, node2 démarre une élection
    node1.step_down(node1.get_term()+1, Some("node2".to_string()));
    let is_leader2 = node2.start_election_and_collect_votes(vec!["node1".to_string(), "node3".to_string()]).await;
    assert!(is_leader2);
    assert_eq!(node2.get_role(), RaftRole::Leader);
}
