+use axum::{routing::post, http::StatusCode, Json, Router};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Deserialize)]
struct ChargeRequest {
    amount_cents: i64,
    token: String,
}

#[derive(Serialize)]
struct ChargeResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    psp_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

async fn charge(Json(req): Json<ChargeRequest>) -> Result<Json<ChargeResponse>, StatusCode> {
    tracing::info!("PSP charge: {} cents, token={}", req.amount_cents, req.token);

    match req.token.as_str() {
        "tok_success" => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse {
                status: "succeeded".into(),
                psp_ref: Some(uuid::Uuid::new_v4().to_string()),
                code: None,
            }))
        }
        "tok_insufficient_funds" => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse {
                status: "failed".into(),
                psp_ref: None,
                code: Some("insufficient_funds".into()),
            }))
        }
        "tok_card_declined" => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(Json(ChargeResponse {
                status: "failed".into(),
                psp_ref: None,
                code: Some("card_declined".into()),
            }))
        }
        "tok_timeout" => {
            // Sleeps 30s then returns success (our service times out at 10s)
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(Json(ChargeResponse {
                status: "succeeded".into(),
                psp_ref: Some(uuid::Uuid::new_v4().to_string()),
                code: None,
            }))
        }
        "tok_network_error" => {
            // Returns HTTP 500 — simulates PSP infrastructure failure
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
        _ => {
            Ok(Json(ChargeResponse {
                status: "failed".into(),
                psp_ref: None,
                code: Some("unknown_token".into()),
            }))
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("mock_psp=debug")
        .init();

    let app = Router::new().route("/charge", post(charge));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8081").await.unwrap();
    tracing::info!("Mock PSP running on :8081");
    axum::serve(listener, app).await.unwrap();
}
