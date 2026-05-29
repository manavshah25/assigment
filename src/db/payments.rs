use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use crate::models::payment::PaymentAttempt;

pub async fn insert_attempt(
    tx: &mut Transaction<'_, Postgres>,
    invoice_id: Uuid,
    amount_cents: i64,
    payment_token: &str,
) -> Result<PaymentAttempt, sqlx::Error> {
    sqlx::query_as::<_, PaymentAttempt>(
        "INSERT INTO payment_attempts (invoice_id, status, amount_cents, payment_token)
         VALUES ($1, 'processing', $2, $3)
         RETURNING id, invoice_id, status, amount_cents, payment_token, psp_transaction_id, failure_reason, created_at, updated_at"
    )
    .bind(invoice_id)
    .bind(amount_cents)
    .bind(payment_token)
    .fetch_one(&mut **tx)
    .await
}

pub async fn update_attempt(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    status: &str,
    psp_txn_id: Option<&str>,
    failure_reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE payment_attempts SET status = $1::payment_status, psp_transaction_id = $2, failure_reason = $3, updated_at = now() WHERE id = $4"
    )
    .bind(status).bind(psp_txn_id).bind(failure_reason).bind(id)
    .execute(&mut **tx).await?;
    Ok(())
}

pub async fn find_idempotency(
    pool: &PgPool,
    business_id: Uuid,
    key: &str,
) -> Result<Option<(String, serde_json::Value)>, sqlx::Error> {
    sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT request_hash, response_body FROM idempotency_keys WHERE business_id = $1 AND key = $2 AND expires_at > now()"
    )
    .bind(business_id).bind(key)
    .fetch_optional(pool).await
}

pub async fn store_idempotency(
    pool: &PgPool,
    business_id: Uuid,
    key: &str,
    request_path: &str,
    request_hash: &str,
    response_body: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO idempotency_keys (business_id, key, request_path, request_hash, response_status, response_body)
         VALUES ($1, $2, $3, $4, 200, $5) ON CONFLICT (business_id, key) DO NOTHING"
    )
    .bind(business_id).bind(key).bind(request_path).bind(request_hash).bind(response_body)
    .execute(pool).await?;
    Ok(())
}
