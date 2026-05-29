use sqlx::PgPool;
use uuid::Uuid;

use crate::db;
use crate::errors::AppError;
use crate::models::customer::{CreateCustomerRequest, CustomerResponse};

pub async fn create(pool: &PgPool, business_id: Uuid, req: &CreateCustomerRequest) -> Result<CustomerResponse, AppError> {
    let email = req.email.trim().to_lowercase();
    let name = req.name.trim().to_string();

    let customer = db::customers::insert(pool, business_id, &email, &name)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err)
                if db_err.constraint() == Some("customers_business_id_email_key") =>
            {
                AppError::conflict("customer_already_exists", format!("Customer with email '{}' already exists", email))
            }
            other => AppError::Database(other),
        })?;

    Ok(customer.into())
}

pub async fn get(pool: &PgPool, id: Uuid, business_id: Uuid) -> Result<CustomerResponse, AppError> {
    let customer = db::customers::find_by_id(pool, id, business_id)
        .await?
        .ok_or_else(|| AppError::not_found("customer_not_found", "Customer not found"))?;

    Ok(customer.into())
}

pub async fn list(pool: &PgPool, business_id: Uuid, limit: i64, offset: i64) -> Result<Vec<CustomerResponse>, AppError> {
    let customers = db::customers::list(pool, business_id, limit, offset).await?;
    Ok(customers.into_iter().map(Into::into).collect())
}
