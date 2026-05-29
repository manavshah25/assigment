# Video Walkthrough Script

Use this as a guide when recording your 5-10 min Loom video.
The spec requires 4 sections in this order. Be unscripted — just use these as talking points.

---

## Section 1: Architecture Overview (1-2 min)

**Open:** `docker-compose.yml` and the project folder structure.

**Talk through:**
- "Three services: PostgreSQL, mock PSP, and the Rust API — all started with one command."
- "The API is a layered Rust/Axum app:"
  - `routes/` — thin HTTP handlers (extract params, call service, wrap response)
  - `validators/` — pure input validation (regex email, invoice number format)
  - `services/` — business logic (payment orchestration, idempotency)
  - `db/` — raw SQL queries (repository pattern)
- "All config is centralized in `config/settings.rs` — loaded from env vars at startup."
- "Request flow: Client → auth middleware → route → validator → service → DB → response"
- "Webhooks: outbox pattern — inserted in same transaction as payment, delivered async by background worker."

**Show:** `src/config/settings.rs` briefly — "All env vars in one place, like Django settings."

---

## Section 2: Live Demo (2-3 min)

**Run:** `docker compose up` (should already be running)

**Demo these curl commands in order:**

```bash
# 1. Get auth token
curl -s -X POST http://localhost:8080/api/auth/token \
  -H "X-API-Key: sk_test_abc123" | jq .

# 2. Create customer (use token from above)
curl -s -X POST http://localhost:8080/api/customers \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"email": "demo@example.com", "name": "Demo User"}' | jq .

# 3. Create invoice
curl -s -X POST http://localhost:8080/api/invoices \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"customer_id": "<id>", "invoice_number": "INV-DEMO-001", "line_items": [{"description": "Widget", "quantity": 2, "unit_price_cents": 1500}]}' | jq .

# 4. Pay invoice (success)
curl -s -X POST http://localhost:8080/api/invoices/<id>/pay \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: demo-pay-001" \
  -d '{"payment_token": "tok_success"}' | jq .

# 5. Pay invoice (failure — use a NEW invoice)
# Create another invoice first, then:
curl -s -X POST http://localhost:8080/api/invoices/<id>/pay \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: demo-pay-002" \
  -d '{"payment_token": "tok_card_declined"}' | jq .
```

**Point out:**
- "Notice the invoice status changed from `pending` to `paid` after successful payment."
- "The failed payment shows `failure_reason: card_declined` and invoice is now `failed` — but retriable."
- "Check docker logs — you'll see webhook delivery attempts in the API logs."

---

## Section 3: State Machine Walkthrough (1-2 min, UNSCRIPTED)

**Open:** `src/models/invoice.rs` — show the `InvoiceStatus` enum and `can_attempt_payment()`.

**Explain in your own words:**
- "I chose 4 states: pending, paid, failed, void."
- "Pending is the initial state when an invoice is created."
- "When payment succeeds → paid. Terminal. Can't pay again."
- "When payment fails (card declined, PSP timeout) → failed. This is retriable."
- "Why retriable? Because real cards get declined for transient reasons — daily limits reset at midnight."
- "Void is for manual cancellation. Also terminal."
- "The guard `can_attempt_payment()` returns true only for pending and failed. Everything else gets a 409 with a specific error code."
- "I deliberately didn't add a `draft` state because it would need a finalize endpoint and line item editing — scope creep for this assignment."

---

## Section 4: Failure Mode Walkthrough (1-2 min, UNSCRIPTED)

**Pick ONE.** I recommend `tok_timeout` because you can demo it live.

**Open:** `src/services/payment.rs` — the `execute()` function.

**Walk through the code:**
- "Here's what happens with `tok_timeout`:"
- "First, TX1: we acquire a row lock with `SELECT FOR UPDATE`, insert a payment_attempt in `processing` state, and commit. This releases the lock."
- "Why commit before calling PSP? Because if we held the transaction open for 30 seconds, we'd exhaust the connection pool."
- "Then we call the PSP. Our timeout is configured at 10 seconds (`settings.psp_timeout_secs`). The PSP sleeps 30 seconds. So reqwest returns a timeout error."
- "We catch it here — `PspError::Timeout` — and mark the payment as `failed`, invoice as `failed`."
- "The client gets back a response in ~10 seconds, not 30. The invoice is in `failed` state — not stuck in `processing`."
- "The client can retry with a new idempotency key and `tok_success` — it will work because `failed` is retriable."

**Optional live demo:**
```bash
# This takes ~10 seconds (not 30!)
curl -s -X POST http://localhost:8080/api/invoices/<id>/pay \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: timeout-demo" \
  -d '{"payment_token": "tok_timeout"}' | jq .
```

"See? 10 seconds, not 30. Invoice is `failed`, not stuck. I can retry now."

---

## Tips for Recording

- Don't read from notes. Just glance at this file for the order.
- Ums and pauses are fine. They want fluency, not polish.
- Keep terminal visible. Use `jq` for pretty output.
- If something breaks, explain what happened — that shows understanding.
- Total: aim for 7 minutes. Under 5 is too rushed, over 10 is too long.
