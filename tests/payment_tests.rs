//! Integration tests for payment correctness.
//! Run with: cargo test --test payment_tests
//!
//! Requires: docker compose up -d (postgres + mock-psp + api running)

use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

fn base_url() -> String {
    std::env::var("TEST_BASE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// Get a fresh auth token
async fn get_token(client: &Client) -> String {
    let resp = client
        .post(format!("{}/api/auth/token", base_url()))
        .header("X-API-Key", "sk_test_abc123")
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    body["data"]["token"].as_str().unwrap().to_string()
}

async fn create_customer(client: &Client, token: &str) -> String {
    let email = format!("test-{}@example.com", uuid::Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/customers", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({"email": email, "name": "Test User"}))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    body["data"]["id"].as_str().unwrap().to_string()
}

async fn create_invoice(client: &Client, token: &str, customer_id: &str) -> String {
    let inv_num = format!("INV-{}", uuid::Uuid::new_v4().to_string().get(..8).unwrap());
    let resp = client
        .post(format!("{}/api/invoices", base_url()))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({
            "customer_id": customer_id,
            "invoice_number": inv_num,
            "line_items": [{"description": "Test Item", "quantity": 1, "unit_price_cents": 5000}]
        }))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    body["data"]["id"].as_str().unwrap().to_string()
}

/// TEST 1: Concurrency — fire 10 concurrent POST /pay for the same invoice.
/// Asserts: at most one succeeds, no double-charges, final state is consistent.
#[tokio::test]
async fn test_concurrent_payments_no_double_charge() {
    let client = Client::new();
    let token = get_token(&client).await;
    let customer_id = create_customer(&client, &token).await;
    let invoice_id = create_invoice(&client, &token, &customer_id).await;

    // Fire 10 concurrent payment requests with different idempotency keys
    let mut handles = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        let invoice_id = invoice_id.clone();
        let token = token.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(format!("{}/api/invoices/{}/pay", base_url(), invoice_id))
                .header("Authorization", format!("Bearer {}", token))
                .header("Idempotency-Key", format!("concurrent-{}-{}", invoice_id, i))
                .json(&json!({"payment_token": "tok_success"}))
                .send()
                .await
                .unwrap()
        }));
    }

    let mut success_count = 0;
    let mut conflict_count = 0;

    for handle in handles {
        let resp = handle.await.unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap();

        match status {
            200 => {
                if body["data"]["status"] == "succeeded" {
                    success_count += 1;
                }
            }
            409 => conflict_count += 1,
            _ => {}
        }
    }

    // At most one payment succeeds
    assert_eq!(success_count, 1, "Expected exactly 1 successful payment, got {}", success_count);
    assert!(conflict_count >= 1, "Expected at least 1 conflict response");

    // Verify final invoice state is 'paid'
    let resp = client
        .get(format!("{}/api/invoices/{}", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["status"], "paid");
}

/// TEST 2: Idempotency — same key returns same response without second PSP call.
#[tokio::test]
async fn test_idempotency_returns_cached_response() {
    let client = Client::new();
    let token = get_token(&client).await;
    let customer_id = create_customer(&client, &token).await;
    let invoice_id = create_invoice(&client, &token, &customer_id).await;
    let idem_key = format!("idem-{}", uuid::Uuid::new_v4());

    // First request — succeeds
    let resp1 = client
        .post(format!("{}/api/invoices/{}/pay", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Idempotency-Key", &idem_key)
        .json(&json!({"payment_token": "tok_success"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 200);
    let body1: Value = resp1.json().await.unwrap();
    assert_eq!(body1["data"]["status"], "succeeded");
    let payment_id = body1["data"]["payment_id"].as_str().unwrap().to_string();

    // Second request with SAME key + SAME payload — returns cached response
    let resp2 = client
        .post(format!("{}/api/invoices/{}/pay", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Idempotency-Key", &idem_key)
        .json(&json!({"payment_token": "tok_success"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), 200);
    let body2: Value = resp2.json().await.unwrap();

    // Same payment_id proves no second PSP call was made
    assert_eq!(body2["data"]["payment_id"].as_str().unwrap(), payment_id);
    assert_eq!(body2["data"]["status"], "succeeded");

    // Third request with SAME key but DIFFERENT payload — returns 422
    let resp3 = client
        .post(format!("{}/api/invoices/{}/pay", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Idempotency-Key", &idem_key)
        .json(&json!({"payment_token": "tok_card_declined"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp3.status(), 422);
    let body3: Value = resp3.json().await.unwrap();
    assert_eq!(body3["error"]["code"], "idempotency_mismatch");
}

/// TEST 3: PSP failure — tok_timeout doesn't hang, invoice not stuck, can retry.
#[tokio::test]
async fn test_psp_failure_does_not_corrupt_state() {
    let client = Client::builder()
        .timeout(Duration::from_secs(30)) // Our test client waits longer than the service's 10s PSP timeout
        .build()
        .unwrap();

    let token = get_token(&client).await;
    let customer_id = create_customer(&client, &token).await;
    let invoice_id = create_invoice(&client, &token, &customer_id).await;

    // --- tok_timeout: PSP sleeps 30s, our service times out at 10s ---
    let resp = client
        .post(format!("{}/api/invoices/{}/pay", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Idempotency-Key", format!("timeout-{}", uuid::Uuid::new_v4()))
        .json(&json!({"payment_token": "tok_timeout"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // Payment attempt is recorded as failed
    assert_eq!(body["data"]["status"], "failed");
    assert!(body["data"]["failure_reason"].as_str().unwrap().contains("unavailable"));

    // Invoice is in 'failed' state (NOT stuck in 'processing')
    let resp = client
        .get(format!("{}/api/invoices/{}", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    let invoice: Value = resp.json().await.unwrap();
    assert_eq!(invoice["data"]["status"], "failed");

    // --- Retry with tok_success: failed state is retriable ---
    let resp = client
        .post(format!("{}/api/invoices/{}/pay", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .header("Idempotency-Key", format!("retry-{}", uuid::Uuid::new_v4()))
        .json(&json!({"payment_token": "tok_success"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // Retry succeeds — payment recorded, invoice now paid
    assert_eq!(body["data"]["status"], "succeeded");

    // Final invoice state is 'paid'
    let resp = client
        .get(format!("{}/api/invoices/{}", base_url(), invoice_id))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    let invoice: Value = resp.json().await.unwrap();
    assert_eq!(invoice["data"]["status"], "paid");
}
