use crate::errors::AppError;
use crate::models::payment::PayInvoiceRequest;

pub fn validate_pay(req: &PayInvoiceRequest) -> Result<(), AppError> {
    if req.payment_token.trim().is_empty() {
        return Err(AppError::bad_request("missing_payment_token", "payment_token is required"));
    }
    Ok(())
}
