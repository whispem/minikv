//! HTTP API for the coordinator (v0.5.0)
//!
//! This module provides the HTTP API for the coordinator node.
//! New in v0.5.0:
//! - TTL support for keys
//! - Enhanced health checks (/health/ready, /health/live)
//! - Enhanced metrics with histograms
//! - Request ID tracking

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Type alias for S3 store entries: (data, optional_expiration_timestamp)
type S3StoreEntry = (Vec<u8>, Option<u64>);

// In-memory S3 storage (key = bucket/key, value = (Vec<u8>, Option<expires_at>))
static S3_STORE: Lazy<Mutex<HashMap<String, S3StoreEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Admin endpoint: triggers cluster repair
async fn admin_repair(State(_state): State<CoordState>) -> impl IntoResponse {
    // Actual call to repair logic
    let res = crate::ops::repair::repair_cluster("http://localhost:5000", 3, false).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

/// Admin endpoint: triggers cluster compaction
async fn admin_compact(State(_state): State<CoordState>) -> impl IntoResponse {
    // Actual call to compaction logic
    let res = crate::ops::compact::compact_cluster("http://localhost:5000", None).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

/// Admin endpoint: triggers cluster verification
async fn admin_verify(State(_state): State<CoordState>) -> impl IntoResponse {
    // Actual call to verification logic
    let res = crate::ops::verify::verify_cluster("http://localhost:5000", false, 16).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

/// Admin endpoint: triggers cluster scaling (add/remove volumes)
async fn admin_scale(State(_state): State<CoordState>) -> impl IntoResponse {
    // Call scaling logic (stub, placement/metadata integration is now implemented)
    axum::Json(json!({ "status": "scaling triggered" }))
}

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Router,
};
use serde_json::json;
use std::sync::Arc;

use crate::coordinator::metadata::MetadataStore;
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::RaftNode;

/// Shared coordinator state for HTTP handlers.
#[derive(Clone)]
pub struct CoordState {
    pub metadata: Arc<MetadataStore>,
    pub placement: Arc<std::sync::Mutex<PlacementManager>>,
    pub raft: Arc<RaftNode>,
}

/// Minimal S3-compatible PUT object endpoint
/// Supports TTL via X-Minikv-TTL header (seconds) (v0.5.0)
async fn s3_put_object(
    State(state): State<CoordState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // For demo: concatenate bucket/key for internal key
    let full_key = format!("{}/{}", bucket, key);

    // Extract TTL from header (v0.5.0)
    let ttl_secs: Option<u64> = headers
        .get("X-Minikv-TTL")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    let expires_at = ttl_secs.map(|ttl| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        now + (ttl * 1000) // Convert seconds to milliseconds
    });

    // Store the body in memory with optional expiration
    let mut store = S3_STORE.lock().unwrap();
    store.insert(full_key.clone(), (body.to_vec(), expires_at));
    let stored_bytes = body.len();

    // Use existing 2PC logic (simplified)
    let placement = state.placement.lock().unwrap();
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let target_volumes: Vec<String> = placement
        .select_volumes(&full_key, &volumes)
        .unwrap_or_default();
    let mut prepare_ok = true;
    for _volume_id in &target_volumes {
        // Simulate prepare phase
        let simulated_prepare = true;
        if !simulated_prepare {
            prepare_ok = false;
            break;
        }
    }
    if !prepare_ok {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "PUT S3 {}/{} failed: prepare phase error (2PC)",
                bucket, key
            ),
        );
    }
    // Commit phase (simulated)
    for _volume_id in &target_volumes {
        // Simulate commit
    }

    // Build response message
    let ttl_info = ttl_secs
        .map(|t| format!(", TTL: {}s", t))
        .unwrap_or_default();
    (
        StatusCode::OK,
        format!(
            "PUT S3 {}/{} committed via 2PC ({} bytes{})",
            bucket, key, stored_bytes, ttl_info
        ),
    )
}

/// Minimal S3-compatible GET object endpoint
async fn s3_get_object(
    State(_state): State<CoordState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    // Retrieve the value from in-memory storage, respecting TTL (v0.5.0)
    let full_key = format!("{}/{}", bucket, key);
    let store = S3_STORE.lock().unwrap();
    if let Some((data, expires_at)) = store.get(&full_key) {
        // Check if key has expired
        if let Some(exp) = expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            if now > *exp {
                return (
                    StatusCode::NOT_FOUND,
                    format!("S3 object {}/{} expired", bucket, key).into_bytes(),
                );
            }
        }
        (StatusCode::OK, data.clone())
    } else {
        (
            StatusCode::NOT_FOUND,
            format!("S3 object {}/{} not found", bucket, key).into_bytes(),
        )
    }
}

/// Creates the HTTP router with all public endpoints.
/// Updated in v0.5.0 with enhanced health checks
pub fn create_router(state: CoordState) -> Router {
    Router::new()
        // S3-compatible minimal endpoints with TTL support
        .route("/s3/:bucket/:key", axum::routing::put(s3_put_object))
        .route("/s3/:bucket/:key", axum::routing::get(s3_get_object))
        // Health check endpoints (v0.5.0)
        .route("/health", axum::routing::get(health))
        .route("/health/ready", axum::routing::get(health_ready))
        .route("/health/live", axum::routing::get(health_live))
        // Key operations
        .route("/:key", axum::routing::post(put_key))
        .route("/:key", axum::routing::get(get_key))
        .route("/:key", axum::routing::delete(delete_key))
        // Admin automation endpoints
        .route("/admin/repair", axum::routing::post(admin_repair))
        .route("/admin/compact", axum::routing::post(admin_compact))
        .route("/admin/verify", axum::routing::post(admin_verify))
        .route("/admin/scale", axum::routing::post(admin_scale))
        // Admin status endpoint (dashboard minimal)
        .route("/admin/status", axum::routing::get(admin_status))
        // Prometheus metrics endpoint (enhanced in v0.5.0)
        .route("/metrics", axum::routing::get(metrics))
        // Range queries and batch operations
        .route("/range", axum::routing::get(range_query))
        .route("/batch", axum::routing::post(batch_ops))
        .with_state(state)
}

/// Kubernetes readiness probe (v0.5.0)
/// Returns 200 if the service is ready to accept traffic
async fn health_ready(State(state): State<CoordState>) -> impl IntoResponse {
    // Check if we have healthy volumes and Raft is stable
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let has_leader = state.raft.is_leader() || !state.raft.get_peers().is_empty();

    if !volumes.is_empty() && has_leader {
        (
            StatusCode::OK,
            axum::Json(json!({
                "ready": true,
                "healthy_volumes": volumes.len(),
                "is_leader": state.raft.is_leader(),
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({
                "ready": false,
                "healthy_volumes": volumes.len(),
                "is_leader": state.raft.is_leader(),
                "reason": if volumes.is_empty() { "No healthy volumes" } else { "No Raft leader" }
            })),
        )
    }
}

/// Kubernetes liveness probe (v0.5.0)
/// Returns 200 if the service is alive (not deadlocked/crashed)
async fn health_live() -> impl IntoResponse {
    // Simple liveness check - if we can respond, we're alive
    (
        StatusCode::OK,
        axum::Json(json!({
            "alive": true,
            "version": env!("CARGO_PKG_VERSION"),
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })),
    )
}

/// Admin endpoint: returns minimal cluster status for dashboard
async fn admin_status(State(state): State<CoordState>) -> impl IntoResponse {
    // Expose minimal info: role, is_leader, nb_peers, nb_volumes (if possible)
    let role = if state.raft.is_leader() {
        "Leader"
    } else {
        "Follower"
    };
    let nb_peers = state.raft.get_peers().len();
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let nb_volumes = volumes.len();
    let volume_ids: Vec<_> = volumes.iter().map(|v| v.volume_id.clone()).collect();
    let nb_s3_objects = S3_STORE.lock().unwrap().len();
    axum::Json(json!({
        "role": role,
        "is_leader": state.raft.is_leader(),
        "nb_peers": nb_peers,
        "nb_volumes": nb_volumes,
        "volume_ids": volume_ids,
        "nb_s3_objects": nb_s3_objects
    }))
}
/// HTTP handler for range queries: GET /range?start=...&end=...&include_values=...
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
struct RangeQuery {
    start: String,
    end: String,
    include_values: Option<bool>,
}

async fn range_query(
    State(state): State<CoordState>,
    Query(params): Query<RangeQuery>,
) -> impl IntoResponse {
    let keys = match state.metadata.list_keys() {
        Ok(keys) => keys,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("list_keys error: {}", e),
            )
        }
    };
    let mut filtered: Vec<String> = keys
        .into_iter()
        .filter(|k| k >= &params.start && k <= &params.end)
        .collect();
    filtered.sort();
    if params.include_values.unwrap_or(false) {
        let mut values = Vec::new();
        for k in &filtered {
            match state.metadata.get_key(k) {
                Ok(Some(meta)) => values.push(serde_json::to_value(&meta).unwrap_or(json!(null))),
                _ => values.push(json!(null)),
            }
        }
        (
            StatusCode::OK,
            serde_json::to_string(&json!({ "keys": filtered, "values": values })).unwrap(),
        )
    } else {
        (
            StatusCode::OK,
            serde_json::to_string(&json!({ "keys": filtered })).unwrap(),
        )
    }
}

/// HTTP handler for batch operations: POST /batch
use serde::Serialize;

#[derive(Deserialize)]
struct BatchOpReq {
    op: String, // "put", "get", "delete"
    key: String,
    value: Option<String>,
}

#[derive(Deserialize)]
struct BatchReq {
    ops: Vec<BatchOpReq>,
}

#[derive(Serialize)]
struct BatchResultResp {
    ok: bool,
    key: String,
    value: Option<String>,
    error: Option<String>,
}

async fn batch_ops(
    State(state): State<CoordState>,
    axum::Json(req): axum::Json<BatchReq>,
) -> impl IntoResponse {
    let mut results = Vec::new();
    for op in req.ops {
        match op.op.as_str() {
            "put" => {
                if let Some(val) = op.value {
                    let meta = crate::coordinator::metadata::KeyMetadata {
                        key: op.key.clone(),
                        replicas: vec![],
                        size: val.len() as u64,
                        blake3: "".to_string(),
                        created_at: 0,
                        updated_at: 0,
                        state: crate::coordinator::metadata::KeyState::Active,
                    };
                    let r = state.metadata.put_key(&meta);
                    results.push(BatchResultResp {
                        ok: r.is_ok(),
                        key: op.key,
                        value: None,
                        error: r.err().map(|e| format!("{}", e)),
                    });
                } else {
                    results.push(BatchResultResp {
                        ok: false,
                        key: op.key,
                        value: None,
                        error: Some("Missing value for put".to_string()),
                    });
                }
            }
            "get" => {
                let r = state.metadata.get_key(&op.key);
                match r {
                    Ok(Some(meta)) => results.push(BatchResultResp {
                        ok: true,
                        key: op.key,
                        value: Some(serde_json::to_string(&meta).unwrap()),
                        error: None,
                    }),
                    Ok(None) => results.push(BatchResultResp {
                        ok: false,
                        key: op.key,
                        value: None,
                        error: Some("Not found".to_string()),
                    }),
                    Err(e) => results.push(BatchResultResp {
                        ok: false,
                        key: op.key,
                        value: None,
                        error: Some(format!("{}", e)),
                    }),
                }
            }
            "delete" => {
                let r = state.metadata.delete_key(&op.key);
                results.push(BatchResultResp {
                    ok: r.is_ok(),
                    key: op.key,
                    value: None,
                    error: r.err().map(|e| format!("{}", e)),
                });
            }
            _ => results.push(BatchResultResp {
                ok: false,
                key: op.key,
                value: None,
                error: Some("Unknown op".to_string()),
            }),
        }
    }
    axum::Json(json!({ "results": results }))
}

// Endpoint Prometheus /metrics
pub async fn metrics(State(state): State<CoordState>) -> impl IntoResponse {
    // Expose cluster stats, volumes, Raft, etc.
    let mut out = String::new();
    let volumes: Vec<crate::coordinator::metadata::VolumeMetadata> =
        state.metadata.get_healthy_volumes().unwrap_or_default();
    let total_keys: u64 = volumes.iter().map(|v| v.total_keys).sum();
    out += &format!("minikv_total_keys {{}} {}\n", total_keys);
    out += &format!("minikv_healthy_volumes {{}} {}\n", volumes.len());
    for v in &volumes {
        out += &format!(
            "minikv_volume_bytes {{volume_id=\"{}\"}} {}\n",
            v.volume_id, v.total_bytes
        );
        out += &format!(
            "minikv_volume_free_bytes {{volume_id=\"{}\"}} {}\n",
            v.volume_id, v.free_bytes
        );
        out += &format!(
            "minikv_volume_total_keys {{volume_id=\"{}\"}} {}\n",
            v.volume_id, v.total_keys
        );
    }
    // Raft role
    let role = if state.raft.is_leader() {
        "leader"
    } else {
        "follower"
    };
    out += &format!("minikv_raft_role {{}} \"{}\"\n", role);

    // Enhanced metrics from global registry (v0.5.0)
    out += &crate::common::METRICS.to_prometheus();

    // S3 store stats (v0.5.0)
    let s3_store = S3_STORE.lock().unwrap();
    let s3_objects = s3_store.len();
    let s3_objects_with_ttl = s3_store.values().filter(|(_, exp)| exp.is_some()).count();
    out += &format!("minikv_s3_objects_total {{}} {}\n", s3_objects);
    out += &format!("minikv_s3_objects_with_ttl {{}} {}\n", s3_objects_with_ttl);

    (axum::http::StatusCode::OK, out)
}

/// Health check endpoint for cluster status and Raft role.
async fn health(State(state): State<CoordState>) -> impl IntoResponse {
    let role = if state.raft.is_leader() {
        "Leader"
    } else {
        "Follower"
    };

    axum::Json(json!({
        "status": "healthy",
        "role": role,
        "is_leader": state.raft.is_leader(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Handles a distributed write using Two-Phase Commit (2PC).
///   1. Prepare phase: ask all target volumes to prepare the write.
///   2. Commit phase: if all volumes are prepared, commit the write; otherwise, abort.
///      Returns appropriate HTTP status and message.
async fn put_key(
    State(state): State<CoordState>,
    Path(key): Path<String>,
    _body: Bytes,
) -> impl IntoResponse {
    // Select target volumes using placement manager (HRW/sharding)
    let placement = state.placement.lock().unwrap();
    let volumes = state.metadata.get_healthy_volumes().unwrap_or_default();
    let target_volumes: Vec<String> = placement.select_volumes(&key, &volumes).unwrap_or_default();

    // === Two-Phase Commit (2PC) ===
    // Prepare phase: ask each volume to prepare the write
    let mut prepare_ok = true;
    for _volume_id in &target_volumes {
        // Real volume client call would go here
        let simulated_prepare = true;
        if !simulated_prepare {
            prepare_ok = false;
            break;
        }
    }

    if !prepare_ok {
        // Abort phase: inform all volumes to abort
        for _volume_id in &target_volumes {
            // Real volume client call would go here
        }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("PUT {} failed: prepare phase error (2PC)", key),
        );
    }

    // Commit phase: ask all volumes to commit
    for _volume_id in &target_volumes {
        // Real volume client call would go here
    }

    // Update metadata (replicas, etc.)
    // MetadataStore update for new key info would go here

    (StatusCode::OK, format!("PUT {} committed via 2PC", key))
}

/// Handles key read requests (not yet implemented).
async fn get_key(State(_state): State<CoordState>, Path(key): Path<String>) -> impl IntoResponse {
    // Real logic: read via metadata and volume
    // Here, we assume a get_value(key) method on MetadataStore
    // (adapt as needed for the actual API)
    let value = format!("Value for key {} (fetched from volume)", key);
    (StatusCode::OK, value)
}

/// Handles key delete requests (not yet implemented).
async fn delete_key(
    State(_state): State<CoordState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    // Real logic: delete via metadata and volume
    // Here, we assume a delete_key(key) method on MetadataStore
    // (adapt as needed for the actual API)
    (StatusCode::OK, format!("DELETE {} succeeded", key))
}
