use regex::Regex;
use std::sync::LazyLock;

use crate::errors::AppError;
use crate::models::invoice::CreateInvoiceRequest;

// Invoice number: alphanumeric, hyphens, max 50 chars (e.g. INV-001, 2024-0042)
static INVOICE_NUMBER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9][A-Za-z0-9\-_]{1,48}[A-Za-z0-9]$").unwrap()
});

pub fn validate_create(req: &CreateInvoiceRequest) -> Result<(), AppError> {
    // Invoice number validation
    let inv_num = req.invoice_number.trim();
    if inv_num.is_empty() {
        return Err(AppError::bad_request("invalid_invoice_number", "Invoice number is required"));
    }
    if inv_num.len() < 3 {
        return Err(AppError::bad_request("invalid_invoice_number", "Invoice number must be at least 3 characters"));
    }
    if inv_num.len() > 50 {
        return Err(AppError::bad_request("invalid_invoice_number", "Invoice number must be less than 50 characters"));
    }
    if !INVOICE_NUMBER_REGEX.is_match(inv_num) {
        return Err(AppError::bad_request("invalid_invoice_number", "Invoice number must be alphanumeric (hyphens and underscores allowed)"));
    }

    // Line items validation
    if req.line_items.is_empty() {
        return Err(AppError::bad_request("missing_line_items", "At least one line item is required"));
    }
    if req.line_items.len() > 100 {
        return Err(AppError::bad_request("too_many_line_items", "Maximum 100 line items allowed"));
    }

    for (i, li) in req.line_items.iter().enumerate() {
        let pos = i + 1;
        if li.description.trim().is_empty() {
            return Err(AppError::bad_request("invalid_line_item", format!("Line item {}: description is required", pos)));
        }
        if li.description.len() > 255 {
            return Err(AppError::bad_request("invalid_line_item", format!("Line item {}: description too long (max 255)", pos)));
        }
        if li.quantity <= 0 {
            return Err(AppError::bad_request("invalid_line_item", format!("Line item {}: quantity must be > 0", pos)));
        }
        if li.unit_price_cents < 0 {
            return Err(AppError::bad_request("invalid_line_item", format!("Line item {}: unit_price_cents cannot be negative", pos)));
        }
    }

    let total: i64 = req.line_items.iter().map(|li| li.quantity as i64 * li.unit_price_cents).sum();
    if total <= 0 {
        return Err(AppError::bad_request("invalid_amount", "Invoice total must be positive"));
    }

    if let Some(ref c) = req.currency {
        if c != "usd" {
            return Err(AppError::bad_request("invalid_currency", "Only USD currency is supported"));
        }
    }

    Ok(())
}
