//! HTTP server for public blob access

use crate::common::decode_key;
use crate::volume::blob::BlobStore;
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
use tower_http::limit::RequestBodyLimitLayer;

#[derive(Clone)]
pub struct VolumeState {
    pub store: Arc<Mutex<BlobStore>>,
    pub volume_id: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct PutResponse {
    key: String,
    size: u64,
    blake3: String,
    volume_id: String,
}

#[derive(Serialize)]
struct StatsResponse {
    volume_id: String,
    total_keys: usize,
    total_bytes: u64,
    total_mb: f64,
}

/// Create HTTP router
pub fn create_router(state: VolumeState, max_size_mb: usize) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/stats", get(stats_handler))
        .route("/blobs/:key", post(put_blob))
        .route("/blobs/:key", get(get_blob))
        .route("/blobs/:key", delete(delete_blob))
        .layer(RequestBodyLimitLayer::new(max_size_mb * 1024 * 1024))
        .with_state(state)
}

async fn health_check(State(state): State<VolumeState>) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    let stats = store.stats();

    Json(serde_json::json!({
        "status": "healthy",
        "volume_id": state.volume_id,
        "keys": stats.total_keys,
        "bytes": stats.total_bytes,
    }))
}

async fn stats_handler(State(state): State<VolumeState>) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    let stats = store.stats();

    Json(StatsResponse {
        volume_id: state.volume_id.clone(),
        total_keys: stats.total_keys,
        total_bytes: stats.total_bytes,
        total_mb: stats.total_bytes as f64 / (1024.0 * 1024.0),
    })
}

async fn put_blob(
    State(state): State<VolumeState>,
    Path(encoded_key): Path<String>,
    body: Bytes,
) -> Response {
    let key = match decode_key(&encoded_key) {
        Ok(k) => k,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid key: {}", e),
                }),
            )
                .into_response()
        }
    };

    let mut store = state.store.lock().unwrap();
    match store.put(&key, &body) {
        Ok(loc) => (
            StatusCode::CREATED,
            Json(PutResponse {
                key: key.clone(),
                size: loc.size,
                blake3: loc.blake3,
                volume_id: state.volume_id.clone(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn get_blob(State(state): State<VolumeState>, Path(encoded_key): Path<String>) -> Response {
    let key = match decode_key(&encoded_key) {
        Ok(k) => k,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid key: {}", e),
                }),
            )
                .into_response()
        }
    };

    let store = state.store.lock().unwrap();
    match store.get(&key) {
        Ok(Some(data)) => (StatusCode::OK, data).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Blob not found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn delete_blob(
    State(state): State<VolumeState>,
    Path(encoded_key): Path<String>,
) -> Response {
    let key = match decode_key(&encoded_key) {
        Ok(k) => k,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid key: {}", e),
                }),
            )
                .into_response()
        }
    };

    let mut store = state.store.lock().unwrap();
    match store.delete(&key) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Blob not found".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}
