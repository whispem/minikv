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
}

impl RaftNode {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            role: Arc::new(Mutex::new(RaftRole::Follower)),
            term: Arc::new(Mutex::new(0)),
            voted_for: Arc::new(Mutex::new(None)),
            leader_id: Arc::new(Mutex::new(None)),
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
        // For single-node testing, just become leader immediately
        node.become_leader();
        tracing::info!("Raft node {} is now leader", node.node_id);

        // In production, this would:
        // - Send heartbeats to followers
        // - Handle election timeouts
        // - Manage log replication

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    })
}
