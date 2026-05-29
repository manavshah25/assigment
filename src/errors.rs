use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

/// All error responses: {"status": "error", "error": {"code": "...", "message": "..."}}
#[derive(Serialize)]
struct ErrorResponse {
    status: &'static str,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

/// Application error enum. Every handler returns Result<T, AppError>.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    BadRequest { code: &'static str, message: String },

    #[error("{message}")]
    NotFound { code: &'static str, message: String },

    #[error("{message}")]
    Conflict { code: &'static str, message: String },

    #[error("{message}")]
    Unprocessable { code: &'static str, message: String },

    #[error("{0}")]
    Internal(String),

    #[error("Database error")]
    Database(#[from] sqlx::Error),
}

impl AppError {
    pub fn bad_request(code: &'static str, msg: impl Into<String>) -> Self {
        Self::BadRequest { code, message: msg.into() }
    }
    pub fn not_found(code: &'static str, msg: impl Into<String>) -> Self {
        Self::NotFound { code, message: msg.into() }
    }
    pub fn conflict(code: &'static str, msg: impl Into<String>) -> Self {
        Self::Conflict { code, message: msg.into() }
    }
    pub fn unprocessable(code: &'static str, msg: impl Into<String>) -> Self {
        Self::Unprocessable { code, message: msg.into() }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match self {
            Self::BadRequest { code, message } => (StatusCode::BAD_REQUEST, code, message),
            Self::NotFound { code, message } => (StatusCode::NOT_FOUND, code, message),
            Self::Conflict { code, message } => (StatusCode::CONFLICT, code, message),
            Self::Unprocessable { code, message } => (StatusCode::UNPROCESSABLE_ENTITY, code, message),
            Self::Internal(msg) => {
                tracing::error!("Internal: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "An internal error occurred".into())
            }
            Self::Database(ref e) => {
                tracing::error!("DB: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "An internal error occurred".into())
            }
        };
        (status, Json(ErrorResponse { status: "error", error: ErrorBody { code, message } })).into_response()
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self { Self::Internal(e.to_string()) }
}
