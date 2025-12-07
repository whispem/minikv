//! HTTP API for coordinator (public-facing)

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use serde_json::json;
use std::sync::Arc;

use crate::coordinator::metadata::MetadataStore;
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::RaftNode;

#[derive(Clone)]
pub struct CoordState {
    pub metadata: Arc<MetadataStore>,
    pub placement: Arc<std::sync::Mutex<PlacementManager>>,
    pub raft: Arc<RaftNode>,
}

pub fn create_router(state: CoordState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/:key", post(put_key))
        .route("/:key", get(get_key))
        .route("/:key", delete(delete_key))
        .with_state(state)
}

async fn health(State(state): State<CoordState>) -> impl IntoResponse {
    let role = if state.raft.is_leader() {
        "Leader"
    } else {
        "Follower"
    };

    axum::Json(json!({
        "status": "healthy",
        "role": role,
        "is_leader": state.raft. is_leader(),
    }))
}

async fn put_key(
    State(_state): State<CoordState>,
    Path(key): Path<String>,
    _body: Bytes,
) -> impl IntoResponse {
    // TODO: Implement 2PC
    (StatusCode::NOT_IMPLEMENTED, format!("PUT {} - 2PC not yet implemented", key))
}

async fn get_key(
    State(_state): State<CoordState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, format!("GET {} not implemented", key))
}

async fn delete_key(
    State(_state): State<CoordState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, format!("DELETE {} not implemented", key))
}
