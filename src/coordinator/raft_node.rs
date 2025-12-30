//! Raft consensus node (simplified wrapper)
//!
//! This is a minimal Raft implementation wrapper.
//! Only basic leader/follower roles are currently supported.
//! Full multi-node Raft (log replication, elections, etc.) is still in progress.
//! For production, use a full Raft library like tikv/raft.

use crate::common::Result;
use crate::coordinator::raft_rpc_client::{send_append_entries_rpc, send_request_vote_rpc};
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
    peers: Arc<Mutex<Vec<String>>>, // List of peer node IDs
    commit_index: Arc<Mutex<u64>>,
    last_applied: Arc<Mutex<u64>>,
    snapshot: Arc<Mutex<Option<Vec<u8>>>>, // Optionally store snapshot bytes
}

impl RaftNode {
    /// Get a copy of the current peer list
    pub fn get_peers(&self) -> Vec<String> {
        self.peers.lock().unwrap().clone()
    }
    /// Detects a network partition (no heartbeat received)
    pub fn detect_partition(
        &self,
        last_heartbeat: tokio::time::Instant,
        timeout: tokio::time::Duration,
    ) -> bool {
        last_heartbeat.elapsed() > timeout
    }

    /// Recovery: reloads the snapshot and log after a crash or partition
    pub fn recover(&self) {
        if self.load_snapshot().is_some() {
            // Apply the snapshot to the real state machine here
        }
        let log = self.log.lock().unwrap().clone();
        if !log.is_empty() {
            // Apply each entry to the real state machine here
        }
        let mut applied = self.last_applied.lock().unwrap();
        *applied = *self.commit_index.lock().unwrap();
    }
    /// Save a snapshot of the current state (log, index, etc.)
    pub fn save_snapshot(&self, data: Vec<u8>) {
        let mut snap = self.snapshot.lock().unwrap();
        *snap = Some(data);
    }

    /// Load a snapshot into the state machine
    pub fn load_snapshot(&self) -> Option<Vec<u8>> {
        self.snapshot.lock().unwrap().clone()
    }

    /// Apply a snapshot received from the leader (InstallSnapshot RPC)
    pub fn apply_snapshot(&self, data: Vec<u8>, last_included_index: u64, last_included_term: u64) {
        self.save_snapshot(data);
        let mut log = self.log.lock().unwrap();
        log.retain(|entry| entry.index > last_included_index);
        let mut commit = self.commit_index.lock().unwrap();
        *commit = last_included_index;
        let mut applied = self.last_applied.lock().unwrap();
        *applied = last_included_index;
        let mut term = self.term.lock().unwrap();
        *term = last_included_term;
    }
    pub fn get_log(&self) -> std::sync::MutexGuard<'_, Vec<crate::common::raft::LogEntry>> {
        self.log.lock().unwrap()
    }

    /// Periodically send heartbeats (AppendEntries RPC) to all followers.
    /// This maintains leadership and triggers log replication.
    /// Send heartbeats (AppendEntries RPC) to all followers.
    /// This maintains leadership and triggers log replication.
    pub async fn send_heartbeats(&self) {
        let peers = self.peers.lock().unwrap().clone();
        let term = self.get_term();
        let leader_id = self.node_id.clone();
        let log_snapshot = self.log.lock().unwrap().clone();
        let prev_log_index = log_snapshot.last().map(|e| e.index).unwrap_or(0);
        let prev_log_term = log_snapshot.last().map(|e| e.term).unwrap_or(0);
        let leader_commit = prev_log_index;
        for peer in &peers {
            let req = crate::common::raft::AppendRequest {
                term,
                leader_id: leader_id.clone(),
                prev_log_index,
                prev_log_term,
                entries: vec![], // Heartbeat: no entries
                leader_commit,
            };
            let _ = send_append_entries_rpc(peer, req).await;
        }
    }

    /// Start the election timer and trigger elections if no heartbeat is received.
    /// This is a stub for multi-node Raft; should run in a background task.
    /// Election timer: triggers election if no heartbeat received.
    pub async fn run_election_timer(&self) {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        loop {
            let timeout = rng.gen_range(150..300);
            tokio::time::sleep(tokio::time::Duration::from_millis(timeout)).await;
            if !self.is_leader() {
                // If no heartbeat, start election
                let peers = {
                    let peers_guard = self.peers.lock().unwrap();
                    peers_guard.clone()
                };
                self.start_election_and_collect_votes(peers).await;
            }
        }
    }

    /// Handle incoming RequestVote RPC from other nodes.
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

    /// Handle incoming AppendEntries RPC from leader.
    /// Used for log replication and heartbeat.
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
                // Update the commit index
                let mut commit = self.commit_index.lock().unwrap();
                if req.leader_commit > *commit {
                    *commit =
                        std::cmp::min(req.leader_commit, log.last().map(|e| e.index).unwrap_or(0));
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

    pub async fn start_election_and_collect_votes(&self, peers: Vec<String>) -> bool {
        let new_term = self.start_election();
        let log_snapshot = self.log.lock().unwrap().clone();
        let last_log_index = log_snapshot.last().map(|e| e.index).unwrap_or(0);
        let last_log_term = log_snapshot.last().map(|e| e.term).unwrap_or(0);
        let mut votes = 1; // Vote for self
        for peer in &peers {
            let req = crate::common::raft::VoteRequest {
                term: new_term,
                candidate_id: self.node_id.clone(),
                last_log_index,
                last_log_term,
            };
            if let Ok(resp) = send_request_vote_rpc(peer, req).await {
                if resp.vote_granted {
                    votes += 1;
                }
            }
        }
        let majority = (peers.len() + 1).div_ceil(2);
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
            peers: Arc::new(Mutex::new(Vec::new())),
            commit_index: Arc::new(Mutex::new(0)),
            last_applied: Arc::new(Mutex::new(0)),
            snapshot: Arc::new(Mutex::new(None)),
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
    pub async fn replicate(&self, _entry: Vec<u8>) -> Result<()> {
        if !self.is_leader() {
            return Err(crate::Error::NotLeader(
                self.get_leader().unwrap_or_else(|| "unknown".to_string()),
            ));
        }
        // 1. Append entry to local log
        let index;
        let term;
        let entry;
        {
            let mut log = self.log.lock().unwrap();
            index = log.last().map(|e| e.index + 1).unwrap_or(1);
            term = self.get_term();
            entry = crate::common::raft::LogEntry {
                term,
                index,
                data: _entry,
            };
            log.push(entry.clone());
        }
        let peers = {
            let peers_guard = self.peers.lock().unwrap();
            peers_guard.clone()
        };
        let entry_snapshot = entry.clone();
        let node_id = self.node_id.clone();
        let mut ack_count = 1; // Leader self-ack
        for peer in &peers {
            let req = crate::common::raft::AppendRequest {
                term,
                leader_id: node_id.clone(),
                prev_log_index: index - 1,
                prev_log_term: term,
                entries: vec![entry_snapshot.clone()],
                leader_commit: index,
            };
            if let Ok(resp) = send_append_entries_rpc(peer, req).await {
                if resp.success {
                    ack_count += 1;
                }
            }
        }
        let majority = (peers.len() + 1).div_ceil(2);
        if ack_count >= majority {
            // Effective commit: advance commit_index
            let mut commit = self.commit_index.lock().unwrap();
            *commit = index;
            let mut applied = self.last_applied.lock().unwrap();
            while *applied < *commit {
                // Apply log[*applied] to the real state machine here
                *applied += 1;
            }
            Ok(())
        } else {
            Err(crate::Error::Internal(
                "Raft: no majority for commit".to_string(),
            ))
        }
    }
}

pub fn start_raft_tasks(node: Arc<RaftNode>) -> tokio::task::JoinHandle<()> {
    tokio::spawn({
        let node = node.clone();
        async move {
            let mut last_heartbeat = tokio::time::Instant::now();
            let mut election_timeout =
                tokio::time::Duration::from_millis(150 + rand::random::<u64>() % 150);
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                // Clone peers each loop to avoid holding MutexGuard
                let peers = node.peers.lock().unwrap().clone();
                // If follower and no heartbeat received, start election
                if !node.is_leader() && last_heartbeat.elapsed() > election_timeout {
                    tracing::info!("Node {} starting election", node.node_id);
                    node.start_election_and_collect_votes(peers).await;
                    election_timeout =
                        tokio::time::Duration::from_millis(150 + rand::random::<u64>() % 150);
                    last_heartbeat = tokio::time::Instant::now();
                }
                // If leader, send heartbeats
                if node.is_leader() {
                    node.send_heartbeats().await;
                    last_heartbeat = tokio::time::Instant::now();
                }
            }
        }
    })
}
