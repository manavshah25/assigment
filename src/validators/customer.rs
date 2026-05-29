use regex::Regex;
use std::sync::LazyLock;

use crate::errors::AppError;
use crate::models::customer::CreateCustomerRequest;

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap()
});

pub fn validate_create(req: &CreateCustomerRequest) -> Result<(), AppError> {
    // Email validation
    let email = req.email.trim();
    if email.is_empty() {
        return Err(AppError::bad_request("invalid_email", "Email is required"));
    }
    if email.len() > 255 {
        return Err(AppError::bad_request("invalid_email", "Email must be less than 255 characters"));
    }
    if !EMAIL_REGEX.is_match(email) {
        return Err(AppError::bad_request("invalid_email", "Email format is invalid"));
    }

    // Name validation
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("invalid_name", "Name is required"));
    }
    if name.len() < 2 {
        return Err(AppError::bad_request("invalid_name", "Name must be at least 2 characters"));
    }
    if name.len() > 100 {
        return Err(AppError::bad_request("invalid_name", "Name must be less than 100 characters"));
    }

    Ok(())
}
