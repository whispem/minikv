use crate::common::raft::{AppendRequest, AppendResponse, LogEntry, VoteRequest, VoteResponse};
use crate::common::Result;
use crate::coordinator::raft_rpc_client::{self, PeerClient};
use crate::coordinator::raft_storage::{HardState, RaftStorage};
use futures_util::future::join_all;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::{watch, Notify};
use tokio::time::MissedTickBehavior;

const MAX_BATCH: usize = 256;
const PROPOSE_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(20);
const TICK: Duration = Duration::from_millis(10);

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

pub type ApplyFn = Arc<dyn Fn(&[u8]) + Send + Sync>;

enum Replication {
    Idle,
    Unreachable,
    UpToDate,
    More,
}

struct Volatile {
    role: RaftRole,
    term: u64,
    voted_for: Option<String>,
    leader_id: Option<String>,
    commit_index: u64,
    last_applied: u64,
    next_index: HashMap<String, u64>,
    match_index: HashMap<String, u64>,
    last_ack: HashMap<String, Instant>,
    last_contact: Instant,
    election_timeout: Duration,
}

pub struct RaftNode {
    node_id: String,
    peers: Mutex<Vec<String>>,
    state: Mutex<Volatile>,
    log: Mutex<Vec<LogEntry>>,
    storage: Mutex<Option<RaftStorage>>,
    clients: Mutex<HashMap<String, PeerClient>>,
    apply_fn: Mutex<Option<ApplyFn>>,
    apply_lock: Mutex<()>,
    applied: watch::Sender<u64>,
    replicate_signal: Notify,
    ack_signal: Notify,
    leader_hint: Mutex<Option<String>>,
    election_ms: u64,
    heartbeat: Duration,
}

fn random_timeout(base_ms: u64) -> Duration {
    Duration::from_millis(base_ms + rand::random::<u64>() % base_ms)
}

fn majority(cluster_size: usize) -> usize {
    cluster_size / 2 + 1
}

fn last_log_info(log: &[LogEntry]) -> (u64, u64) {
    log.last().map(|e| (e.index, e.term)).unwrap_or((0, 0))
}

impl RaftNode {
    pub fn new(node_id: String) -> Self {
        Self::build(
            node_id,
            Vec::new(),
            None,
            HardState::default(),
            Vec::new(),
            300,
            50,
        )
    }

    pub fn with_config(
        node_id: String,
        peers: Vec<String>,
        storage_dir: Option<PathBuf>,
        election_timeout_ms: u64,
        heartbeat_interval_ms: u64,
    ) -> Result<Self> {
        let peers = peers
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(raft_rpc_client::normalize_addr)
            .collect();
        let (storage, hard_state, log) = match storage_dir {
            Some(dir) => {
                let (storage, hard_state, log) = RaftStorage::open(&dir)?;
                (Some(storage), hard_state, log)
            }
            None => (None, HardState::default(), Vec::new()),
        };
        if !log.is_empty() || hard_state.term > 0 {
            tracing::info!(
                "Raft state restored: term {}, {} log entries",
                hard_state.term,
                log.len()
            );
        }
        Ok(Self::build(
            node_id,
            peers,
            storage,
            hard_state,
            log,
            election_timeout_ms,
            heartbeat_interval_ms,
        ))
    }

    fn build(
        node_id: String,
        peers: Vec<String>,
        storage: Option<RaftStorage>,
        hard_state: HardState,
        log: Vec<LogEntry>,
        election_ms: u64,
        heartbeat_ms: u64,
    ) -> Self {
        let election_ms = election_ms.max(10);
        let (applied, _) = watch::channel(0);
        Self {
            node_id,
            peers: Mutex::new(peers),
            state: Mutex::new(Volatile {
                role: RaftRole::Follower,
                term: hard_state.term,
                voted_for: hard_state.voted_for,
                leader_id: None,
                commit_index: 0,
                last_applied: 0,
                next_index: HashMap::new(),
                match_index: HashMap::new(),
                last_ack: HashMap::new(),
                last_contact: Instant::now(),
                election_timeout: random_timeout(election_ms),
            }),
            log: Mutex::new(log),
            storage: Mutex::new(storage),
            clients: Mutex::new(HashMap::new()),
            apply_fn: Mutex::new(None),
            apply_lock: Mutex::new(()),
            applied,
            replicate_signal: Notify::new(),
            ack_signal: Notify::new(),
            leader_hint: Mutex::new(None),
            election_ms,
            heartbeat: Duration::from_millis(heartbeat_ms.max(1)),
        }
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn get_peers(&self) -> Vec<String> {
        self.peers.lock().unwrap().clone()
    }

    pub fn is_leader(&self) -> bool {
        self.state.lock().unwrap().role == RaftRole::Leader
    }

    pub fn get_role(&self) -> RaftRole {
        self.state.lock().unwrap().role
    }

    pub fn get_term(&self) -> u64 {
        self.state.lock().unwrap().term
    }

    pub fn get_leader(&self) -> Option<String> {
        self.state.lock().unwrap().leader_id.clone()
    }

    pub fn commit_index(&self) -> u64 {
        self.state.lock().unwrap().commit_index
    }

    pub fn last_applied(&self) -> u64 {
        self.state.lock().unwrap().last_applied
    }

    pub fn log_len(&self) -> u64 {
        self.log.lock().unwrap().len() as u64
    }

    pub fn get_log(&self) -> MutexGuard<'_, Vec<LogEntry>> {
        self.log.lock().unwrap()
    }

    pub fn set_apply_fn<F>(&self, apply: F)
    where
        F: Fn(&[u8]) + Send + Sync + 'static,
    {
        *self.apply_fn.lock().unwrap() = Some(Arc::new(apply));
    }

    pub fn become_leader(&self) {
        let peers = self.get_peers();
        let mut st = self.state.lock().unwrap();
        let log_len = self.log.lock().unwrap().len() as u64;
        self.init_leader(&mut st, &peers, log_len);
    }

    pub fn step_down(&self, new_term: u64, leader_id: Option<String>) {
        let mut st = self.state.lock().unwrap();
        if Self::become_follower(&mut st, new_term, leader_id) {
            if let Err(e) = self.persist_state(&st) {
                tracing::error!("Cannot persist raft state: {}", e);
            }
        }
        st.last_contact = Instant::now();
    }

    pub fn handle_request_vote(&self, req: VoteRequest) -> VoteResponse {
        let mut st = self.state.lock().unwrap();
        let mut dirty = false;
        if req.term > st.term {
            Self::become_follower(&mut st, req.term, None);
            dirty = true;
        }
        let mut granted = false;
        if req.term == st.term {
            let (last_index, last_term) = last_log_info(&self.log.lock().unwrap());
            let up_to_date = req.last_log_term > last_term
                || (req.last_log_term == last_term && req.last_log_index >= last_index);
            let available = match &st.voted_for {
                None => true,
                Some(candidate) => candidate == &req.candidate_id,
            };
            if available && up_to_date {
                dirty |= st.voted_for.is_none();
                st.voted_for = Some(req.candidate_id.clone());
                st.last_contact = Instant::now();
                granted = true;
            }
        }
        if dirty {
            if let Err(e) = self.persist_state(&st) {
                tracing::error!("Cannot persist raft state, vote refused: {}", e);
                granted = false;
            }
        }
        VoteResponse {
            term: st.term,
            vote_granted: granted,
        }
    }

    pub fn handle_append_entries(&self, req: AppendRequest) -> AppendResponse {
        let (response, needs_apply) = self.append_locked(req);
        if needs_apply {
            self.apply_committed();
        }
        response
    }

    fn append_locked(&self, req: AppendRequest) -> (AppendResponse, bool) {
        let mut st = self.state.lock().unwrap();
        let reject = |term: u64, conflict_index: u64| AppendResponse {
            term,
            success: false,
            conflict_index,
        };
        if req.term < st.term {
            return (reject(st.term, 0), false);
        }
        if (req.term > st.term || st.role != RaftRole::Follower)
            && Self::become_follower(&mut st, req.term, None)
        {
            if let Err(e) = self.persist_state(&st) {
                tracing::error!("Cannot persist raft state: {}", e);
                return (reject(st.term, 0), false);
            }
        }
        st.leader_id = Some(req.leader_id.clone());
        st.last_contact = Instant::now();

        let mut log = self.log.lock().unwrap();
        let last_index = log.len() as u64;
        if req.prev_log_index > last_index {
            return (reject(st.term, last_index + 1), false);
        }
        if req.prev_log_index > 0 {
            let local_term = log[(req.prev_log_index - 1) as usize].term;
            if local_term != req.prev_log_term {
                let mut first = req.prev_log_index;
                while first > 1 && log[(first - 2) as usize].term == local_term {
                    first -= 1;
                }
                return (reject(st.term, first), false);
            }
        }

        let last_new = req.prev_log_index + req.entries.len() as u64;
        let mut fresh = Vec::new();
        for (offset, entry) in req.entries.into_iter().enumerate() {
            let index = req.prev_log_index + 1 + offset as u64;
            let position = (index - 1) as usize;
            if position < log.len() {
                if log[position].term == entry.term {
                    continue;
                }
                if let Err(e) = self.persist_truncate(index) {
                    tracing::error!("Cannot truncate raft log: {}", e);
                    return (reject(st.term, 0), false);
                }
                log.truncate(position);
            }
            fresh.push(LogEntry {
                term: entry.term,
                index,
                data: entry.data,
            });
        }
        if !fresh.is_empty() {
            if let Err(e) = self.persist_append(&fresh) {
                tracing::error!("Cannot append to raft log: {}", e);
                return (reject(st.term, 0), false);
            }
            log.extend(fresh);
        }

        let target = req.leader_commit.min(last_new);
        if target > st.commit_index {
            st.commit_index = target;
        }
        let response = AppendResponse {
            term: st.term,
            success: true,
            conflict_index: 0,
        };
        (response, st.commit_index > st.last_applied)
    }

    pub async fn propose(&self, data: Vec<u8>) -> Result<u64> {
        let (index, term) = {
            let st = self.state.lock().unwrap();
            if st.role != RaftRole::Leader {
                return Err(crate::Error::NotLeader(
                    st.leader_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                ));
            }
            let mut log = self.log.lock().unwrap();
            let entry = LogEntry {
                term: st.term,
                index: log.len() as u64 + 1,
                data,
            };
            self.persist_append(std::slice::from_ref(&entry))?;
            let position = (entry.index, entry.term);
            log.push(entry);
            position
        };
        self.replicate_signal.notify_waiters();
        self.advance_commit();
        self.apply_committed();
        self.wait_applied(index, term).await
    }

    pub async fn replicate(&self, data: Vec<u8>) -> Result<()> {
        self.propose(data).await.map(|_| ())
    }

    async fn wait_applied(&self, index: u64, term: u64) -> Result<u64> {
        self.wait_applied_index(index, Instant::now() + PROPOSE_TIMEOUT)
            .await?;
        let log = self.log.lock().unwrap();
        match log.get((index - 1) as usize) {
            Some(entry) if entry.term == term => Ok(index),
            _ => Err(crate::Error::Raft(
                "entry was replaced by another leader".to_string(),
            )),
        }
    }

    async fn wait_applied_index(&self, index: u64, deadline: Instant) -> Result<()> {
        let mut applied = self.applied.subscribe();
        let reached = async {
            loop {
                if *applied.borrow_and_update() >= index {
                    return true;
                }
                if applied.changed().await.is_err() {
                    return false;
                }
            }
        };
        match tokio::time::timeout_at(deadline.into(), reached).await {
            Ok(true) => Ok(()),
            _ => Err(crate::Error::ConsensusTimeout),
        }
    }

    pub async fn read_index(&self) -> Result<u64> {
        let started = Instant::now();
        let deadline = started + READ_TIMEOUT;
        let peers = self.get_peers();
        let needed = majority(peers.len() + 1);
        loop {
            let acked = self.ack_signal.notified();
            let confirmed = {
                let st = self.state.lock().unwrap();
                if st.role != RaftRole::Leader {
                    return Err(crate::Error::NotLeader(
                        st.leader_id
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                    ));
                }
                let log = self.log.lock().unwrap();
                let committed_in_term = st.commit_index > 0
                    && log.get((st.commit_index - 1) as usize).map(|e| e.term) == Some(st.term);
                let acks = 1 + peers
                    .iter()
                    .filter(|p| st.last_ack.get(*p).is_some_and(|at| *at >= started))
                    .count();
                (committed_in_term && acks >= needed).then_some(st.commit_index)
            };
            if let Some(index) = confirmed {
                return Ok(index);
            }
            if Instant::now() >= deadline {
                return Err(crate::Error::ConsensusTimeout);
            }
            self.replicate_signal.notify_waiters();
            let wake_up = (Instant::now() + self.heartbeat).min(deadline);
            let _ = tokio::time::timeout_at(wake_up.into(), acked).await;
        }
    }

    pub async fn read_barrier(&self) -> Result<()> {
        let deadline = Instant::now() + READ_TIMEOUT;
        loop {
            let attempt = if self.is_leader() {
                self.read_index().await
            } else {
                self.remote_read_index().await
            };
            match attempt {
                Ok(index) => return self.wait_applied_index(index, deadline).await,
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => tokio::time::sleep(RETRY_INTERVAL).await,
            }
        }
    }

    async fn remote_read_index(&self) -> Result<u64> {
        let hint = self.leader_hint.lock().unwrap().clone();
        if let Some(peer) = hint {
            if let Some(index) = self.ask_read_index(&peer).await {
                return Ok(index);
            }
        }
        let peers = self.get_peers();
        let answers = join_all(peers.iter().map(|peer| async move {
            self.ask_read_index(peer)
                .await
                .map(|index| (peer.clone(), index))
        }))
        .await;
        match answers.into_iter().flatten().next() {
            Some((peer, index)) => {
                *self.leader_hint.lock().unwrap() = Some(peer);
                Ok(index)
            }
            None => Err(crate::Error::NotLeader(
                self.get_leader().unwrap_or_else(|| "unknown".to_string()),
            )),
        }
    }

    async fn ask_read_index(&self, peer: &str) -> Option<u64> {
        let mut client = self.client_for(peer)?;
        let response = raft_rpc_client::read_index(&mut client).await.ok()?;
        response.is_leader.then_some(response.read_index)
    }

    fn become_follower(st: &mut Volatile, term: u64, leader_id: Option<String>) -> bool {
        let term_changed = term > st.term;
        if term_changed {
            st.term = term;
            st.voted_for = None;
        }
        st.role = RaftRole::Follower;
        st.leader_id = leader_id;
        st.next_index.clear();
        st.match_index.clear();
        st.last_ack.clear();
        term_changed
    }

    fn init_leader(&self, st: &mut Volatile, peers: &[String], log_len: u64) {
        let now = Instant::now();
        st.role = RaftRole::Leader;
        st.leader_id = Some(self.node_id.clone());
        st.next_index = peers.iter().map(|p| (p.clone(), log_len + 1)).collect();
        st.match_index = peers.iter().map(|p| (p.clone(), 0)).collect();
        st.last_ack = peers.iter().map(|p| (p.clone(), now)).collect();
    }

    fn persist_state(&self, st: &Volatile) -> Result<()> {
        match self.storage.lock().unwrap().as_ref() {
            Some(storage) => storage.save_state(&HardState {
                term: st.term,
                voted_for: st.voted_for.clone(),
            }),
            None => Ok(()),
        }
    }

    fn persist_append(&self, entries: &[LogEntry]) -> Result<()> {
        match self.storage.lock().unwrap().as_mut() {
            Some(storage) => storage.append(entries),
            None => Ok(()),
        }
    }

    fn persist_truncate(&self, index: u64) -> Result<()> {
        match self.storage.lock().unwrap().as_mut() {
            Some(storage) => storage.truncate_from(index),
            None => Ok(()),
        }
    }

    fn apply_committed(&self) {
        let _serial = self.apply_lock.lock().unwrap();
        loop {
            let (from, to) = {
                let st = self.state.lock().unwrap();
                (st.last_applied, st.commit_index)
            };
            if from >= to {
                return;
            }
            let batch: Vec<LogEntry> = {
                let log = self.log.lock().unwrap();
                let end = (to as usize).min(log.len());
                if from as usize >= end {
                    return;
                }
                log[from as usize..end].to_vec()
            };
            let apply = self.apply_fn.lock().unwrap().clone();
            let mut last = from;
            for entry in &batch {
                if let Some(apply) = &apply {
                    if !entry.data.is_empty() {
                        apply(&entry.data);
                    }
                }
                last = entry.index;
            }
            self.state.lock().unwrap().last_applied = last;
            self.applied.send_replace(last);
        }
    }

    fn advance_commit(&self) {
        let peers = self.get_peers();
        let advanced = {
            let mut st = self.state.lock().unwrap();
            if st.role != RaftRole::Leader {
                return;
            }
            let log = self.log.lock().unwrap();
            let needed = majority(peers.len() + 1);
            let before = st.commit_index;
            let mut candidate = log.len() as u64;
            while candidate > st.commit_index {
                let entry_term = log[(candidate - 1) as usize].term;
                if entry_term < st.term {
                    break;
                }
                let replicated = 1 + peers
                    .iter()
                    .filter(|p| st.match_index.get(*p).copied().unwrap_or(0) >= candidate)
                    .count();
                if replicated >= needed {
                    st.commit_index = candidate;
                    break;
                }
                candidate -= 1;
            }
            st.commit_index > before
        };
        if advanced && !peers.is_empty() {
            self.replicate_signal.notify_waiters();
        }
    }

    fn client_for(&self, peer: &str) -> Option<PeerClient> {
        let mut clients = self.clients.lock().unwrap();
        if let Some(client) = clients.get(peer) {
            return Some(client.clone());
        }
        match raft_rpc_client::lazy_client(peer) {
            Ok(client) => {
                clients.insert(peer.to_string(), client.clone());
                Some(client)
            }
            Err(e) => {
                tracing::error!("Invalid peer address {}: {}", peer, e);
                None
            }
        }
    }

    async fn run_election(&self) {
        let peers = self.get_peers();
        let request = {
            let mut st = self.state.lock().unwrap();
            st.term += 1;
            st.role = RaftRole::Candidate;
            st.voted_for = Some(self.node_id.clone());
            st.leader_id = None;
            st.last_contact = Instant::now();
            st.election_timeout = random_timeout(self.election_ms);
            if let Err(e) = self.persist_state(&st) {
                tracing::error!("Cannot persist raft state, election aborted: {}", e);
                st.role = RaftRole::Follower;
                return;
            }
            let (last_log_index, last_log_term) = last_log_info(&self.log.lock().unwrap());
            VoteRequest {
                term: st.term,
                candidate_id: self.node_id.clone(),
                last_log_index,
                last_log_term,
            }
        };
        tracing::debug!(
            "{} starts an election for term {}",
            self.node_id,
            request.term
        );

        let ballots = peers.iter().map(|peer| {
            let client = self.client_for(peer);
            let request = request.clone();
            async move {
                let mut client = client?;
                raft_rpc_client::request_vote(&mut client, &request)
                    .await
                    .ok()
            }
        });
        let responses = join_all(ballots).await;

        let mut votes = 1;
        let mut highest_term = request.term;
        for response in responses.into_iter().flatten() {
            highest_term = highest_term.max(response.term);
            if response.vote_granted && response.term == request.term {
                votes += 1;
            }
        }

        let mut st = self.state.lock().unwrap();
        if highest_term > st.term {
            Self::become_follower(&mut st, highest_term, None);
            if let Err(e) = self.persist_state(&st) {
                tracing::error!("Cannot persist raft state: {}", e);
            }
            return;
        }
        if st.term != request.term || st.role != RaftRole::Candidate {
            return;
        }
        if votes < majority(peers.len() + 1) {
            tracing::debug!(
                "{} got {} vote(s) for term {}, not enough",
                self.node_id,
                votes,
                request.term
            );
            return;
        }

        let mut log = self.log.lock().unwrap();
        self.init_leader(&mut st, &peers, log.len() as u64);
        let noop = LogEntry {
            term: st.term,
            index: log.len() as u64 + 1,
            data: Vec::new(),
        };
        if let Err(e) = self.persist_append(std::slice::from_ref(&noop)) {
            tracing::error!("Cannot append to raft log, giving up leadership: {}", e);
            let term = st.term;
            Self::become_follower(&mut st, term, None);
            return;
        }
        log.push(noop);
        drop(log);
        drop(st);

        tracing::info!(
            "{} elected leader for term {} with {} vote(s)",
            self.node_id,
            request.term,
            votes
        );
        self.replicate_signal.notify_waiters();
        self.advance_commit();
        self.apply_committed();
    }

    async fn replicate_to(&self, peer: &str, client: &mut PeerClient) -> Replication {
        let request = {
            let st = self.state.lock().unwrap();
            if st.role != RaftRole::Leader {
                return Replication::Idle;
            }
            let log = self.log.lock().unwrap();
            let log_len = log.len() as u64;
            let next = st
                .next_index
                .get(peer)
                .copied()
                .unwrap_or(log_len + 1)
                .clamp(1, log_len + 1);
            let prev_log_index = next - 1;
            let prev_log_term = match prev_log_index {
                0 => 0,
                i => log[(i - 1) as usize].term,
            };
            let end = log.len().min(prev_log_index as usize + MAX_BATCH);
            AppendRequest {
                term: st.term,
                leader_id: self.node_id.clone(),
                prev_log_index,
                prev_log_term,
                entries: log[prev_log_index as usize..end].to_vec(),
                leader_commit: st.commit_index,
            }
        };
        let sent_up_to = request.prev_log_index + request.entries.len() as u64;
        let sent_at = Instant::now();

        let response = match raft_rpc_client::append_entries(client, &request).await {
            Ok(response) => response,
            Err(e) => {
                tracing::trace!("AppendEntries to {} failed: {}", peer, e);
                return Replication::Unreachable;
            }
        };

        let outcome = {
            let mut st = self.state.lock().unwrap();
            if response.term > st.term {
                tracing::info!(
                    "{} steps down: {} is at term {}",
                    self.node_id,
                    peer,
                    response.term
                );
                Self::become_follower(&mut st, response.term, None);
                st.last_contact = Instant::now();
                if let Err(e) = self.persist_state(&st) {
                    tracing::error!("Cannot persist raft state: {}", e);
                }
                Replication::Idle
            } else if st.role != RaftRole::Leader || st.term != request.term {
                Replication::Idle
            } else {
                st.last_ack.insert(peer.to_string(), sent_at);
                if response.success {
                    let matched = st.match_index.entry(peer.to_string()).or_insert(0);
                    *matched = (*matched).max(sent_up_to);
                    let next = st.next_index.entry(peer.to_string()).or_insert(1);
                    *next = (*next).max(sent_up_to + 1);
                    if sent_up_to < self.log.lock().unwrap().len() as u64 {
                        Replication::More
                    } else {
                        Replication::UpToDate
                    }
                } else {
                    let current = st.next_index.get(peer).copied().unwrap_or(1);
                    let hint = match response.conflict_index {
                        0 => current.saturating_sub(1),
                        conflict => conflict,
                    };
                    st.next_index
                        .insert(peer.to_string(), hint.min(current.saturating_sub(1)).max(1));
                    Replication::More
                }
            }
        };

        if response.success && !matches!(outcome, Replication::Idle) {
            self.advance_commit();
            self.apply_committed();
        }
        self.ack_signal.notify_waiters();
        outcome
    }

    async fn peer_loop(self: Arc<Self>, peer: String) {
        let mut client = match raft_rpc_client::lazy_client(&peer) {
            Ok(client) => client,
            Err(e) => {
                tracing::error!("Invalid peer address {}: {}", peer, e);
                return;
            }
        };
        loop {
            let notified = self.replicate_signal.notified();
            if let Replication::More = self.replicate_to(&peer, &mut client).await {
                continue;
            }
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep(self.heartbeat) => {}
            }
        }
    }

    async fn election_loop(self: Arc<Self>) {
        if self.get_peers().is_empty() {
            self.run_election().await;
        }
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let peers = self.get_peers();
            let start_election = {
                let mut st = self.state.lock().unwrap();
                if st.role == RaftRole::Leader {
                    let window = Duration::from_millis(self.election_ms * 2);
                    let reachable = 1 + peers
                        .iter()
                        .filter(|p| {
                            st.last_ack
                                .get(*p)
                                .map(|at| at.elapsed() < window)
                                .unwrap_or(false)
                        })
                        .count();
                    if reachable < majority(peers.len() + 1) {
                        tracing::warn!(
                            "{} lost contact with the majority, stepping down",
                            self.node_id
                        );
                        let term = st.term;
                        Self::become_follower(&mut st, term, None);
                        st.last_contact = Instant::now();
                    }
                    false
                } else {
                    st.last_contact.elapsed() >= st.election_timeout
                }
            };
            if start_election {
                self.run_election().await;
            }
        }
    }
}

pub fn start_raft_tasks(node: Arc<RaftNode>) -> tokio::task::JoinHandle<()> {
    for peer in node.get_peers() {
        tokio::spawn(node.clone().peer_loop(peer));
    }
    tokio::spawn(node.election_loop())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: u64, index: u64, data: &[u8]) -> LogEntry {
        LogEntry {
            term,
            index,
            data: data.to_vec(),
        }
    }

    fn append(term: u64, prev: (u64, u64), entries: Vec<LogEntry>, commit: u64) -> AppendRequest {
        AppendRequest {
            term,
            leader_id: "leader".to_string(),
            prev_log_index: prev.0,
            prev_log_term: prev.1,
            entries,
            leader_commit: commit,
        }
    }

    #[test]
    fn vote_requires_an_up_to_date_log() {
        let node = RaftNode::new("n1".to_string());
        node.handle_append_entries(append(2, (0, 0), vec![entry(2, 1, b"x")], 0));
        let stale = node.handle_request_vote(VoteRequest {
            term: 3,
            candidate_id: "n2".to_string(),
            last_log_index: 5,
            last_log_term: 1,
        });
        assert!(!stale.vote_granted);
        assert_eq!(stale.term, 3);
        let fresh = node.handle_request_vote(VoteRequest {
            term: 3,
            candidate_id: "n3".to_string(),
            last_log_index: 1,
            last_log_term: 2,
        });
        assert!(fresh.vote_granted);
        let second = node.handle_request_vote(VoteRequest {
            term: 3,
            candidate_id: "n2".to_string(),
            last_log_index: 9,
            last_log_term: 3,
        });
        assert!(!second.vote_granted);
    }

    #[test]
    fn append_rejects_stale_terms_and_gaps() {
        let node = RaftNode::new("n1".to_string());
        node.step_down(5, None);
        let stale = node.handle_append_entries(append(4, (0, 0), vec![], 0));
        assert!(!stale.success);
        assert_eq!(stale.term, 5);
        let gap = node.handle_append_entries(append(5, (3, 5), vec![], 0));
        assert!(!gap.success);
        assert_eq!(gap.conflict_index, 1);
    }

    #[test]
    fn conflicting_entries_are_replaced_and_applied_once() {
        let node = RaftNode::new("n1".to_string());
        let applied = Arc::new(Mutex::new(Vec::new()));
        let sink = applied.clone();
        node.set_apply_fn(move |data| sink.lock().unwrap().push(data.to_vec()));

        node.handle_append_entries(append(
            1,
            (0, 0),
            vec![entry(1, 1, b"a"), entry(1, 2, b"b"), entry(1, 3, b"c")],
            1,
        ));
        let mismatch = node.handle_append_entries(append(2, (3, 2), vec![], 1));
        assert!(!mismatch.success);
        assert_eq!(mismatch.conflict_index, 1);

        let ok = node.handle_append_entries(append(
            2,
            (1, 1),
            vec![entry(2, 2, b"B"), entry(2, 3, b"C")],
            3,
        ));
        assert!(ok.success);
        let data: Vec<Vec<u8>> = node.get_log().iter().map(|e| e.data.clone()).collect();
        assert_eq!(data, vec![b"a".to_vec(), b"B".to_vec(), b"C".to_vec()]);
        assert_eq!(node.commit_index(), 3);
        assert_eq!(
            *applied.lock().unwrap(),
            vec![b"a".to_vec(), b"B".to_vec(), b"C".to_vec()]
        );

        node.handle_append_entries(append(2, (3, 2), vec![], 3));
        assert_eq!(applied.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn single_node_commits_on_its_own() {
        let node = Arc::new(RaftNode::new("solo".to_string()));
        let applied = Arc::new(Mutex::new(Vec::new()));
        let sink = applied.clone();
        node.set_apply_fn(move |data| sink.lock().unwrap().push(data.to_vec()));
        node.run_election().await;
        assert!(node.is_leader());
        assert_eq!(node.get_term(), 1);
        let index = node.propose(b"put k".to_vec()).await.unwrap();
        assert_eq!(index, 2);
        assert_eq!(node.commit_index(), 2);
        assert_eq!(*applied.lock().unwrap(), vec![b"put k".to_vec()]);
    }

    #[tokio::test]
    async fn leader_reads_wait_for_committed_writes() {
        let node = Arc::new(RaftNode::new("solo".to_string()));
        assert!(node.read_index().await.is_err());
        node.run_election().await;
        let index = node.propose(b"x".to_vec()).await.unwrap();
        assert!(node.read_index().await.unwrap() >= index);
        node.read_barrier().await.unwrap();
        assert!(node.last_applied() >= index);
    }

    #[tokio::test]
    async fn followers_cannot_propose() {
        let node = RaftNode::new("n1".to_string());
        node.step_down(1, Some("n2".to_string()));
        match node.propose(b"x".to_vec()).await {
            Err(crate::Error::NotLeader(leader)) => assert_eq!(leader, "n2"),
            other => panic!("unexpected result: {:?}", other.map(|_| ())),
        }
    }
}
