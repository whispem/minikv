//! HTTP API for coordinator (public interface)

use crate::coordinator::metadata::{KeyMetadata, KeyState, MetadataStore};
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::RaftNode;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct CoordState {
    pub metadata: Arc<MetadataStore>,
    pub placement: Arc<Mutex<PlacementManager>>,
    pub raft: Arc<RaftNode>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

pub fn create_router(state: CoordState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/:key", post(put_key))
        .route("/:key", get(get_key))
        .route("/:key", delete(delete_key))
        .with_state(state)
}

async fn health_check(State(state): State<CoordState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "role": format!("{:?}", state.raft.get_role()),
        "is_leader": state.raft.is_leader(),
    }))
}

async fn put_key(
    State(state): State<CoordState>,
    Path(key): Path<String>,
    body: Bytes,
) -> Response {
    if !state.raft.is_leader() {
        return (
            StatusCode::TEMPORARY_REDIRECT,
            Json(ErrorResponse {
                error: "Not leader".to_string(),
            }),
        )
            .into_response();
    }

    // TODO: Implement 2PC write to volumes
    StatusCode::NOT_IMPLEMENTED.into_response()
}

async fn get_key(State(state): State<CoordState>, Path(key): Path<String>) -> Response {
    match state.metadata.get_key(&key) {
        Ok(Some(meta)) => {
            // TODO: Redirect to volume or proxy read
            Json(meta).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn delete_key(State(state): State<CoordState>, Path(key): Path<String>) -> Response {
    if !state.raft.is_leader() {
        return StatusCode::TEMPORARY_REDIRECT.into_response();
    }

    // TODO: Implement delete with 2PC
    StatusCode::NOT_IMPLEMENTED.into_response()
}
