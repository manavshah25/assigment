use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    business_id: Uuid,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO webhook_events (business_id, event_type, payload) VALUES ($1, $2, $3)")
        .bind(business_id).bind(event_type).bind(payload)
        .execute(&mut **tx).await?;
    Ok(())
}

pub fn sign_payload(payload: &[u8], secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(payload);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}
