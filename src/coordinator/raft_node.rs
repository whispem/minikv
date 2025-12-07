//! Raft consensus node (simplified wrapper)
//!
//! This is a minimal Raft implementation wrapper.
//! For production, use a full Raft library like tikv/raft.

use crate::common::Result;
use std::sync::{Arc, Mutex};

/// Simplified Raft state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaftRole {
    Follower,
    Candidate,
    Leader,
}

impl std::fmt::Display for RaftRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RaftRole::Follower => write!(f, "follower"),
            RaftRole::Candidate => write!(f, "candidate"),
            RaftRole::Leader => write!(f, "leader"),
        }
    }
}

/// Raft node state
pub struct RaftNode {
    node_id: String,
    role: Arc<Mutex<RaftRole>>,
    term: Arc<Mutex<u64>>,
    voted_for: Arc<Mutex<Option<String>>>,
    leader_id: Arc<Mutex<Option<String>>>,
    log: Arc<Mutex<Vec<crate::common::raft::LogEntry>>>,
}

impl RaftNode {
    /// Accès public au log pour les tests
    pub fn get_log(&self) -> std::sync::MutexGuard<'_, Vec<crate::common::raft::LogEntry>> {
        self.log.lock().unwrap()
    }
    /// Envoi des heartbeats (AppendEntries vides) aux followers
    pub async fn send_heartbeats(&self) {
        // TODO: envoyer AppendEntries (heartbeat) à tous les peers via gRPC
    }
    /// Traiter une requête RequestVote reçue
    pub fn handle_request_vote(
        &self,
        req: crate::common::raft::VoteRequest,
    ) -> crate::common::raft::VoteResponse {
        let mut term = self.term.lock().unwrap();
        let mut voted_for = self.voted_for.lock().unwrap();
        let current_term = *term;
        let mut vote_granted = false;

        if req.term < current_term {
            vote_granted = false;
        } else {
            if req.term > current_term {
                *term = req.term;
                *voted_for = None;
            }
            if voted_for.is_none() || voted_for.as_ref() == Some(&req.candidate_id) {
                *voted_for = Some(req.candidate_id.clone());
                vote_granted = true;
            }
        }
        crate::common::raft::VoteResponse {
            term: *term,
            vote_granted,
        }
    }

    /// Traiter une requête AppendEntries reçue
    pub fn handle_append_entries(
        &self,
        req: crate::common::raft::AppendRequest,
    ) -> crate::common::raft::AppendResponse {
        let mut term = self.term.lock().unwrap();
        let current_term = *term;
        let mut log = self.log.lock().unwrap();
        let mut conflict_index = 0;
        let success = if req.term < current_term {
            false
        } else {
            if req.term > current_term {
                *term = req.term;
            }
            if req.prev_log_index as usize <= log.len() {
                for entry in req.entries {
                    log.push(entry);
                }
                true
            } else {
                conflict_index = log.len() as u64;
                false
            }
        };
        crate::common::raft::AppendResponse {
            term: *term,
            success,
            conflict_index,
        }
    }

    /// Démarrer une élection et collecter les votes
    pub async fn start_election_and_collect_votes(&self, peers: Vec<String>) -> bool {
        let new_term = self.start_election();
        let mut votes = 1; // On vote pour soi-même
                           // TODO: envoyer RequestVote à tous les peers via gRPC
                           // Simuler la majorité pour l'exemple
        if !peers.is_empty() {
            votes += peers.len();
        }
        let majority = peers.len().div_ceil(2);
        if votes >= majority {
            self.become_leader();
            true
        } else {
            self.step_down(new_term, None);
            false
        }
    }
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            role: Arc::new(Mutex::new(RaftRole::Follower)),
            term: Arc::new(Mutex::new(0)),
            voted_for: Arc::new(Mutex::new(None)),
            leader_id: Arc::new(Mutex::new(None)),
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn is_leader(&self) -> bool {
        matches!(*self.role.lock().unwrap(), RaftRole::Leader)
    }

    pub fn get_role(&self) -> RaftRole {
        *self.role.lock().unwrap()
    }

    pub fn get_leader(&self) -> Option<String> {
        self.leader_id.lock().unwrap().clone()
    }

    pub fn get_term(&self) -> u64 {
        *self.term.lock().unwrap()
    }

    /// Become leader (for single-node testing)
    pub fn become_leader(&self) {
        *self.role.lock().unwrap() = RaftRole::Leader;
        *self.leader_id.lock().unwrap() = Some(self.node_id.clone());
    }

    /// Step down to follower
    pub fn step_down(&self, new_term: u64, leader_id: Option<String>) {
        *self.role.lock().unwrap() = RaftRole::Follower;
        *self.term.lock().unwrap() = new_term;
        *self.leader_id.lock().unwrap() = leader_id;
        *self.voted_for.lock().unwrap() = None;
    }

    /// Start election
    pub fn start_election(&self) -> u64 {
        let mut term = self.term.lock().unwrap();
        *term += 1;
        let new_term = *term;

        *self.role.lock().unwrap() = RaftRole::Candidate;
        *self.voted_for.lock().unwrap() = Some(self.node_id.clone());
        *self.leader_id.lock().unwrap() = None;

        new_term
    }

    /// Grant vote
    pub fn grant_vote(&self, term: u64, candidate_id: String) -> bool {
        let mut current_term = self.term.lock().unwrap();
        let mut voted = self.voted_for.lock().unwrap();

        if term < *current_term {
            return false;
        }

        if term > *current_term {
            *current_term = term;
            *voted = None;
        }

        if voted.is_none() || voted.as_ref() == Some(&candidate_id) {
            *voted = Some(candidate_id);
            true
        } else {
            false
        }
    }

    /// Replicate entry (simplified)
    pub fn replicate(&self, _entry: Vec<u8>) -> Result<()> {
        if !self.is_leader() {
            return Err(crate::Error::NotLeader(
                self.get_leader().unwrap_or_else(|| "unknown".to_string()),
            ));
        }

        // In a real implementation, this would:
        // 1. Append entry to local log
        // 2. Send AppendEntries RPCs to followers
        // 3. Wait for majority acknowledgment
        // 4. Apply to state machine
        // 5. Update commit index

        Ok(())
    }
}

/// Start Raft background tasks (heartbeats, elections)
pub fn start_raft_tasks(node: Arc<RaftNode>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Raft main loop: handle timeouts, elections, heartbeats
        let mut last_heartbeat = tokio::time::Instant::now();
        let mut election_timeout =
            tokio::time::Duration::from_millis(150 + rand::random::<u64>() % 150);

        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            // If follower and no heartbeat received, start election
            if !node.is_leader() && last_heartbeat.elapsed() > election_timeout {
                tracing::info!("Node {} starting election", node.node_id);
                node.start_election();
                // TODO: Send RequestVote RPCs to other nodes
                // TODO: Collect votes and become leader if majority
                // TODO: Reset election timeout
                election_timeout =
                    tokio::time::Duration::from_millis(150 + rand::random::<u64>() % 150);
                last_heartbeat = tokio::time::Instant::now();
            }

            // If leader, send heartbeats
            if node.is_leader() {
                // TODO: Send AppendEntries (heartbeat) RPCs to followers
                last_heartbeat = tokio::time::Instant::now();
            }
        }
    })
}
