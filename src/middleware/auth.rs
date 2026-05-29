use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

#[derive(Clone)]
pub struct AuthenticatedBusiness {
    pub business_id: Uuid,
}

/// Auth middleware: validates bearer token from Authorization header.
///
/// Supports two token types:
/// 1. Session token (tok_xxx) — issued by POST /api/auth/token, looked up in auth_tokens table
/// 2. API key (sk_test_xxx) — direct key auth, looked up in api_keys table (legacy/fallback)
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, Response> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| unauthorized("Missing Authorization header"))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| unauthorized("Invalid Authorization format, expected: Bearer <token>"))?;

    if token.is_empty() {
        return Err(unauthorized("Token cannot be empty"));
    }

    // Determine token type and validate
    let business_id = if token.starts_with("tok_") {
        // Session token — look up in auth_tokens table
        validate_session_token(&state, token).await?
    } else {
        // API key (sk_test_xxx or sk_live_xxx) — look up in api_keys table
        validate_api_key(&state, token).await?
    };

    req.extensions_mut().insert(AuthenticatedBusiness { business_id });
    Ok(next.run(req).await)
}

/// Validate a session token (tok_xxx) against auth_tokens table
async fn validate_session_token(state: &AppState, token: &str) -> Result<Uuid, Response> {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

    let result = sqlx::query_scalar::<_, Uuid>(
        "SELECT business_id FROM auth_tokens WHERE token_hash = $1 AND expires_at > now()"
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| internal_error())?;

    result.ok_or_else(|| unauthorized("Invalid or expired token"))
}

/// Validate an API key (sk_test_xxx) against api_keys table
async fn validate_api_key(state: &AppState, token: &str) -> Result<Uuid, Response> {
    let secret_part = token.strip_prefix("sk_test_")
        .or_else(|| token.strip_prefix("sk_live_"))
        .unwrap_or(token);

    let mut hasher = Sha256::new();
    hasher.update(secret_part.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    let result = sqlx::query_scalar::<_, Uuid>(
        "SELECT business_id FROM api_keys WHERE key_hash = $1 AND revoked_at IS NULL"
    )
    .bind(&key_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| internal_error())?;

    result.ok_or_else(|| unauthorized("Invalid API key"))
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "status": "error",
            "error": {
                "code": "unauthorized",
                "message": message
            }
        })),
    ).into_response()
}

fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "status": "error",
            "error": {
                "code": "internal_error",
                "message": "Authentication service error"
            }
        })),
    ).into_response()
}
