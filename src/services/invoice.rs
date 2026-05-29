use sqlx::PgPool;
use uuid::Uuid;

use crate::db;
use crate::errors::AppError;
use crate::models::invoice::*;
use crate::services::webhooks;

pub async fn create(pool: &PgPool, business_id: Uuid, req: &CreateInvoiceRequest) -> Result<InvoiceResponse, AppError> {
    let total_cents: i64 = req.line_items.iter()
        .map(|li| li.quantity as i64 * li.unit_price_cents)
        .sum();
    let currency = req.currency.as_deref().unwrap_or("usd");
    let invoice_number = req.invoice_number.trim();

    // Verify customer exists
    let exists = db::customers::exists(pool, req.customer_id, business_id).await?;
    if !exists {
        return Err(AppError::not_found("customer_not_found", "Customer does not exist or does not belong to this business"));
    }

    let mut tx = pool.begin().await?;

    let invoice = db::invoices::insert(&mut tx, business_id, req.customer_id, invoice_number, total_cents, currency, req.due_date)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err)
                if db_err.constraint() == Some("invoices_business_id_invoice_number_key") =>
            {
                AppError::conflict("duplicate_invoice_number", format!("Invoice number '{}' already exists", invoice_number))
            }
            other => AppError::Database(other),
        })?;

    for li in &req.line_items {
        db::invoices::insert_line_item(&mut tx, invoice.id, li.description.trim(), li.quantity, li.unit_price_cents).await?;
    }

    webhooks::enqueue(&mut tx, business_id, "invoice.created", &serde_json::json!({
        "event": "invoice.created", "invoice_id": invoice.id,
        "invoice_number": invoice_number,
        "customer_id": req.customer_id, "amount_cents": total_cents, "status": "pending",
    })).await?;

    tx.commit().await?;

    build_response(pool, invoice).await
}

pub async fn get(pool: &PgPool, id: Uuid, business_id: Uuid) -> Result<InvoiceResponse, AppError> {
    let invoice = db::invoices::find_by_id(pool, id, business_id)
        .await?
        .ok_or_else(|| AppError::not_found("invoice_not_found", "Invoice not found"))?;

    build_response(pool, invoice).await
}

pub async fn list(pool: &PgPool, business_id: Uuid, status: Option<&str>, limit: i64, offset: i64) -> Result<Vec<InvoiceResponse>, AppError> {
    // Validate status
    if let Some(s) = status {
        if !["pending", "paid", "failed", "void", "draft"].contains(&s) {
            return Err(AppError::bad_request("invalid_status", format!("Invalid status '{}'. Must be: pending, paid, failed, void", s)));
        }
    }

    let invoices = db::invoices::list(pool, business_id, status, limit, offset).await?;
    let mut results = Vec::with_capacity(invoices.len());
    for inv in invoices {
        results.push(build_response(pool, inv).await?);
    }
    Ok(results)
}

async fn build_response(pool: &PgPool, invoice: Invoice) -> Result<InvoiceResponse, AppError> {
    let items = db::invoices::find_line_items(pool, invoice.id).await?;
    Ok(InvoiceResponse {
        id: invoice.id,
        invoice_number: invoice.invoice_number,
        customer_id: invoice.customer_id,
        status: invoice.status,
        amount_cents: invoice.amount_cents,
        currency: invoice.currency.to_uppercase(),
        due_date: invoice.due_date,
        line_items: items.into_iter().map(|li| LineItemResponse {
            id: li.id, description: li.description, quantity: li.quantity,
            unit_price_cents: li.unit_price_cents, amount_cents: li.amount_cents,
        }).collect(),
        created_at: invoice.created_at,
    })
}
