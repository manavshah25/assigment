use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;

pub struct ValidJson<T>(pub T);

#[axum::async_trait]
impl<S, T> FromRequest<S> for ValidJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ValidJson(value)),
            Err(rejection) => {
                let (code, message) = parse_rejection(&rejection);

                let body = serde_json::json!({
                    "status": "error",
                    "error": {
                        "code": code,
                        "message": message
                    }
                });

                Err((StatusCode::BAD_REQUEST, Json(body)).into_response())
            }
        }
    }
}

fn parse_rejection(rejection: &JsonRejection) -> (&'static str, String) {
    match rejection {
        JsonRejection::JsonDataError(e) => {
            let detail = e.body_text();
            // Parse common serde error patterns into clean messages
            if let Some(field) = extract_missing_field(&detail) {
                ("validation_error", format!("{} is required", field))
            } else if let Some(info) = extract_invalid_type(&detail) {
                ("validation_error", info)
            } else {
                ("validation_error", clean_serde_message(&detail))
            }
        }
        JsonRejection::JsonSyntaxError(_) => {
            ("invalid_json", "Request body contains invalid JSON".to_string())
        }
        JsonRejection::MissingJsonContentType(_) => {
            ("missing_content_type", "Content-Type header must be application/json".to_string())
        }
        _ => {
            ("invalid_request_body", "Invalid request body".to_string())
        }
    }
}

/// Extracts field name from "missing field `field_name`" serde error
fn extract_missing_field(msg: &str) -> Option<String> {
    if let Some(start) = msg.find("missing field `") {
        let rest = &msg[start + 15..];
        if let Some(end) = rest.find('`') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Extracts info from "invalid type" serde error
fn extract_invalid_type(msg: &str) -> Option<String> {
    if msg.contains("invalid type") {
        // e.g. "invalid type: string "abc", expected i32"
        let cleaned = msg.split(" at line").next().unwrap_or(msg);
        return Some(format!("Invalid field value: {}", cleaned));
    }
    None
}

/// Remove "at line X column Y" noise from serde messages
fn clean_serde_message(msg: &str) -> String {
    msg.split(" at line").next().unwrap_or(msg).to_string()
}
