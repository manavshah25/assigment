use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/auth/token", post(create_token))
}

/// POST /api/auth/token
/// Header: X-API-Key: sk_test_abc123
/// Validates the API key and issues a unique bearer token (24h expiry).
pub async fn create_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Get API key from header
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| error_response(
            StatusCode::BAD_REQUEST,
            "missing_api_key",
            "X-API-Key header is required",
        ))?;

    // Strip prefix
    let secret_part = api_key.strip_prefix("sk_test_")
        .or_else(|| api_key.strip_prefix("sk_live_"))
        .unwrap_or(api_key);

    // Hash the secret
    let mut hasher = Sha256::new();
    hasher.update(secret_part.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    // Look up key (including revoked to give specific error)
    let key_record = sqlx::query_as::<_, (Uuid, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT business_id, revoked_at FROM api_keys WHERE key_hash = $1"
    )
    .bind(&key_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Authentication service unavailable",
    ))?;

    let (business_id, revoked_at) = key_record.ok_or_else(|| error_response(
        StatusCode::UNAUTHORIZED,
        "invalid_api_key",
        "The provided API key is invalid",
    ))?;

    if revoked_at.is_some() {
        return Err(error_response(
            StatusCode::UNAUTHORIZED,
            "api_key_revoked",
            "This API key has been revoked",
        ));
    }

    // Generate unique session token
    let token = format!("tok_{}", Uuid::new_v4().to_string().replace("-", ""));

    // Store hashed token
    let token_hash = {
        let mut h = Sha256::new();
        h.update(token.as_bytes());
        hex::encode(h.finalize())
    };

    sqlx::query(
        "INSERT INTO auth_tokens (business_id, token_hash, expires_at)
         VALUES ($1, $2, now() + INTERVAL '24 hours')"
    )
    .bind(business_id)
    .bind(&token_hash)
    .execute(&state.db)
    .await
    .map_err(|_| error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Failed to issue token",
    ))?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "data": {
            "token": token,
            "token_type": "Bearer",
            "business_id": business_id,
            "expires_in": 86400
        }
    })))
}

fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({
        "status": "error",
        "error": {
            "code": code,
            "message": message
        }
    })))
}
