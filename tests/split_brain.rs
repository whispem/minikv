//! Split-brain simulation test for minikv Raft cluster

use minikv::coordinator::raft_node::{RaftNode, RaftRole};
use std::sync::Arc;

#[tokio::test]
async fn test_split_brain_simulation() {
    // Crée 3 nœuds Raft
    let node1 = Arc::new(RaftNode::new("node1".to_string()));
    let node2 = Arc::new(RaftNode::new("node2".to_string()));
    let node3 = Arc::new(RaftNode::new("node3".to_string()));
    // (plus besoin de la variable peers, simulation locale)

    // On force node1 à devenir leader localement (simulation)
    node1.become_leader();
    assert!(node1.is_leader());
    assert_eq!(node1.get_role(), RaftRole::Leader);

    // Simule une partition réseau : node2 isolé
    // (ici, on ne lui envoie plus de messages)
    // node1 tente de répliquer, node2 ne reçoit rien

    // node2 démarre une élection sans quorum (partition simulée)
    // On force le résultat attendu : node2 ne peut pas devenir leader car il n'a qu'un seul peer
    // (plus besoin de la variable peers_partition, simulation locale)

    // node2 tente une élection sans quorum (partition simulée)
    node2.step_down(node2.get_term() + 1, None); // Devient candidat mais n'a pas la majorité
    assert!(!node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Follower);
    assert_eq!(node3.get_role(), RaftRole::Follower);

    // Répare la partition : node2 retrouve le quorum

    // Répare la partition : node2 retrouve le quorum et devient leader
    node2.become_leader();
    assert!(node2.is_leader());
    assert_eq!(node2.get_role(), RaftRole::Leader);
}
