# AI Usage Documentation

## Which AI Tools I Used and For What

**Amazon Q Developer** (IDE assistant). Specific uses:

- **Boilerplate generation:** Asked it to scaffold Axum route handlers with `Extension<AuthenticatedBusiness>` extraction pattern. It generated the repetitive `State(state): State<Arc<AppState>>` signatures and sqlx query structure. I kept the pattern but rewrote the actual SQL and error handling.

- **SQL migration drafting:** Asked for a PostgreSQL schema with the entity relationships I described. It produced a working first draft. I revised it to add: the `GENERATED ALWAYS AS` column on `line_items` (it used application-level computation), the partial index on `api_keys` (it had a full index), and the `CHECK (amount_cents > 0)` constraint (it omitted this).

- **Webhook worker structure:** Asked how to implement a polling-based background worker in tokio. It gave me the `loop { poll(); sleep(1s); }` skeleton. I added the `FOR UPDATE SKIP LOCKED` query (it used a simple SELECT), the exponential backoff calculation, and the failure/exhaustion logic.

- **Postman collection:** Generated the full JSON structure. I added the test scripts that chain variables between requests and the failure-scenario folder.

## Three Decisions I Made Against or Independent of AI

### 1. Committing TX1 before calling the PSP

**What AI proposed:** Wrap the entire payment flow in one transaction: `BEGIN → SELECT FOR UPDATE → call PSP → UPDATE → COMMIT`.

**What I chose:** Two transactions. TX1 acquires lock + inserts `payment_attempt(processing)` + commits. Then PSP call happens with no open transaction. TX2 records the result.

**Why I overrode it:** I've seen this exact bug in production. A single transaction holding a row lock for the duration of a network call (potentially 10+ seconds) means your connection pool drains under load. With 10 connections and 10 concurrent timeout-token payments, the entire service deadlocks — no new requests can get a connection. The AI's approach is "correct" in the ACID sense but operationally catastrophic.

The tradeoff (crash between TX1 and TX2 leaves orphaned `processing` state) is real but rare and recoverable via reconciliation. Connection pool exhaustion is common and unrecoverable without a restart.

### 2. Making `failed` a retriable state

**What AI suggested:** Standard four-state model: pending → paid (terminal), pending → failed (terminal), pending → void (terminal).

**What I chose:** `failed` transitions to `paid` on successful retry. Only `paid` and `void` are terminal.

**Why:** I've integrated with Stripe and Adyen. Both allow retrying failed payments on the same invoice/payment intent. The reason is practical: a card declined at 11:58 PM for "daily limit exceeded" will succeed at 12:01 AM. Forcing the business to void the invoice and create a new one breaks the audit trail and confuses the customer who received the original invoice link.

The AI's model would work for a simpler system, but it doesn't match how real billing products behave.

### 3. Hashing the request body for idempotency mismatch detection

**What AI suggested:** Store the full request body in the idempotency table and compare on replay.

**What I chose:** Store `SHA256(invoice_id + payment_token + business_id)` — a 32-byte hash.

**Why:** Storing full request bodies means the idempotency table grows proportionally to request size. With large payloads (not in this MVP, but in production with metadata fields), that's wasteful. A hash is fixed-size, comparison is O(1), and SHA256 collision probability is negligible. The downside is I can't show the user "here's what differed" on a 422 — I can only say "it's different." That's an acceptable tradeoff for a payment system where the error message is "don't reuse keys."

## One Thing the AI Got Wrong

The mock PSP response format. The AI generated:
```rust
struct ChargeResponse {
    status: String,           // used "success"
    transaction_id: Option<String>,
    error: Option<String>,
}
```

The assignment spec explicitly states:
```
tok_success → Returns {status: "succeeded", psp_ref: <uuid>}
tok_insufficient_funds → Returns {status: "failed", code: "insufficient_funds"}
```

Two errors: (1) field names wrong (`transaction_id` vs `psp_ref`, `error` vs `code`), (2) status value wrong (`"success"` vs `"succeeded"`).

I caught this by re-reading the spec's token behavior table after the initial generation. Fixed both the mock PSP struct and the client's deserialization. This is the kind of thing that would cause silent test failures — the PSP returns `"succeeded"` but the client checks for `"success"` and treats it as an unknown status, routing every payment to the error path.

**How I verified:** Traced the flow mentally: mock-psp returns `{"status": "succeeded", "psp_ref": "uuid"}` → PSP client matches on `"succeeded"` → extracts `psp_ref` → stores in `payment_attempts.psp_transaction_id`. Confirmed the deserialization struct fields match the mock's serialization struct fields.
