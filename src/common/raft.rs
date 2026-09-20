use serde::{Deserialize, Serialize};

impl From<&VoteRequest> for crate::proto::VoteRequest {
    fn from(req: &VoteRequest) -> Self {
        Self {
            term: req.term,
            candidate_id: req.candidate_id.clone(),
            last_log_index: req.last_log_index,
            last_log_term: req.last_log_term,
        }
    }
}

impl From<crate::proto::VoteRequest> for VoteRequest {
    fn from(req: crate::proto::VoteRequest) -> Self {
        Self {
            term: req.term,
            candidate_id: req.candidate_id,
            last_log_index: req.last_log_index,
            last_log_term: req.last_log_term,
        }
    }
}

impl From<&crate::proto::VoteResponse> for VoteResponse {
    fn from(resp: &crate::proto::VoteResponse) -> Self {
        Self {
            term: resp.term,
            vote_granted: resp.vote_granted,
        }
    }
}

impl From<VoteResponse> for crate::proto::VoteResponse {
    fn from(resp: VoteResponse) -> Self {
        Self {
            term: resp.term,
            vote_granted: resp.vote_granted,
        }
    }
}

impl From<&AppendRequest> for crate::proto::AppendRequest {
    fn from(req: &AppendRequest) -> Self {
        Self {
            term: req.term,
            leader_id: req.leader_id.clone(),
            prev_log_index: req.prev_log_index,
            prev_log_term: req.prev_log_term,
            entries: req.entries.iter().map(|e| e.into()).collect(),
            leader_commit: req.leader_commit,
        }
    }
}

impl From<crate::proto::AppendRequest> for AppendRequest {
    fn from(req: crate::proto::AppendRequest) -> Self {
        Self {
            term: req.term,
            leader_id: req.leader_id,
            prev_log_index: req.prev_log_index,
            prev_log_term: req.prev_log_term,
            entries: req.entries.into_iter().map(LogEntry::from).collect(),
            leader_commit: req.leader_commit,
        }
    }
}

impl From<&crate::proto::AppendResponse> for AppendResponse {
    fn from(resp: &crate::proto::AppendResponse) -> Self {
        Self {
            term: resp.term,
            success: resp.success,
            conflict_index: resp.conflict_index,
        }
    }
}

impl From<AppendResponse> for crate::proto::AppendResponse {
    fn from(resp: AppendResponse) -> Self {
        Self {
            term: resp.term,
            success: resp.success,
            conflict_index: resp.conflict_index,
        }
    }
}

impl From<&LogEntry> for crate::proto::LogEntry {
    fn from(e: &LogEntry) -> Self {
        Self {
            term: e.term,
            index: e.index,
            data: e.data.clone(),
        }
    }
}

impl From<crate::proto::LogEntry> for LogEntry {
    fn from(e: crate::proto::LogEntry) -> Self {
        Self {
            term: e.term,
            index: e.index,
            data: e.data,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VoteRequest {
    pub term: u64,
    pub candidate_id: String,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Debug, Clone)]
pub struct VoteResponse {
    pub term: u64,
    pub vote_granted: bool,
}

#[derive(Debug, Clone)]
pub struct AppendRequest {
    pub term: u64,
    pub leader_id: String,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

#[derive(Debug, Clone)]
pub struct AppendResponse {
    pub term: u64,
    pub success: bool,
    pub conflict_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub term: u64,
    pub index: u64,
    pub data: Vec<u8>,
}
