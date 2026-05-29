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
use config::{settings::Settings, state::AppState};

pub use config::state::AppState as AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("invoice_service=debug,tower_http=debug")
        .init();

    let settings = Settings::from_env();
    let addr = settings.server_addr();

    let pool = config::database::connect(&settings).await;
    let state = Arc::new(AppState::new(pool, settings));

    tokio::spawn(workers::webhook::run(state.clone()));

    let app = router::build(state);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    tracing::info!("Server running on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
