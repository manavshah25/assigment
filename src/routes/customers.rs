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
use crate::models::customer::{CreateCustomerRequest, CustomerResponse};
use crate::response::{self, ApiResponse};
use crate::services;
use crate::validators;
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/customers", post(create).get(list))
        .route("/customers/", get(list))
        .route("/customers/:id", get(get_one))
}

async fn create(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    ValidJson(req): ValidJson<CreateCustomerRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CustomerResponse>>), AppError> {
    validators::customer::validate_create(&req)?;
    let data = services::customer::create(&state.db, auth.business_id, &req).await?;
    Ok(response::created(data))
}

async fn get_one(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<ApiResponse<CustomerResponse>>, AppError> {
    let Path(id) = id.map_err(|_| AppError::bad_request("invalid_id", "Invalid UUID format for customer_id"))?;
    let data = services::customer::get(&state.db, id, auth.business_id).await?;
    Ok(response::ok(data))
}

#[derive(Deserialize)]
struct ListParams { limit: Option<i64>, offset: Option<i64> }

async fn list(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthenticatedBusiness>,
    Query(params): Query<ListParams>,
) -> Result<Json<ApiResponse<Vec<CustomerResponse>>>, AppError> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);
    let data = services::customer::list(&state.db, auth.business_id, limit, offset).await?;
    Ok(response::ok(data))
}
