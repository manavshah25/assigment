use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db;
use crate::errors::AppError;
use crate::models::invoice::InvoiceStatus;
use crate::models::payment::*;
use crate::services::{psp, webhooks};

pub fn compute_request_hash(invoice_id: Uuid, token: &str, business_id: Uuid) -> String {
    let mut h = Sha256::new();
    h.update(format!("{}{}{}", invoice_id, token, business_id));
    hex::encode(h.finalize())
}

pub async fn check_idempotency(
    pool: &PgPool, business_id: Uuid, key: &str, request_hash: &str,
) -> Result<Option<PaymentResponse>, AppError> {
    let record = db::payments::find_idempotency(pool, business_id, key).await?;
    match record {
        Some((stored_hash, body)) => {
            if stored_hash != request_hash {
                return Err(AppError::unprocessable("idempotency_mismatch", "Idempotency key already used with a different request payload"));
            }
            Ok(Some(serde_json::from_value(body)?))
        }
        None => Ok(None),
    }
}

pub async fn execute(
    pool: &PgPool, psp_url: &str, psp_timeout_secs: u64, business_id: Uuid, invoice_id: Uuid, payment_token: &str,
) -> Result<PaymentResponse, AppError> {
    // TX1: Lock invoice, validate state, create payment attempt
    let mut tx = pool.begin().await?;

    let invoice = db::invoices::lock_for_update(&mut tx, invoice_id, business_id)
        .await?
        .ok_or_else(|| AppError::not_found("invoice_not_found", "Invoice not found"))?;

    let (_, status, amount_cents, _) = invoice;

    if !status.can_attempt_payment() {
        return Err(match status {
            InvoiceStatus::Paid => AppError::conflict("invoice_already_paid", "Invoice has already been paid"),
            InvoiceStatus::Void => AppError::conflict("invoice_voided", "Invoice has been voided"),
            _ => AppError::conflict("invalid_invoice_state", "Invoice cannot accept payments in current state"),
        });
    }

    let attempt = db::payments::insert_attempt(&mut tx, invoice_id, amount_cents, payment_token).await?;
    tx.commit().await?; // Release lock before PSP call

    // PSP call (outside transaction)
    let psp_result = psp::charge(psp_url, amount_cents, payment_token, psp_timeout_secs).await;

    let (pay_status, inv_status, failure_reason, psp_txn_id) = match psp_result {
        Ok(txn_id) => (PaymentStatus::Succeeded, InvoiceStatus::Paid, None, Some(txn_id)),
        Err(psp::PspError::Declined(reason)) => (PaymentStatus::Failed, InvoiceStatus::Failed, Some(reason), None),
        Err(psp::PspError::Timeout) | Err(psp::PspError::NetworkError(_)) => {
            (PaymentStatus::Failed, InvoiceStatus::Failed, Some("Payment processor unavailable, please retry".to_string()), None)
        }
    };

    // TX2: Persist result + webhook
    let mut tx = pool.begin().await?;

    db::payments::update_attempt(
        &mut tx, attempt.id,
        &format!("{:?}", pay_status).to_lowercase(),
        psp_txn_id.as_deref(),
        failure_reason.as_deref(),
    ).await?;

    db::invoices::update_status(&mut tx, invoice_id, &format!("{:?}", inv_status).to_lowercase()).await?;

    let event_type = if pay_status == PaymentStatus::Succeeded { "invoice.paid" } else { "invoice.payment_failed" };
    webhooks::enqueue(&mut tx, business_id, event_type, &serde_json::json!({
        "event": event_type, "invoice_id": invoice_id, "payment_id": attempt.id,
        "amount_cents": amount_cents, "status": format!("{:?}", pay_status).to_lowercase(),
    })).await?;

    tx.commit().await?;

    Ok(PaymentResponse {
        payment_id: attempt.id,
        invoice_id,
        status: pay_status,
        amount_cents,
        failure_reason,
        created_at: attempt.created_at,
    })
}

pub async fn store_idempotency_response(
    pool: &PgPool, business_id: Uuid, key: &str, invoice_id: Uuid, hash: &str, response: &PaymentResponse,
) {
    let body = serde_json::to_value(response).unwrap_or_default();
    let _ = db::payments::store_idempotency(pool, business_id, key, &format!("/invoices/{}/pay", invoice_id), hash, &body).await;
}
