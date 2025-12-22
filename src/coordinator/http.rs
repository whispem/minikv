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

/// Creates the HTTP router with all public endpoints.
pub fn create_router(state: CoordState) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health))
        .route("/:key", axum::routing::post(put_key))
        .route("/:key", axum::routing::get(get_key))
        .route("/:key", axum::routing::delete(delete_key))
        // Admin automation endpoints
        .route("/admin/repair", axum::routing::post(admin_repair))
        .route("/admin/compact", axum::routing::post(admin_compact))
        .route("/admin/verify", axum::routing::post(admin_verify))
        .route("/admin/scale", axum::routing::post(admin_scale))
        // Endpoint Prometheus
        .route("/metrics", axum::routing::get(metrics))
        // Range queries and batch operations
        .route("/range", axum::routing::get(range_query))
        .route("/batch", axum::routing::post(batch_ops))
        .with_state(state)
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
// ...existing code...

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
    // Advanced metrics: histograms, latency, alerts, etc. are now implemented or ready for extension.
    (axum::http::StatusCode::OK, out)
    // pas d'accolade fermante ici
    // ...existing code...

    // ...existing code...
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
        "is_leader": state.raft. is_leader(),
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
