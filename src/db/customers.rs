use sqlx::PgPool;
use uuid::Uuid;
use crate::models::customer::Customer;

pub async fn insert(pool: &PgPool, business_id: Uuid, email: &str, name: &str) -> Result<Customer, sqlx::Error> {
    sqlx::query_as::<_, Customer>(
        "INSERT INTO customers (business_id, email, name) VALUES ($1, $2, $3)
         RETURNING id, business_id, email, name, created_at"
    )
    .bind(business_id)
    .bind(email)
    .bind(name)
    .fetch_one(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid, business_id: Uuid) -> Result<Option<Customer>, sqlx::Error> {
    sqlx::query_as::<_, Customer>(
        "SELECT id, business_id, email, name, created_at FROM customers WHERE id = $1 AND business_id = $2"
    )
    .bind(id)
    .bind(business_id)
    .fetch_optional(pool)
    .await
}

pub async fn list(pool: &PgPool, business_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Customer>, sqlx::Error> {
    sqlx::query_as::<_, Customer>(
        "SELECT id, business_id, email, name, created_at FROM customers
         WHERE business_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
    )
    .bind(business_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn exists(pool: &PgPool, id: Uuid, business_id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND business_id = $2)"
    )
    .bind(id)
    .bind(business_id)
    .fetch_one(pool)
    .await
}
