use sqlx::{PgPool, postgres::PgPoolOptions};

pub async fn connect() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/invoices".into());

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .expect("Failed to connect to database");

    sqlx::raw_sql(include_str!("../../migrations/001_init.sql"))
        .execute(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}
