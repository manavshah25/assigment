use sqlx::PgPool;
use crate::config::settings::Settings;

pub struct AppState {
    pub db: PgPool,
    pub settings: Settings,
}

impl AppState {
    pub fn new(db: PgPool, settings: Settings) -> Self {
        Self { db, settings }
    }
}
