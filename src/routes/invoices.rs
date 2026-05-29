use axum::{
    extract::{Path, Query, State, rejection::PathRejection},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::AppError;
use crate::extractors::ValidJson;
use crate::middleware::auth::AuthenticatedBusiness;
use crate::models::invoice::{CreateInvoiceRequest, InvoiceResponse};
use crate::response::{self, ApiResponse};
use crate::services;
use crate::validators;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/invoices", post(create).get(list))
        .route("/invoices/", get(list))
        .route("/invoices/:id", get(get_one))
}

async fn create(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    ValidJson(req): ValidJson<CreateInvoiceRequest>,
) -> Result<(StatusCode, Json<ApiResponse<InvoiceResponse>>), AppError> {
    validators::invoice::validate_create(&req)?;
    let data = services::invoice::create(&state.db, auth.business_id, &req).await?;
    Ok(response::created(data))
}

async fn get_one(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<ApiResponse<InvoiceResponse>>, AppError> {
    let Path(id) = id.map_err(|_| AppError::bad_request("invalid_id", "Invalid UUID format for invoice_id"))?;
    let data = services::invoice::get(&state.db, id, auth.business_id).await?;
    Ok(response::ok(data))
}

#[derive(Deserialize)]
struct ListParams { status: Option<String>, limit: Option<i64>, offset: Option<i64> }

async fn list(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Query(params): Query<ListParams>,
) -> Result<Json<ApiResponse<Vec<InvoiceResponse>>>, AppError> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);
    let data = services::invoice::list(&state.db, auth.business_id, params.status.as_deref(), limit, offset).await?;
    Ok(response::ok(data))
}
