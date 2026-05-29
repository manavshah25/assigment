use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Invoice state machine:
///   pending -> paid     (successful payment)
///   pending -> failed   (failed payment, can retry)
///   failed  -> paid     (successful retry)
///   pending -> void     (manual cancellation)
///   failed  -> void     (manual cancellation)
///
/// Terminal states: paid, void
/// Retriable state: failed
#[derive(Debug, Clone, PartialEq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "invoice_status", rename_all = "lowercase")]
pub enum InvoiceStatus {
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "paid")]
    Paid,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "void")]
    Void,
}

impl InvoiceStatus {
    /// Returns true if a payment attempt can be initiated from this state
    pub fn can_attempt_payment(&self) -> bool {
        matches!(self, Self::Pending | Self::Failed)
    }
}

#[derive(FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub business_id: Uuid,
    pub customer_id: Uuid,
    pub invoice_number: String,
    pub status: InvoiceStatus,
    pub amount_cents: i64,
    pub currency: String,
    pub due_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(FromRow)]
pub struct LineItem {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub description: String,
    pub quantity: i32,
    pub unit_price_cents: i64,
    pub amount_cents: i64,
}

#[derive(Deserialize)]
pub struct LineItemInput {
    pub description: String,
    pub quantity: i32,
    pub unit_price_cents: i64,
}

#[derive(Deserialize)]
pub struct CreateInvoiceRequest {
    pub customer_id: Uuid,
    pub invoice_number: String,
    pub currency: Option<String>,
    pub due_date: Option<DateTime<Utc>>,
    pub line_items: Vec<LineItemInput>,
}

#[derive(Serialize)]
pub struct InvoiceResponse {
    pub id: Uuid,
    pub invoice_number: String,
    pub customer_id: Uuid,
    pub status: InvoiceStatus,
    pub amount_cents: i64,
    pub currency: String,
    pub due_date: Option<DateTime<Utc>>,
    pub line_items: Vec<LineItemResponse>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct LineItemResponse {
    pub id: Uuid,
    pub description: String,
    pub quantity: i32,
    pub unit_price_cents: i64,
    pub amount_cents: i64,
}
