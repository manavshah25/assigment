use axum::{
    extract::{Path, State, rejection::PathRejection},
    http::HeaderMap,
    routing::post,
    Extension, Json, Router,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::AppError;
use crate::extractors::ValidJson;
use crate::middleware::auth::AuthenticatedBusiness;
use crate::models::payment::{PayInvoiceRequest, PaymentResponse};
use crate::response::{self, ApiResponse};
use crate::services::payment;
use crate::validators;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/invoices/:id/pay", post(pay))
}

async fn pay(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    invoice_id: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    ValidJson(req): ValidJson<PayInvoiceRequest>,
) -> Result<Json<ApiResponse<PaymentResponse>>, AppError> {
    let Path(invoice_id) = invoice_id.map_err(|_| AppError::bad_request("invalid_id", "Invalid UUID format for invoice_id"))?;

    validators::payment::validate_pay(&req)?;

    let token = req.payment_token.trim();
    let idempotency_key = headers.get("idempotency-key").and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let request_hash = payment::compute_request_hash(invoice_id, token, auth.business_id);

    if let Some(ref key) = idempotency_key {
        if let Some(cached) = payment::check_idempotency(&state.db, auth.business_id, key, &request_hash).await? {
            return Ok(response::ok(cached));
        }
    }

    let data = payment::execute(&state.db, &state.settings.psp_url, state.settings.psp_timeout_secs, auth.business_id, invoice_id, token).await?;

    if let Some(ref key) = idempotency_key {
        payment::store_idempotency_response(&state.db, auth.business_id, key, invoice_id, &request_hash, &data).await;
    }

    Ok(response::ok(data))
}
