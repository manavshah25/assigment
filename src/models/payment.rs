use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "payment_status", rename_all = "lowercase")]
pub enum PaymentStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "processing")]
    Processing,
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(FromRow)]
pub struct PaymentAttempt {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub status: PaymentStatus,
    pub amount_cents: i64,
    pub payment_token: String,
    pub psp_transaction_id: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct PayInvoiceRequest {
    pub payment_token: String,
}

#[derive(Serialize, Deserialize)]
pub struct PaymentResponse {
    pub payment_id: Uuid,
    pub invoice_id: Uuid,
    pub status: PaymentStatus,
    pub amount_cents: i64,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}
