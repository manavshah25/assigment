use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::models::invoice::{Invoice, LineItem, InvoiceStatus};

pub async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    business_id: Uuid,
    customer_id: Uuid,
    invoice_number: &str,
    amount_cents: i64,
    currency: &str,
    due_date: Option<DateTime<Utc>>,
) -> Result<Invoice, sqlx::Error> {
    sqlx::query_as::<_, Invoice>(
        "INSERT INTO invoices (business_id, customer_id, invoice_number, amount_cents, currency, due_date)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, business_id, customer_id, invoice_number, status, amount_cents, currency, due_date, created_at, updated_at"
    )
    .bind(business_id)
    .bind(customer_id)
    .bind(invoice_number)
    .bind(amount_cents)
    .bind(currency)
    .bind(due_date)
    .fetch_one(&mut **tx)
    .await
}

pub async fn insert_line_item(
    tx: &mut Transaction<'_, Postgres>,
    invoice_id: Uuid,
    description: &str,
    quantity: i32,
    unit_price_cents: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO line_items (invoice_id, description, quantity, unit_price_cents) VALUES ($1, $2, $3, $4)"
    )
    .bind(invoice_id)
    .bind(description)
    .bind(quantity)
    .bind(unit_price_cents)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn find_by_id(pool: &PgPool, id: Uuid, business_id: Uuid) -> Result<Option<Invoice>, sqlx::Error> {
    sqlx::query_as::<_, Invoice>(
        "SELECT id, business_id, customer_id, invoice_number, status, amount_cents, currency, due_date, created_at, updated_at
         FROM invoices WHERE id = $1 AND business_id = $2"
    )
    .bind(id)
    .bind(business_id)
    .fetch_optional(pool)
    .await
}

pub async fn list(pool: &PgPool, business_id: Uuid, status: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Invoice>, sqlx::Error> {
    if let Some(s) = status {
        sqlx::query_as::<_, Invoice>(
            "SELECT id, business_id, customer_id, invoice_number, status, amount_cents, currency, due_date, created_at, updated_at
             FROM invoices WHERE business_id = $1 AND status = $2::invoice_status ORDER BY created_at DESC LIMIT $3 OFFSET $4"
        )
        .bind(business_id).bind(s).bind(limit).bind(offset)
        .fetch_all(pool).await
    } else {
        sqlx::query_as::<_, Invoice>(
            "SELECT id, business_id, customer_id, invoice_number, status, amount_cents, currency, due_date, created_at, updated_at
             FROM invoices WHERE business_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(business_id).bind(limit).bind(offset)
        .fetch_all(pool).await
    }
}

pub async fn find_line_items(pool: &PgPool, invoice_id: Uuid) -> Result<Vec<LineItem>, sqlx::Error> {
    sqlx::query_as::<_, LineItem>(
        "SELECT id, invoice_id, description, quantity, unit_price_cents, amount_cents FROM line_items WHERE invoice_id = $1"
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await
}

pub async fn lock_for_update(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    business_id: Uuid,
) -> Result<Option<(Uuid, InvoiceStatus, i64, Uuid)>, sqlx::Error> {
    sqlx::query_as::<_, (Uuid, InvoiceStatus, i64, Uuid)>(
        "SELECT id, status, amount_cents, business_id FROM invoices WHERE id = $1 AND business_id = $2 FOR UPDATE"
    )
    .bind(id)
    .bind(business_id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn update_status(tx: &mut Transaction<'_, Postgres>, id: Uuid, status: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE invoices SET status = $1::invoice_status, updated_at = now() WHERE id = $2")
        .bind(status).bind(id).execute(&mut **tx).await?;
    Ok(())
}
