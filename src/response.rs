use axum::{http::StatusCode, Json};
use serde::Serialize;

/// All success responses: {"status": "success", "data": T}
#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub status: &'static str,
    pub data: T,
}

/// 200 OK
pub fn ok<T: Serialize>(data: T) -> Json<ApiResponse<T>> {
    Json(ApiResponse { status: "success", data })
}

/// 201 Created
pub fn created<T: Serialize>(data: T) -> (StatusCode, Json<ApiResponse<T>>) {
    (StatusCode::CREATED, Json(ApiResponse { status: "success", data }))
}
