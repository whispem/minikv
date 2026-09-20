//! HTTP API for the coordinator.
//!
//! Provides REST, S3-compatible APIs.

use std::time::Duration;

use crate::common::auth::{Role, KEY_STORE};
use crate::common::{AuditEventType, AUDIT_LOGGER};
use async_stream::stream;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

pub static WATCH_CHANNEL: Lazy<broadcast::Sender<KeyChangeEvent>> = Lazy::new(|| {
    let (tx, _rx) = broadcast::channel(100);
    tx
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyChangeEvent {
    pub event: String, // "put" | "delete" | "revoke"
    pub key: String,
    pub tenant: Option<String>,
    pub timestamp: i64,
}

pub async fn watch_sse(
) -> Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let mut rx = WATCH_CHANNEL.subscribe();
    let stream = stream! {
        while let Ok(event) = rx.recv().await {
            let data = serde_json::to_string(&event).unwrap();
            yield Ok(axum::response::sse::Event::default().data(data));
        }
    };
    Sse::new(stream)
}

pub async fn watch_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws)
}

async fn handle_ws(mut socket: WebSocket) {
    let mut rx = WATCH_CHANNEL.subscribe();
    while let Ok(event) = rx.recv().await {
        let msg = serde_json::to_string(&event).unwrap();
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}

const VECTOR_INDEX_PATH: &str = "./coord-data/vector_index.json";
static VECTOR_INDEX_LOADED: AtomicBool = AtomicBool::new(false);

static VECTOR_INDEX: Lazy<std::sync::RwLock<HashMap<String, VectorPoint>>> =
    Lazy::new(|| std::sync::RwLock::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VectorPoint {
    id: String,
    values: Vec<f32>,
    metadata: Option<serde_json::Value>,
    updated_at: i64,
}

fn ensure_timeseries_engine() {
    let has_engine = {
        let guard = crate::common::timeseries::TIMESERIES_ENGINE.read().unwrap();
        guard.is_some()
    };

    if !has_engine {
        let config = crate::common::timeseries::TimeseriesConfig {
            enabled: true,
            ..Default::default()
        };
        crate::common::timeseries::init_timeseries(config);
    }
}

fn load_vector_index_if_needed() -> Result<(), String> {
    if VECTOR_INDEX_LOADED.load(Ordering::SeqCst) {
        return Ok(());
    }

    let path = std::path::Path::new(VECTOR_INDEX_PATH);
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read vector index: {}", e))?;
        let parsed: HashMap<String, VectorPoint> = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse vector index: {}", e))?;
        let mut index = VECTOR_INDEX.write().unwrap();
        *index = parsed;
    }

    VECTOR_INDEX_LOADED.store(true, Ordering::SeqCst);
    Ok(())
}

fn persist_vector_index() -> Result<(), String> {
    let path = std::path::Path::new(VECTOR_INDEX_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create vector index directory: {}", e))?;
    }

    let index = VECTOR_INDEX.read().unwrap();
    let content = serde_json::to_string_pretty(&*index)
        .map_err(|e| format!("failed to serialize vector index: {}", e))?;
    std::fs::write(path, content).map_err(|e| format!("failed to write vector index: {}", e))?;

    Ok(())
}

async fn admin_repair(State(_state): State<CoordState>) -> impl IntoResponse {
    let res = crate::ops::repair::repair_cluster("http://localhost:8000", 3, false).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

async fn admin_compact(State(_state): State<CoordState>) -> impl IntoResponse {
    let res = crate::ops::compact::compact_cluster("http://localhost:8000", None).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

async fn admin_verify(State(_state): State<CoordState>) -> impl IntoResponse {
    let res = crate::ops::verify::verify_cluster("http://localhost:8000", false, 16).await;
    match res {
        Ok(report) => axum::Json(json!({ "status": "ok", "report": report })),
        Err(e) => axum::Json(json!({ "status": "error", "error": format!("{}", e) })),
    }
}

async fn admin_scale(State(_state): State<CoordState>) -> impl IntoResponse {
    axum::Json(json!({ "status": "scaling triggered" }))
}

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::coordinator::metadata::MetadataStore;
use crate::coordinator::objects::{ObjectError, ObjectStore, VolumeHeartbeat};
use crate::coordinator::placement::PlacementManager;
use crate::coordinator::raft_node::{RaftNode, RaftRole};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::{Response, Sse};

impl IntoResponse for ObjectError {
    fn into_response(self) -> Response {
        let status = match &self {
            ObjectError::NotFound => StatusCode::NOT_FOUND,
            ObjectError::NotLeader(_) | ObjectError::NoVolumes | ObjectError::Consensus(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            ObjectError::Volume(_) => StatusCode::BAD_GATEWAY,
            ObjectError::Metadata(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let mut response = (status, self.to_string()).into_response();
        if let ObjectError::NotLeader(Some(leader)) = &self {
            if let Ok(value) = axum::http::HeaderValue::from_str(leader) {
                response.headers_mut().insert("x-minikv-leader", value);
            }
        }
        response
    }
}

#[derive(Debug, Deserialize)]
struct CreateKeyRequest {
    name: String,
    #[serde(default = "default_tenant")]
    tenant: String,
    #[serde(default)]
    role: String,
    expires_in_secs: Option<u64>,
}

fn default_tenant() -> String {
    "default".to_string()
}

#[derive(Debug, Serialize)]
struct CreateKeyResponse {
    id: String,
    /// The plaintext API key (shown only once!)
    key: String,
    tenant: String,
    role: String,
    warning: String,
}

async fn admin_create_key(axum::Json(req): axum::Json<CreateKeyRequest>) -> impl IntoResponse {
    let role = match req.role.to_lowercase().as_str() {
        "admin" => Role::Admin,
        "read_write" | "readwrite" | "rw" => Role::ReadWrite,
        "read_only" | "readonly" | "ro" | "" => Role::ReadOnly,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(json!({
                    "error": "Invalid role",
                    "valid_roles": ["admin", "read_write", "read_only"]
                })),
            )
                .into_response();
        }
    };

    let expires_in = req.expires_in_secs.map(Duration::from_secs);

    match KEY_STORE.generate_key(&req.name, &req.tenant, role, expires_in) {
        Ok((id, key)) => {
            let response = CreateKeyResponse {
                id: id.clone(),
                key,
                tenant: req.tenant.clone(),
                role: format!("{:?}", role),
                warning: "Store this key securely - it cannot be retrieved again!".to_string(),
            };
            AUDIT_LOGGER.log_event(
                AuditEventType::ApiKeyCreated,
                req.name.clone(),
                Some(id.clone()),
                format!(
                    "API key created for tenant {} with role {:?}",
                    req.tenant.clone(),
                    role
                ),
                None,
            );
            (StatusCode::CREATED, axum::Json(json!(response))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Query parameter: `?tenant=<tenant>` to filter by tenant.
#[derive(Debug, Deserialize)]
struct ListKeysQuery {
    tenant: Option<String>,
}

async fn admin_list_keys(Query(query): Query<ListKeysQuery>) -> impl IntoResponse {
    let keys = if let Some(tenant) = query.tenant {
        KEY_STORE.list_keys_for_tenant(&tenant)
    } else {
        KEY_STORE.list_keys()
    };

    let safe_keys: Vec<serde_json::Value> = keys
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "name": k.name,
                "tenant": k.tenant,
                "role": format!("{:?}", k.role),
                "active": k.active,
                "created_at": k.created_at,
                "expires_at": k.expires_at,
                "last_used_at": k.last_used_at,
            })
        })
        .collect();

    axum::Json(json!({
        "keys": safe_keys,
        "total": safe_keys.len()
    }))
}

async fn admin_get_key(Path(key_id): Path<String>) -> impl IntoResponse {
    match KEY_STORE.get_key(&key_id) {
        Some(k) => (
            StatusCode::OK,
            axum::Json(json!({
                "id": k.id,
                "name": k.name,
                "tenant": k.tenant,
                "role": format!("{:?}", k.role),
                "active": k.active,
                "created_at": k.created_at,
                "expires_at": k.expires_at,
                "last_used_at": k.last_used_at,
            })),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": "Key not found" })),
        )
            .into_response(),
    }
}

async fn admin_revoke_key(Path(key_id): Path<String>) -> impl IntoResponse {
    match KEY_STORE.revoke_key(&key_id) {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::ApiKeyRevoked,
                "admin",
                Some(key_id.clone()),
                "API key revoked",
                None,
            );
            let _ = WATCH_CHANNEL.send(KeyChangeEvent {
                event: "revoke".to_string(),
                key: key_id.clone(),
                tenant: None,
                timestamp: chrono::Utc::now().timestamp(),
            });
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "revoked", "id": key_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

async fn admin_delete_key(Path(key_id): Path<String>) -> impl IntoResponse {
    match KEY_STORE.delete_key(&key_id) {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::ApiKeyDeleted,
                "admin",
                Some(key_id.clone()),
                "API key deleted",
                None,
            );
            let _ = WATCH_CHANNEL.send(KeyChangeEvent {
                event: "delete".to_string(),
                key: key_id.clone(),
                tenant: None,
                timestamp: chrono::Utc::now().timestamp(),
            });
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "deleted", "id": key_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

#[derive(Clone)]
pub struct CoordState {
    pub metadata: Arc<MetadataStore>,
    pub placement: Arc<std::sync::Mutex<PlacementManager>>,
    pub raft: Arc<RaftNode>,
    pub objects: ObjectStore,
}

fn notify_change(event: &str, key: &str) {
    let _ = WATCH_CHANNEL.send(KeyChangeEvent {
        event: event.to_string(),
        key: key.to_string(),
        tenant: Some("default".to_string()),
        timestamp: chrono::Utc::now().timestamp(),
    });
}

async fn s3_put_object(
    State(state): State<CoordState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let full_key = format!("{}/{}", bucket, key);

    let ttl_secs: Option<u64> = headers
        .get("X-Minikv-TTL")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    match state.objects.put(&full_key, body.to_vec()).await {
        Ok(stored) => {
            notify_change("put", &full_key);
            let ttl_info = ttl_secs
                .map(|t| format!(", TTL: {}s", t))
                .unwrap_or_default();
            (
                StatusCode::OK,
                format!(
                    "PUT S3 {}/{} committed via 2PC on {} volume(s) ({} bytes{})",
                    bucket,
                    key,
                    stored.replicas.len(),
                    stored.size,
                    ttl_info
                ),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn s3_get_object(
    State(state): State<CoordState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    let full_key = format!("{}/{}", bucket, key);
    match state.objects.get(&full_key).await {
        Ok(data) => (StatusCode::OK, data).into_response(),
        Err(ObjectError::NotFound) => (
            StatusCode::NOT_FOUND,
            format!("S3 object {}/{} not found", bucket, key),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn s3_delete_object(
    State(state): State<CoordState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    let full_key = format!("{}/{}", bucket, key);
    match state.objects.delete(&full_key).await {
        Ok(()) => {
            notify_change("delete", &full_key);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(ObjectError::NotFound) => (
            StatusCode::NOT_FOUND,
            format!("S3 object {}/{} not found", bucket, key),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn kv_put(State(state): State<CoordState>, Path(key): Path<String>, body: Bytes) -> Response {
    match state.objects.put(&key, body.to_vec()).await {
        Ok(stored) => {
            notify_change("put", &key);
            (
                StatusCode::OK,
                format!(
                    "PUT {} committed via 2PC on {} volume(s) ({} bytes)",
                    key,
                    stored.replicas.len(),
                    stored.size
                ),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn kv_get(State(state): State<CoordState>, Path(key): Path<String>) -> Response {
    match state.objects.get(&key).await {
        Ok(data) => (StatusCode::OK, data).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn kv_delete(State(state): State<CoordState>, Path(key): Path<String>) -> Response {
    match state.objects.delete(&key).await {
        Ok(()) => {
            notify_change("delete", &key);
            (StatusCode::OK, format!("DELETE {} succeeded", key)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn volume_heartbeat(
    State(state): State<CoordState>,
    axum::Json(heartbeat): axum::Json<VolumeHeartbeat>,
) -> Response {
    match state.objects.record_heartbeat(heartbeat) {
        Ok(()) => (
            StatusCode::OK,
            axum::Json(json!({ "ok": true, "leader": state.raft.get_leader() })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "ok": false, "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub fn create_router(state: CoordState) -> Router {
    Router::new()
        .route("/watch/sse", axum::routing::get(watch_sse))
        .route("/watch/ws", axum::routing::get(watch_ws))
        .route(
            "/s3/:bucket/:key",
            axum::routing::put(s3_put_object)
                .get(s3_get_object)
                .delete(s3_delete_object),
        )
        .route(
            "/:key",
            axum::routing::get(kv_get)
                .put(kv_put)
                .post(kv_put)
                .delete(kv_delete),
        )
        .route(
            "/internal/volumes/heartbeat",
            axum::routing::post(volume_heartbeat),
        )
        .route("/admin/repair", axum::routing::post(admin_repair))
        .route("/admin/compact", axum::routing::post(admin_compact))
        .route("/admin/verify", axum::routing::post(admin_verify))
        .route("/admin/scale", axum::routing::post(admin_scale))
        .route("/admin/status", axum::routing::get(admin_status))
        .route("/health/ready", axum::routing::get(health_ready))
        .route("/health/live", axum::routing::get(health_live))
        .route("/admin/keys", axum::routing::post(admin_create_key))
        .route("/admin/keys", axum::routing::get(admin_list_keys))
        .route("/admin/keys/:key_id", axum::routing::get(admin_get_key))
        .route(
            "/admin/keys/:key_id/revoke",
            axum::routing::post(admin_revoke_key),
        )
        .route(
            "/admin/keys/:key_id",
            axum::routing::delete(admin_delete_key),
        )
        .route("/admin/import", axum::routing::post(admin_import))
        .route("/admin/export", axum::routing::get(admin_export))
        .route("/transaction", axum::routing::post(transaction_ops))
        .route("/search", axum::routing::get(search_keys))
        .route("/metrics", axum::routing::get(metrics))
        .route("/range", axum::routing::get(range_query))
        .route("/batch", axum::routing::post(batch_ops))
        .route("/admin/ui", axum::routing::get(admin_ui_handler))
        .route("/admin/ui/*path", axum::routing::get(admin_ui_handler))
        .route("/admin/backup", axum::routing::post(admin_create_backup))
        .route("/admin/backups", axum::routing::get(admin_list_backups))
        .route(
            "/admin/backups/:backup_id",
            axum::routing::get(admin_get_backup),
        )
        .route(
            "/admin/backups/:backup_id",
            axum::routing::delete(admin_delete_backup),
        )
        .route("/admin/restore", axum::routing::post(admin_restore))
        .route(
            "/admin/replication/status",
            axum::routing::get(admin_replication_status),
        )
        .route("/admin/plugins", axum::routing::get(admin_list_plugins))
        .route(
            "/admin/plugins/:plugin_id/enable",
            axum::routing::post(admin_enable_plugin),
        )
        .route(
            "/admin/plugins/:plugin_id/disable",
            axum::routing::post(admin_disable_plugin),
        )
        .route("/admin/cdc/status", axum::routing::get(admin_cdc_status))
        .route(
            "/admin/timeseries/stats",
            axum::routing::get(admin_timeseries_stats),
        )
        .route("/admin/geo/status", axum::routing::get(admin_geo_status))
        .route("/ts/write", axum::routing::post(ts_write))
        .route("/ts/query", axum::routing::post(ts_query))
        .route("/ts/query", axum::routing::get(ts_query_get))
        .route("/vector/upsert", axum::routing::post(vector_upsert))
        .route("/vector/query", axum::routing::post(vector_query))
        .route(
            "/admin/vector/stats",
            axum::routing::get(admin_vector_stats),
        )
        .with_state(state)
}

async fn health_ready(State(state): State<CoordState>) -> impl IntoResponse {
    let volumes = state.objects.live_volumes();
    let leader = state.raft.get_leader();

    if !volumes.is_empty() && leader.is_some() {
        (
            StatusCode::OK,
            axum::Json(json!({
                "ready": true,
                "healthy_volumes": volumes.len(),
                "is_leader": state.raft.is_leader(),
                "leader": leader,
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({
                "ready": false,
                "healthy_volumes": volumes.len(),
                "is_leader": state.raft.is_leader(),
                "leader": leader,
                "reason": if volumes.is_empty() { "No healthy volumes" } else { "No Raft leader" }
            })),
        )
    }
}

async fn health_live() -> impl IntoResponse {
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

async fn admin_status(State(state): State<CoordState>) -> impl IntoResponse {
    let role = match state.raft.get_role() {
        RaftRole::Leader => "Leader",
        RaftRole::Candidate => "Candidate",
        RaftRole::Follower => "Follower",
    };
    let nb_peers = state.raft.get_peers().len();
    let volumes = state.objects.live_volumes();
    let nb_volumes = volumes.len();
    let volume_ids: Vec<_> = volumes.iter().map(|v| v.volume_id.clone()).collect();
    let nb_s3_objects = state.metadata.list_keys().map(|k| k.len()).unwrap_or(0);
    axum::Json(json!({
        "node_id": state.raft.node_id(),
        "role": role,
        "is_leader": role == "Leader",
        "leader": state.raft.get_leader(),
        "term": state.raft.get_term(),
        "commit_index": state.raft.commit_index(),
        "last_applied": state.raft.last_applied(),
        "log_length": state.raft.log_len(),
        "nb_peers": nb_peers,
        "nb_volumes": nb_volumes,
        "volume_ids": volume_ids,
        "nb_s3_objects": nb_s3_objects
    }))
}

#[derive(Deserialize)]
struct ImportRequest {
    entries: Vec<KeyValueEntry>,
}

#[derive(Deserialize)]
struct KeyValueEntry {
    key: String,
    value: String,
}

async fn admin_import(
    State(state): State<CoordState>,
    axum::Json(req): axum::Json<ImportRequest>,
) -> impl IntoResponse {
    let mut success_count = 0;
    let mut errors: Vec<String> = Vec::new();

    for entry in req.entries {
        match state
            .objects
            .put(&entry.key, entry.value.into_bytes())
            .await
        {
            Ok(_) => success_count += 1,
            Err(e) => errors.push(format!("{}: {}", entry.key, e)),
        }
    }

    AUDIT_LOGGER.log_event(
        AuditEventType::System,
        "admin".to_string(),
        None,
        format!("Imported {} keys", success_count),
        None,
    );

    axum::Json(json!({
        "imported": success_count,
        "errors": errors
    }))
}

async fn admin_export(State(state): State<CoordState>) -> impl IntoResponse {
    let keys = match state.metadata.list_keys() {
        Ok(keys) => keys,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("list_keys error: {}", e),
            )
                .into_response();
        }
    };

    let objects = state.objects.clone();
    let body = stream! {
        for key in keys {
            if let Ok(value) = objects.get(&key).await {
                let entry = json!({
                    "key": key,
                    "value": String::from_utf8_lossy(&value)
                });
                yield Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(format!("{}\n", entry)));
            }
        }
    };

    (
        StatusCode::OK,
        [("content-type", "application/x-ndjson")],
        axum::body::Body::from_stream(body),
    )
        .into_response()
}

#[derive(Deserialize)]
struct TransactionRequest {
    operations: Vec<Operation>,
}

#[derive(Deserialize)]
struct Operation {
    op: String, // "put" or "delete"
    key: String,
    value: Option<String>,
}

async fn transaction_ops(
    State(state): State<CoordState>,
    axum::Json(req): axum::Json<TransactionRequest>,
) -> impl IntoResponse {
    let mut results = Vec::new();
    let mut success_count = 0;
    let total_operations = req.operations.len();

    for op in &req.operations {
        match op.op.as_str() {
            "put" => {
                if let Some(ref value) = op.value {
                    let outcome = state.objects.put(&op.key, value.clone().into_bytes()).await;
                    if outcome.is_ok() {
                        success_count += 1;
                    }
                    results.push(TransactionResult {
                        op: op.op.clone(),
                        key: op.key.clone(),
                        success: outcome.is_ok(),
                        error: outcome.err().map(|e| e.to_string()),
                    });
                } else {
                    results.push(TransactionResult {
                        op: op.op.clone(),
                        key: op.key.clone(),
                        success: false,
                        error: Some("value required for put".to_string()),
                    });
                }
            }
            "delete" => {
                let outcome = match state.objects.delete(&op.key).await {
                    Err(ObjectError::NotFound) => Ok(()),
                    other => other,
                };
                if outcome.is_ok() {
                    success_count += 1;
                }
                results.push(TransactionResult {
                    op: op.op.clone(),
                    key: op.key.clone(),
                    success: outcome.is_ok(),
                    error: outcome.err().map(|e| e.to_string()),
                });
            }
            _ => {
                results.push(TransactionResult {
                    op: op.op.clone(),
                    key: op.key.clone(),
                    success: false,
                    error: Some("unknown operation".to_string()),
                });
            }
        }
    }

    AUDIT_LOGGER.log_event(
        AuditEventType::System,
        "transaction".to_string(),
        None,
        format!("Executed {} operations in transaction", success_count),
        None,
    );

    axum::Json(json!({
        "results": results,
        "total_operations": total_operations,
        "successful_operations": success_count
    }))
}

#[derive(Deserialize)]
struct SearchQuery {
    value: String,
}

async fn search_keys(
    State(state): State<CoordState>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    match state.metadata.list_keys() {
        Ok(keys) => {
            let mut matching_keys = Vec::new();
            for key in keys {
                if let Ok(value_bytes) = state.objects.get(&key).await {
                    if let Ok(value_str) = std::str::from_utf8(&value_bytes) {
                        if value_str.contains(&params.value) {
                            matching_keys.push(key);
                        }
                    }
                }
            }
            axum::Json(json!({
                "query": params.value,
                "matching_keys": matching_keys,
                "total_matches": matching_keys.len()
            }))
        }
        Err(e) => axum::Json(json!({ "error": format!("list_keys error: {}", e) })),
    }
}

#[derive(Serialize)]
struct TransactionResult {
    op: String,
    key: String,
    success: bool,
    error: Option<String>,
}

/// HTTP handler for range queries: GET /range?start=...&end=...&include_values=...
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
                    let r = state.objects.put(&op.key, val.into_bytes()).await;
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
                let r = state.objects.get(&op.key).await;
                match r {
                    Ok(value) => results.push(BatchResultResp {
                        ok: true,
                        key: op.key,
                        value: Some(String::from_utf8_lossy(&value).into_owned()),
                        error: None,
                    }),
                    Err(ObjectError::NotFound) => results.push(BatchResultResp {
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
                let r = state.objects.delete(&op.key).await;
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

pub async fn metrics(State(state): State<CoordState>) -> impl IntoResponse {
    let mut out = String::new();
    let volumes: Vec<crate::coordinator::metadata::VolumeMetadata> = state.objects.live_volumes();
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
    let role = if state.raft.is_leader() {
        "leader"
    } else {
        "follower"
    };
    out += &format!("minikv_raft_role {{}} \"{}\"\n", role);
    out += &format!("minikv_raft_term {{}} {}\n", state.raft.get_term());
    out += &format!(
        "minikv_raft_commit_index {{}} {}\n",
        state.raft.commit_index()
    );

    out += &crate::common::METRICS.to_prometheus();

    let s3_objects = state.metadata.list_keys().map(|k| k.len()).unwrap_or(0);
    let s3_objects_with_ttl = 0;
    out += &format!("minikv_s3_objects_total {{}} {}\n", s3_objects);
    out += &format!("minikv_s3_objects_with_ttl {{}} {}\n", s3_objects_with_ttl);

    (axum::http::StatusCode::OK, out)
}

#[allow(dead_code)]
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

async fn admin_ui_handler() -> impl IntoResponse {
    crate::common::admin_ui::admin_dashboard().await
}

#[derive(Debug, Deserialize)]
struct CreateBackupRequest {
    #[serde(rename = "type", default = "default_backup_type")]
    backup_type: String,
}

fn default_backup_type() -> String {
    "full".to_string()
}

async fn admin_create_backup(
    axum::Json(req): axum::Json<CreateBackupRequest>,
) -> impl IntoResponse {
    let backup_type = match req.backup_type.as_str() {
        "incremental" => crate::common::backup::BackupType::Incremental,
        _ => crate::common::backup::BackupType::Full,
    };

    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        let config = crate::common::backup::BackupConfig::default();
        match manager.start_backup(config, backup_type).await {
            Ok(backup_id) => {
                AUDIT_LOGGER.log_event(
                    AuditEventType::System,
                    "admin".to_string(),
                    Some(backup_id.clone()),
                    format!("Started {:?} backup", backup_type),
                    None,
                );
                (
                    StatusCode::ACCEPTED,
                    axum::Json(json!({
                        "status": "started",
                        "backup_id": backup_id,
                        "type": req.backup_type
                    })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": format!("{}", e) })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

async fn admin_list_backups() -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        let backups = manager.list_backups().await;
        axum::Json(json!({
            "backups": backups,
            "total": backups.len()
        }))
    } else {
        axum::Json(json!({
            "backups": [],
            "total": 0
        }))
    }
}

async fn admin_get_backup(Path(backup_id): Path<String>) -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        match manager.get_backup(&backup_id).await {
            Some(manifest) => (StatusCode::OK, axum::Json(json!(manifest))).into_response(),
            None => (
                StatusCode::NOT_FOUND,
                axum::Json(json!({ "error": "Backup not found" })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

async fn admin_delete_backup(Path(backup_id): Path<String>) -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        match manager.delete_backup(&backup_id).await {
            Ok(()) => {
                AUDIT_LOGGER.log_event(
                    AuditEventType::System,
                    "admin".to_string(),
                    Some(backup_id.clone()),
                    "Deleted backup".to_string(),
                    None,
                );
                (
                    StatusCode::OK,
                    axum::Json(json!({ "status": "deleted", "backup_id": backup_id })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": format!("{}", e) })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct RestoreRequest {
    backup_id: String,
    target_path: Option<String>,
}

async fn admin_restore(axum::Json(req): axum::Json<RestoreRequest>) -> impl IntoResponse {
    let guard = crate::common::backup::BACKUP_MANAGER.read().await;
    if let Some(ref manager) = *guard {
        let config = crate::common::backup::RestoreConfig {
            backup_id: req.backup_id.clone(),
            source: crate::common::backup::BackupDestination::Local {
                path: "./backups".to_string(),
            },
            target_path: req.target_path.unwrap_or_else(|| "./restore".to_string()),
            decryption_key: None,
            point_in_time: None,
            parallel_workers: 4,
            verify_checksums: true,
        };

        match manager.start_restore(config).await {
            Ok(restore_id) => {
                AUDIT_LOGGER.log_event(
                    AuditEventType::System,
                    "admin".to_string(),
                    Some(req.backup_id.clone()),
                    format!("Started restore from backup {}", req.backup_id),
                    None,
                );
                (
                    StatusCode::ACCEPTED,
                    axum::Json(json!({
                        "status": "started",
                        "restore_id": restore_id,
                        "backup_id": req.backup_id
                    })),
                )
                    .into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": format!("{}", e) })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({ "error": "Backup manager not initialized" })),
        )
            .into_response()
    }
}

async fn admin_replication_status() -> impl IntoResponse {
    let guard = crate::common::replication::REPLICATION_MANAGER
        .read()
        .unwrap();
    if let Some(ref manager) = *guard {
        let config = manager.config();
        let status = manager.get_status();
        let healthy = manager.is_healthy();

        axum::Json(json!({
            "enabled": true,
            "local_dc": config.local_dc,
            "conflict_resolution": format!("{:?}", config.conflict_resolution),
            "async_replication": config.async_replication,
            "healthy": healthy,
            "remote_dcs": status.iter().map(|s| json!({
                "dc_id": s.dc_id,
                "healthy": s.healthy,
                "lag_secs": s.lag_secs,
                "pending_events": s.pending_events,
                "last_replicated_at": s.last_replicated_at,
                "last_error": s.last_error
            })).collect::<Vec<_>>()
        }))
    } else {
        axum::Json(json!({
            "enabled": false,
            "message": "Replication not configured"
        }))
    }
}

async fn admin_list_plugins() -> impl IntoResponse {
    let plugins = crate::common::plugin::get_plugin_manager()
        .list_plugins()
        .await;

    let plugin_list: Vec<serde_json::Value> = plugins
        .iter()
        .map(|(info, state)| {
            json!({
                "id": info.id,
                "name": info.name,
                "description": info.description,
                "version": info.version.to_string(),
                "author": info.author,
                "plugin_type": format!("{:?}", info.plugin_type),
                "state": format!("{:?}", state)
            })
        })
        .collect();

    axum::Json(json!({
        "plugins": plugin_list,
        "total": plugin_list.len()
    }))
}

async fn admin_enable_plugin(Path(plugin_id): Path<String>) -> impl IntoResponse {
    match crate::common::plugin::get_plugin_manager()
        .enable(&plugin_id)
        .await
    {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::System,
                "admin".to_string(),
                Some(plugin_id.clone()),
                "Enabled plugin".to_string(),
                None,
            );
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "enabled", "plugin_id": plugin_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

async fn admin_disable_plugin(Path(plugin_id): Path<String>) -> impl IntoResponse {
    match crate::common::plugin::get_plugin_manager()
        .disable(&plugin_id)
        .await
    {
        Ok(()) => {
            AUDIT_LOGGER.log_event(
                AuditEventType::System,
                "admin".to_string(),
                Some(plugin_id.clone()),
                "Disabled plugin".to_string(),
                None,
            );
            (
                StatusCode::OK,
                axum::Json(json!({ "status": "disabled", "plugin_id": plugin_id })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

async fn admin_cdc_status() -> impl IntoResponse {
    let (enabled, sequence) = {
        let guard = crate::common::cdc::CDC_MANAGER.read().unwrap();
        if let Some(ref manager) = *guard {
            (true, manager.current_sequence())
        } else {
            (false, 0)
        }
    };

    if enabled {
        axum::Json(json!({
            "enabled": true,
            "current_sequence": sequence,
            "sinks": []
        }))
    } else {
        axum::Json(json!({
            "enabled": false,
            "message": "CDC not configured"
        }))
    }
}

async fn admin_timeseries_stats() -> impl IntoResponse {
    ensure_timeseries_engine();
    let guard = crate::common::timeseries::TIMESERIES_ENGINE.read().unwrap();

    let stats = guard.as_ref().map(|engine| engine.stats()).unwrap_or(
        crate::common::timeseries::TimeseriesStats {
            total_series: 0,
            total_points: 0,
            total_bytes: 0,
            retention_days: 30,
            downsample_rules: 0,
        },
    );

    axum::Json(json!({
        "enabled": true,
        "resolutions": ["raw", "1min", "5min", "1hour", "1day"],
        "compression": ["delta", "gorilla"],
        "aggregations": ["sum", "avg", "min", "max", "count", "stddev"],
        "stats": stats,
    }))
}

async fn admin_geo_status() -> impl IntoResponse {
    axum::Json(json!({
        "enabled": false,
        "local_region": null,
        "remote_regions": [],
        "routing_strategy": "nearest",
    }))
}

#[derive(Debug, Deserialize)]
struct TsPointInput {
    timestamp: i64,
    value: f64,
}

#[derive(Debug, Deserialize)]
struct TsWriteRequest {
    metric: String,
    #[serde(default)]
    tags: HashMap<String, String>,
    points: Vec<TsPointInput>,
}

#[derive(Debug, Deserialize)]
struct TsQueryRequest {
    metric: String,
    start: Option<String>,
    end: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
    aggregation: Option<crate::common::timeseries::Aggregation>,
    resolution: Option<crate::common::timeseries::Resolution>,
    limit: Option<usize>,
}

fn parse_ts_datetime(input: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(input)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| format!("invalid datetime '{}': {}", input, e))
}

fn build_ts_query(
    req: TsQueryRequest,
) -> Result<crate::common::timeseries::TimeseriesQuery, String> {
    let end = match req.end {
        Some(v) => parse_ts_datetime(&v)?,
        None => Utc::now(),
    };
    let start = match req.start {
        Some(v) => parse_ts_datetime(&v)?,
        None => end - ChronoDuration::hours(1),
    };

    Ok(crate::common::timeseries::TimeseriesQuery {
        metric: req.metric,
        start,
        end,
        tags: req.tags,
        aggregation: req.aggregation,
        resolution: req.resolution,
        limit: req.limit,
    })
}

async fn ts_write(axum::Json(req): axum::Json<TsWriteRequest>) -> impl IntoResponse {
    if req.metric.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": "metric is required" })),
        )
            .into_response();
    }

    ensure_timeseries_engine();

    let mut series = crate::common::timeseries::TimeSeries::new(&req.metric);
    series.tags = req.tags;
    series.points = req
        .points
        .into_iter()
        .map(|p| crate::common::timeseries::DataPoint::new(p.timestamp, p.value))
        .collect();

    let guard = crate::common::timeseries::TIMESERIES_ENGINE.read().unwrap();
    let engine = match guard.as_ref() {
        Some(engine) => engine,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(json!({ "error": "timeseries engine unavailable" })),
            )
                .into_response();
        }
    };

    match engine.write(&series) {
        Ok(()) => (
            StatusCode::OK,
            axum::Json(json!({
                "success": true,
                "metric": series.metric,
                "points_written": series.points.len()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": format!("timeseries write failed: {}", e) })),
        )
            .into_response(),
    }
}

async fn ts_query(axum::Json(req): axum::Json<TsQueryRequest>) -> impl IntoResponse {
    ensure_timeseries_engine();

    let query = match build_ts_query(req) {
        Ok(q) => q,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, axum::Json(json!({ "error": e }))).into_response();
        }
    };

    let guard = crate::common::timeseries::TIMESERIES_ENGINE.read().unwrap();
    let engine = match guard.as_ref() {
        Some(engine) => engine,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(json!({ "error": "timeseries engine unavailable" })),
            )
                .into_response();
        }
    };

    match engine.query(&query) {
        Ok(result) => (
            StatusCode::OK,
            axum::Json(json!({
                "success": true,
                "series": result.series,
                "points": result
                    .series
                    .iter()
                    .flat_map(|s| s.points.iter().cloned())
                    .collect::<Vec<_>>(),
                "execution_time_ms": result.execution_time_ms,
                "points_scanned": result.points_scanned,
                "points_returned": result.points_returned
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": format!("timeseries query failed: {}", e) })),
        )
            .into_response(),
    }
}

async fn ts_query_get(Query(req): Query<TsQueryRequest>) -> impl IntoResponse {
    ts_query(axum::Json(req)).await
}

#[derive(Debug, Deserialize)]
struct VectorUpsertRequest {
    id: String,
    values: Vec<f32>,
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct VectorQueryRequest {
    vector: Vec<f32>,
    top_k: Option<usize>,
}

#[derive(Debug, Serialize)]
struct VectorMatch {
    id: String,
    score: f32,
    metadata: Option<serde_json::Value>,
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return None;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return None;
    }
    Some(dot / (norm_a.sqrt() * norm_b.sqrt()))
}

async fn vector_upsert(axum::Json(req): axum::Json<VectorUpsertRequest>) -> impl IntoResponse {
    if let Err(e) = load_vector_index_if_needed() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": e })),
        )
            .into_response();
    }

    if req.id.trim().is_empty() || req.values.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({
                "error": "id and non-empty values are required"
            })),
        )
            .into_response();
    }

    let point = VectorPoint {
        id: req.id.clone(),
        values: req.values,
        metadata: req.metadata,
        updated_at: chrono::Utc::now().timestamp(),
    };

    let mut index = VECTOR_INDEX.write().unwrap();
    index.insert(req.id.clone(), point);
    drop(index);

    if let Err(e) = persist_vector_index() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": e })),
        )
            .into_response();
    }

    let index = VECTOR_INDEX.read().unwrap();

    (
        StatusCode::OK,
        axum::Json(json!({
            "status": "upserted",
            "id": req.id,
            "total_vectors": index.len()
        })),
    )
        .into_response()
}

async fn vector_query(axum::Json(req): axum::Json<VectorQueryRequest>) -> impl IntoResponse {
    if let Err(e) = load_vector_index_if_needed() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": e })),
        )
            .into_response();
    }

    if req.vector.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({ "error": "vector must be non-empty" })),
        )
            .into_response();
    }

    let top_k = req.top_k.unwrap_or(10).clamp(1, 100);
    let index = VECTOR_INDEX.read().unwrap();

    let mut matches: Vec<VectorMatch> = index
        .values()
        .filter_map(|point| {
            cosine_similarity(&req.vector, &point.values).map(|score| VectorMatch {
                id: point.id.clone(),
                score,
                metadata: point.metadata.clone(),
            })
        })
        .collect();

    matches.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches.truncate(top_k);

    (
        StatusCode::OK,
        axum::Json(json!({
            "matches": matches,
            "top_k": top_k,
            "total_indexed": index.len()
        })),
    )
        .into_response()
}

async fn admin_vector_stats() -> impl IntoResponse {
    if let Err(e) = load_vector_index_if_needed() {
        return axum::Json(json!({
            "enabled": false,
            "error": e
        }));
    }

    let index = VECTOR_INDEX.read().unwrap();
    let dims = index
        .values()
        .next()
        .map(|point| point.values.len())
        .unwrap_or(0);

    axum::Json(json!({
        "enabled": true,
        "index_type": "persistent_flat",
        "vectors": index.len(),
        "dimensions": dims,
        "path": VECTOR_INDEX_PATH
    }))
}
