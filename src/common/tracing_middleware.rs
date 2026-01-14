//! Structured logging and request tracing middleware (v0.5.0)
//!
//! This module provides:
//! - Unique request ID generation for each request
//! - Structured logging with tracing
//! - Request/response timing metrics

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, Response},
    middleware::Next,
};
use std::net::SocketAddr;
use std::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

/// Header name for request ID
pub const REQUEST_ID_HEADER: &str = "X-Request-ID";

/// Generate a new unique request ID
pub fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}

/// Middleware that adds request ID and structured logging to each request
pub async fn request_tracing_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let start = Instant::now();

    // Generate or extract request ID
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(generate_request_id);

    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();
    let client_ip = addr.ip().to_string();

    // Create a span for this request
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
        client_ip = %client_ip,
    );

    let _guard = span.enter();

    info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        client_ip = %client_ip,
        "Request started"
    );

    // Execute the request
    let mut response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    // Add request ID to response headers
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id.parse().unwrap());

    // Log completion
    if status.is_success() {
        info!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request completed"
        );
    } else if status.is_client_error() {
        warn!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Client error"
        );
    } else {
        warn!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Server error"
        );
    }

    response
}

/// Simpler middleware that just adds request ID without full tracing
pub async fn request_id_middleware(request: Request<Body>, next: Next) -> Response<Body> {
    // Generate or extract request ID
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(generate_request_id);

    let mut response = next.run(request).await;

    // Add request ID to response headers
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, request_id.parse().unwrap());

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_request_id() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();

        // Should be valid UUIDs
        assert!(Uuid::parse_str(&id1).is_ok());
        assert!(Uuid::parse_str(&id2).is_ok());

        // Should be unique
        assert_ne!(id1, id2);
    }
}
