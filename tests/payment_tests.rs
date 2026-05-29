//! Integration tests for payment correctness.
//! Run with: cargo test --test payment_tests
//!
//! Requires: docker compose up (postgres + mock-psp running)
//! Set DATABASE_URL and PSP_URL env vars, or use defaults.

use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

const BASE_URL: &str = "http://localhost:8080";
const AUTH_HEADER: &str = "Bearer sk_test_abc123";

async fn create_test_customer(client: &Client) -> String {
    let email = format!("test-{}@example.com", uuid::Uuid::new_v4());
    let resp = client
        .post(format!("{}/api/customers", BASE_URL))
        .header("Authorization", AUTH_HEADER)
        .json(&json!({"email": email, "name": "Test User"}))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

async fn create_test_invoice(client: &Client, customer_id: &str) -> String {
    let resp = client
        .post(format!("{}/api/invoices", BASE_URL))
        .header("Authorization", AUTH_HEADER)
        .json(&json!({
            "customer_id": customer_id,
            "line_items": [{"description": "Test", "quantity": 1, "unit_price_cents": 5000}]
        }))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

/// TEST 1: Concurrent payment — fire N requests simultaneously.
/// Asserts: exactly one succeeds, no double-charge, final state is 'paid'.
#[tokio::test]
async fn test_concurrent_payments_no_double_charge() {
    let client = Client::new();
    let customer_id = create_test_customer(&client).await;
    let invoice_id = create_test_invoice(&client, &customer_id).await;

    // Fire 10 concurrent payment requests
    let mut handles = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        let invoice_id = invoice_id.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(format!("{}/api/invoices/{}/pay", BASE_URL, invoice_id))
                .header("Authorization", AUTH_HEADER)
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
                if body["status"] == "succeeded" {
                    success_count += 1;
                }
            }
            409 => conflict_count += 1,
            _ => {}
        }
    }

    // Exactly one payment should succeed
    assert_eq!(success_count, 1, "Expected exactly 1 successful payment, got {}", success_count);
    assert!(conflict_count >= 1, "Expected at least 1 conflict response");

    // Verify final invoice state is 'paid'
    let resp = client
        .get(format!("{}/api/invoices/{}", BASE_URL, invoice_id))
        .header("Authorization", AUTH_HEADER)
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "paid");
}

/// TEST 2: Idempotency — same key returns same response without second PSP call.
#[tokio::test]
async fn test_idempotency_returns_cached_response() {
    let client = Client::new();
    let customer_id = create_test_customer(&client).await;
    let invoice_id = create_test_invoice(&client, &customer_id).await;
    let idem_key = format!("idem-test-{}", uuid::Uuid::new_v4());

    // First request
    let resp1 = client
        .post(format!("{}/api/invoices/{}/pay", BASE_URL, invoice_id))
        .header("Authorization", AUTH_HEADER)
        .header("Idempotency-Key", &idem_key)
        .json(&json!({"payment_token": "tok_success"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 200);
    let body1: Value = resp1.json().await.unwrap();
    assert_eq!(body1["status"], "succeeded");

    // Second request with same key — should return cached response
    let resp2 = client
        .post(format!("{}/api/invoices/{}/pay", BASE_URL, invoice_id))
        .header("Authorization", AUTH_HEADER)
        .header("Idempotency-Key", &idem_key)
        .json(&json!({"payment_token": "tok_success"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), 200);
    let body2: Value = resp2.json().await.unwrap();

    // Same payment_id returned (proves no second PSP call)
    assert_eq!(body1["payment_id"], body2["payment_id"]);

    // Same key, DIFFERENT payload — should return 422
    let resp3 = client
        .post(format!("{}/api/invoices/{}/pay", BASE_URL, invoice_id))
        .header("Authorization", AUTH_HEADER)
        .header("Idempotency-Key", &idem_key)
        .json(&json!({"payment_token": "tok_card_declined"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp3.status(), 422);
}

/// TEST 3: PSP timeout — invoice is not stuck, can be retried.
#[tokio::test]
async fn test_psp_timeout_does_not_corrupt_state() {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    let customer_id = create_test_customer(&client).await;
    let invoice_id = create_test_invoice(&client, &customer_id).await;

    // Use tok_timeout — our service has 10s client timeout, so this will fail
    let resp = client
        .post(format!("{}/api/invoices/{}/pay", BASE_URL, invoice_id))
        .header("Authorization", AUTH_HEADER)
        .header("Idempotency-Key", format!("timeout-{}", uuid::Uuid::new_v4()))
        .json(&json!({"payment_token": "tok_timeout"}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "failed");
    assert!(body["failure_reason"].as_str().unwrap().contains("unavailable"));

    // Invoice should be in 'failed' state (retriable, not stuck)
    let resp = client
        .get(format!("{}/api/invoices/{}", BASE_URL, invoice_id))
        .header("Authorization", AUTH_HEADER)
        .send()
        .await
        .unwrap();
    let invoice: Value = resp.json().await.unwrap();
    assert_eq!(invoice["status"], "failed");

    // Retry with tok_success — should work (failed is retriable)
    let resp = client
        .post(format!("{}/api/invoices/{}/pay", BASE_URL, invoice_id))
        .header("Authorization", AUTH_HEADER)
        .header("Idempotency-Key", format!("retry-{}", uuid::Uuid::new_v4()))
        .json(&json!({"payment_token": "tok_success"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "succeeded");
}
