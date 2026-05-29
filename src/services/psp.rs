use serde::Deserialize;
use std::time::Duration;

#[derive(Debug)]
pub enum PspError {
    Declined(String),
    Timeout,
    NetworkError(String),
}

#[derive(Deserialize)]
struct PspResponse {
    status: String,
    psp_ref: Option<String>,
    code: Option<String>,
}

pub async fn charge(psp_url: &str, amount_cents: i64, token: &str) -> Result<String, PspError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| PspError::NetworkError(e.to_string()))?;

    let response = client
        .post(format!("{}/charge", psp_url))
        .json(&serde_json::json!({"amount_cents": amount_cents, "token": token}))
        .send()
        .await
        .map_err(|e| if e.is_timeout() { PspError::Timeout } else { PspError::NetworkError(e.to_string()) })?;

    if response.status().is_server_error() {
        return Err(PspError::NetworkError(format!("PSP returned {}", response.status())));
    }

    let body: PspResponse = response.json().await.map_err(|e| PspError::NetworkError(e.to_string()))?;

    match body.status.as_str() {
        "succeeded" => Ok(body.psp_ref.unwrap_or_else(|| "unknown".to_string())),
        "failed" => Err(PspError::Declined(body.code.unwrap_or_else(|| "payment_declined".to_string()))),
        _ => Err(PspError::NetworkError(format!("Unknown PSP status: {}", body.status))),
    }
}
