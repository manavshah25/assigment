mod config;
mod db;
mod errors;
mod extractors;
mod middleware;
mod models;
mod response;
mod router;
mod routes;
mod services;
mod validators;
mod workers;

use std::sync::Arc;

// Re-export so all modules can use `crate::AppState`
pub use config::state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("invoice_service=debug,tower_http=debug")
        .init();

    let pool = config::database::connect().await;
    let state = Arc::new(AppState::new(pool));

    tokio::spawn(workers::webhook::run(state.clone()));

    let app = router::build(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    tracing::info!("Server running on :8080");
    axum::serve(listener, app).await.unwrap();
}
