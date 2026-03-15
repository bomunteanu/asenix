use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;

// ── JWT claims ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct OwnerClaims {
    pub sub: String, // always "owner"
    pub exp: usize,  // unix timestamp
}

/// Issue a JWT signed with OWNER_SECRET. Expiry: 24 hours.
pub fn issue_owner_jwt(secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (chrono::Utc::now() + chrono::Duration::hours(24))
        .timestamp() as usize;
    let claims = OwnerClaims { sub: "owner".to_string(), exp };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Verify a JWT against OWNER_SECRET.
pub fn verify_owner_jwt(token: &str, secret: &str) -> bool {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_required_spec_claims(&["exp", "sub"]);
    decode::<OwnerClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims.sub == "owner")
    .unwrap_or(false)
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

// ── Middleware: owner JWT required (for /admin/* and /review/*) ───────────────

pub async fn owner_jwt_middleware(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let secret = match std::env::var("OWNER_SECRET") {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "OWNER_SECRET not configured"})),
            )
                .into_response();
        }
    };

    let _ = state; // state available for future use (audit log, etc.)

    match extract_bearer(&headers) {
        Some(token) if verify_owner_jwt(&token, &secret) => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Admin authentication required"})),
        )
            .into_response(),
    }
}

// ── Middleware: IP rate limit (60 req/min, unauthenticated endpoints) ─────────

pub async fn ip_rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let headers = request.headers();

    // /rpc and /mcp carry agent credentials inside the JSON body and have their own
    // per-agent rate limiting — skip the IP-level limiter for these paths entirely.
    let skip = path == "/rpc"
        || path.starts_with("/mcp")
        // Also skip if caller sends explicit agent auth headers (e.g. scripts).
        || (headers.contains_key("x-agent-id") && headers.contains_key("x-api-token"));

    if !skip && !state.ip_rate_limiter.check(addr.ip(), 60, 60) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", "60")],
            Json(json!({"error": "Too many requests. Retry after 60 seconds."})),
        )
            .into_response();
    }

    next.run(request).await
}
