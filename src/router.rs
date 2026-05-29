use axum::{
    Router,
    middleware as axum_mw,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::config::state::AppState;
use crate::middleware;
use crate::routes;

pub fn build(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api", protected_routes()
            .layer(axum_mw::from_fn_with_state(state.clone(), middleware::auth::auth_middleware)))
        .nest("/api", routes::auth::router())
        // Fallback for unmatched routes
        .fallback(fallback_handler)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn protected_routes() -> Router<Arc<AppState>> {
    Router::new()
        .merge(routes::customers::router())
        .merge(routes::invoices::router())
        .merge(routes::payments::router())
}

async fn fallback_handler() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "status": "error",
            "error": {
                "code": "not_found",
                "message": "The requested endpoint does not exist"
            }
        })),
    )
}
