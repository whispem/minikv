//! Authentication middleware for axum (v0.6.0)
//!
//! This module provides middleware to protect routes with authentication.
//! Supports both API keys and JWT tokens.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::common::auth::{AuthConfig, AuthContext, AuthResult, KeyStore, KEY_STORE};

/// Extension type for passing auth context to handlers
#[derive(Clone, Debug)]
pub struct AuthExtension(pub Option<AuthContext>);

/// State for auth middleware
#[derive(Clone)]
pub struct AuthState {
    pub key_store: Arc<KeyStore>,
    pub config: AuthConfig,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            key_store: KEY_STORE.clone(),
            config: AuthConfig::default(),
        }
    }
}

/// Authentication middleware
/// Validates the Authorization header and adds AuthContext to request extensions
pub async fn auth_middleware(
    State(state): State<AuthState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Skip auth if disabled
    if !state.config.enabled {
        request.extensions_mut().insert(AuthExtension(None));
        return next.run(request).await;
    }

    // Check if path is public
    let path = request.uri().path();
    if state
        .config
        .public_paths
        .iter()
        .any(|p| path.starts_with(p))
    {
        request.extensions_mut().insert(AuthExtension(None));
        return next.run(request).await;
    }

    // Get Authorization header
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    // Also check X-API-Key header as alternative
    let api_key_header = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    let auth_result = if let Some(header) = auth_header {
        state.key_store.authenticate(header)
    } else if let Some(key) = api_key_header {
        state.key_store.validate_key(key)
    } else {
        // Check if reads require auth
        let is_read = matches!(request.method().as_str(), "GET" | "HEAD" | "OPTIONS");
        if is_read && !state.config.require_auth_for_reads {
            request.extensions_mut().insert(AuthExtension(None));
            return next.run(request).await;
        }
        AuthResult::Missing
    };

    match auth_result {
        AuthResult::Ok(ctx) => {
            // Update last used timestamp
            state.key_store.touch_key(&ctx.key_id);
            request.extensions_mut().insert(AuthExtension(Some(ctx)));
            next.run(request).await
        }
        AuthResult::Missing => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Authentication required",
                "hint": "Provide Authorization header with 'Bearer <jwt>' or 'ApiKey <key>'"
            })),
        )
            .into_response(),
        AuthResult::Invalid(msg) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Invalid credentials",
                "message": msg
            })),
        )
            .into_response(),
        AuthResult::Expired => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Credentials expired",
                "hint": "Please generate a new API key or refresh your token"
            })),
        )
            .into_response(),
        AuthResult::Forbidden(msg) => (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Access denied",
                "message": msg
            })),
        )
            .into_response(),
    }
}

/// Require write permission middleware
/// Must be used after auth_middleware
pub async fn require_write_middleware(request: Request<Body>, next: Next) -> Response {
    if let Some(AuthExtension(Some(ref ctx))) = request.extensions().get::<AuthExtension>() {
        if !ctx.can_write() {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "Write permission required",
                    "role": format!("{:?}", ctx.role)
                })),
            )
                .into_response();
        }
    }
    next.run(request).await
}

/// Require admin permission middleware
/// Must be used after auth_middleware
pub async fn require_admin_middleware(request: Request<Body>, next: Next) -> Response {
    if let Some(AuthExtension(Some(ref ctx))) = request.extensions().get::<AuthExtension>() {
        if !ctx.can_admin() {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "Admin permission required",
                    "role": format!("{:?}", ctx.role)
                })),
            )
                .into_response();
        }
    } else {
        // No auth context = no admin access
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Admin permission required",
                "hint": "Authenticate with an admin API key"
            })),
        )
            .into_response();
    }
    next.run(request).await
}

/// Extract tenant from request
/// Returns the tenant from auth context, or "default" if no auth
pub fn get_tenant_from_request(request: &Request<Body>) -> String {
    request
        .extensions()
        .get::<AuthExtension>()
        .and_then(|ext| ext.0.as_ref())
        .map(|ctx| ctx.tenant.clone())
        .unwrap_or_else(|| "default".to_string())
}

/// Check if request has admin access
pub fn is_admin_request(request: &Request<Body>) -> bool {
    request
        .extensions()
        .get::<AuthExtension>()
        .and_then(|ext| ext.0.as_ref())
        .map(|ctx| ctx.can_admin())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_default() {
        let state = AuthState::default();
        assert!(!state.config.enabled);
    }

    #[test]
    fn test_public_paths() {
        let config = AuthConfig::default();
        assert!(config.public_paths.contains(&"/health".to_string()));
        assert!(config.public_paths.contains(&"/metrics".to_string()));
    }
}
