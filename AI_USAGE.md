# AI Usage Documentation

## Which AI Tools I Used and For What

**Amazon Q Developer** (IDE assistant). Specific uses:

- **Boilerplate generation:** Scaffolded Axum route handlers with `Extension<AuthenticatedBusiness>` extraction pattern and sqlx query structure. I kept the pattern but rewrote SQL and error handling.

- **SQL migration drafting:** Produced a working first draft. I revised it to add: `GENERATED ALWAYS AS` column on `line_items` (it used application-level computation), partial index on `api_keys` (it had a full index), `CHECK (amount_cents > 0)` constraint (it omitted this), and `UNIQUE(business_id, invoice_number)`.

- **Webhook worker structure:** Gave me the `loop { poll(); sleep(); }` skeleton. I added `FOR UPDATE SKIP LOCKED`, exponential backoff calculation, and failure/exhaustion logic.

- **Settings pattern:** Suggested using `dotenvy` crate. I chose a simpler `Settings::from_env()` struct with `std::env::var` + defaults — no extra dependency needed for this scale.

## Three Decisions I Made Against or Independent of AI

### 1. Committing TX1 before calling the PSP

**What AI proposed:** Single transaction: `BEGIN → SELECT FOR UPDATE → call PSP → UPDATE → COMMIT`.

**What I chose:** Two transactions. TX1 acquires lock + inserts `payment_attempt(processing)` + commits. PSP call with no open transaction. TX2 records result.

**Why:** A single transaction holding a row lock for 10+ seconds drains the connection pool. With `MAX_DB_CONNECTIONS=10` and 10 concurrent timeout payments, the entire service deadlocks. The split means locks are held for milliseconds. The tradeoff (crash between TX1 and TX2) is rare and recoverable via reconciliation.

### 2. Making `failed` a retriable state

**What AI suggested:** Four terminal states: pending → paid, pending → failed, pending → void.

**What I chose:** `failed` transitions to `paid` on successful retry. Only `paid` and `void` are terminal.

**Why:** Real payment failures are transient (daily card limits reset at midnight). Making `failed` terminal forces creating a new invoice for every retry — breaks audit trail and customer experience. Matches how Stripe/Adyen work.

### 3. Centralized Settings struct instead of scattered env::var() calls

**What AI did:** Put `std::env::var("PSP_URL")` directly in `AppState::new()` and `std::env::var("DATABASE_URL")` in `database.rs`.

**What I chose:** Single `Settings::from_env()` in `config/settings.rs` that loads ALL config once. Passed through `AppState` to every handler.

**Why:** Scattered env reads are untestable (can't override in tests), hard to audit (grep for env vars across 20 files), and fail at runtime not startup. Centralized settings fail fast on boot if config is missing.

## One Thing the AI Got Wrong

The mock PSP response format. AI generated:
```rust
struct ChargeResponse {
    status: String,           // used "success"
    transaction_id: Option<String>,
    error: Option<String>,
}
```

The spec requires:
```
tok_success → {status: "succeeded", psp_ref: <uuid>}
tok_insufficient_funds → {status: "failed", code: "insufficient_funds"}
```

Two errors: (1) field names wrong (`transaction_id` vs `psp_ref`, `error` vs `code`), (2) status value wrong (`"success"` vs `"succeeded"`).

Also, for `tok_network_error`, the AI returned a 200 with `{"status": "error"}` in the body. The spec says "Returns 500 or drops the connection." I fixed it to return actual `StatusCode::INTERNAL_SERVER_ERROR`.

**How I verified:** Traced the full flow: mock-psp serializes `psp_ref` → PSP client deserializes `psp_ref` → stores as `psp_transaction_id`. Confirmed field names match across the boundary.
