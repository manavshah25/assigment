use sqlx::{PgPool, postgres::PgPoolOptions};
use crate::config::settings::Settings;

pub async fn connect(settings: &Settings) -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(settings.max_db_connections)
        .connect(&settings.database_url)
        .await
        .expect("Failed to connect to database");

    sqlx::raw_sql(include_str!("../../migrations/001_init.sql"))
        .execute(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}
