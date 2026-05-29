use sqlx::PgPool;

pub struct AppState {
    pub db: PgPool,
    pub psp_url: String,
}

impl AppState {
    pub fn new(db: PgPool) -> Self {
        let psp_url = std::env::var("PSP_URL")
            .unwrap_or_else(|_| "http://localhost:8081".into());
        Self { db, psp_url }
    }
}
