use crate::services::webhooks;
use crate::AppState;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Background worker that polls for pending webhook events and delivers them.
/// Uses exponential backoff: 1s, 2s, 4s, 8s, 16s between retries.
/// After max_attempts (5), marks as failed.
///
/// WHY polling instead of notify/listen:
/// - Simpler, no connection management for LISTEN channels
/// - Naturally handles restarts (picks up where it left off)
/// - 1s poll interval is acceptable for webhook delivery SLA
pub async fn run(state: Arc<AppState>) {
    loop {
        match deliver_pending(&state).await {
            Ok(count) if count > 0 => {
                tracing::debug!("Delivered {} webhook(s)", count);
            }
            Err(e) => {
                tracing::error!("Webhook worker error: {}", e);
            }
            _ => {}
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[derive(sqlx::FromRow)]
struct PendingEvent {
    id: Uuid,
    business_id: Uuid,
    event_type: String,
    payload: serde_json::Value,
    attempts: i32,
    max_attempts: i32,
}

async fn deliver_pending(state: &AppState) -> Result<usize, sqlx::Error> {
    // Fetch up to 10 pending events that are due for delivery.
    // FOR UPDATE SKIP LOCKED: if multiple workers existed, they wouldn't conflict.
    let events = sqlx::query_as::<_, PendingEvent>(
        "SELECT id, business_id, event_type, payload, attempts, max_attempts
         FROM webhook_events
         WHERE status = 'pending' AND next_attempt_at <= now()
         ORDER BY created_at ASC
         LIMIT 10
         FOR UPDATE SKIP LOCKED"
    )
    .fetch_all(&state.db)
    .await?;

    let count = events.len();

    for event in events {
        // Get business webhook config
        let config = sqlx::query_as::<_, (Option<String>, String)>(
            "SELECT webhook_url, webhook_secret FROM businesses WHERE id = $1"
        )
        .bind(event.business_id)
        .fetch_optional(&state.db)
        .await?;

        let (webhook_url, webhook_secret) = match config {
            Some((Some(url), secret)) => (url, secret),
            _ => {
                // No webhook URL configured, mark as delivered (nothing to do)
                sqlx::query("UPDATE webhook_events SET status = 'delivered' WHERE id = $1")
                    .bind(event.id)
                    .execute(&state.db)
                    .await?;
                continue;
            }
        };

        let payload_bytes = serde_json::to_vec(&event.payload).unwrap_or_default();
        let signature = webhooks::sign_payload(&payload_bytes, &webhook_secret);
        let timestamp = chrono::Utc::now().timestamp().to_string();

        let result = reqwest::Client::new()
            .post(&webhook_url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Signature", &signature)
            .header("X-Webhook-Timestamp", &timestamp)
            .header("X-Webhook-Event", &event.event_type)
            .header("X-Webhook-Id", event.id.to_string())
            .body(payload_bytes)
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        let new_attempts = event.attempts + 1;

        match result {
            Ok(resp) if resp.status().is_success() => {
                sqlx::query(
                    "UPDATE webhook_events SET status = 'delivered', attempts = $1 WHERE id = $2"
                )
                .bind(new_attempts)
                .bind(event.id)
                .execute(&state.db)
                .await?;
            }
            Ok(resp) => {
                let error = format!("HTTP {}", resp.status());
                handle_failure(&state.db, event.id, new_attempts, event.max_attempts, &error).await?;
            }
            Err(e) => {
                handle_failure(&state.db, event.id, new_attempts, event.max_attempts, &e.to_string()).await?;
            }
        }
    }

    Ok(count)
}

async fn handle_failure(
    db: &sqlx::PgPool,
    event_id: Uuid,
    attempts: i32,
    max_attempts: i32,
    error: &str,
) -> Result<(), sqlx::Error> {
    if attempts >= max_attempts {
        sqlx::query(
            "UPDATE webhook_events SET status = 'failed', attempts = $1, last_error = $2 WHERE id = $3"
        )
        .bind(attempts)
        .bind(error)
        .bind(event_id)
        .execute(db)
        .await?;
    } else {
        // Exponential backoff: 2^attempts seconds
        let backoff_secs = 2_i64.pow(attempts as u32);
        sqlx::query(
            "UPDATE webhook_events SET attempts = $1, last_error = $2,
             next_attempt_at = now() + ($3 || ' seconds')::interval
             WHERE id = $4"
        )
        .bind(attempts)
        .bind(error)
        .bind(backoff_secs.to_string())
        .bind(event_id)
        .execute(db)
        .await?;
    }
    Ok(())
}
