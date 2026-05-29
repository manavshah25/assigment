# Design Document — Invoice & Payment Service

## Architecture

One Rust binary. One PostgreSQL. One mock PSP over HTTP. No Redis, no Kafka, no microservices.

```
routes/ → validators/ → services/ → db/
                            ↓
                        PSP (HTTP, 10s timeout)
Background: webhook worker (tokio::spawn, polls every 1s)
```

I chose this because the problem is about payment correctness, not distributed systems choreography. Adding Redis for idempotency would mean handling Redis-Postgres inconsistency (Redis says "processed" but Postgres transaction rolled back — I've debugged this exact bug before). PostgreSQL handles both concerns atomically.

All configuration lives in `config/settings.rs` — loaded from environment variables once at startup. No `env::var()` calls scattered across the codebase.

---

## 1. Data Model

```
businesses ──1:N──→ api_keys
     │         └──→ auth_tokens (session tokens, 24h expiry)
     ├──1:N──→ customers
     └──1:N──→ invoices ──1:N──→ line_items
                    └──1:N──→ payment_attempts
businesses ──1:N──→ webhook_events
idempotency_keys: UNIQUE(business_id, key)
```

**Key constraints:**
- `invoices.amount_cents BIGINT CHECK (amount_cents > 0)` — integer cents, never floats.
- `invoices.invoice_number TEXT NOT NULL, UNIQUE(business_id, invoice_number)` — user-provided, validated with regex `^[A-Za-z0-9][A-Za-z0-9\-_]{1,48}[A-Za-z0-9]$`.
- `line_items.amount_cents GENERATED ALWAYS AS (quantity * unit_price_cents) STORED` — DB computes totals.
- `UNIQUE(business_id, email)` on customers — multi-tenant isolation at constraint level.
- `auth_tokens.token_hash` — SHA256-hashed, expires after configurable hours (default 24h via `TOKEN_EXPIRY_HOURS`).

**PK: UUID v4.** No information leakage, no sequence contention. At 100x scale → UUIDv7 for B-tree locality.

**At 100x scale:** Partition invoices by business_id hash. Move idempotency_keys to Redis (high-write, short-lived). Separate webhook worker process. Read replicas for GET endpoints.

---

## 2. Invoice State Machine

```
┌─────────┐  PSP succeeded   ┌──────┐
│ pending │ ────────────────→ │ paid │ (terminal)
└────┬────┘                   └──────┘
     │ PSP failed/timeout
     ▼
┌─────────┐  retry succeeds  ┌──────┐
│ failed  │ ────────────────→ │ paid │
└────┬────┘                   └──────┘
     │ manual void
     ▼
┌─────────┐
│  void   │ (terminal)       ← also reachable from pending
└─────────┘
```

**Why `failed` is retriable:** A card declined at 11:58 PM for "daily limit" succeeds at 12:01 AM. Making `failed` terminal forces creating a new invoice — breaks the audit trail. Matches Stripe PaymentIntents behavior.

**Rejection:** `can_attempt_payment()` on the `InvoiceStatus` enum returns true only for `Pending | Failed`. Any other state → HTTP 409 with specific error code (`invoice_already_paid` or `invoice_voided`).

**Which transitions are reversible:** None. `paid` and `void` are terminal. Once paid, refunds are a separate domain (out of scope). Once voided, the invoice is dead.

---

## 3. Payment Correctness & Failure Modes

**Concurrency mechanism:** `SELECT id, status, amount_cents FROM invoices WHERE id = $1 FOR UPDATE`

Why not alternatives:
- **Advisory locks:** UUIDs don't fit in bigint without hashing; collisions block unrelated invoices.
- **Optimistic locking:** Retry loop might call PSP twice — unacceptable for payments.
- **Serializable isolation:** Aborts transactions, requires retry logic dangerous with PSP calls.
- **Conditional UPDATE:** Can't distinguish "not found" from "wrong state" — both return 0 rows.

**Split transaction around PSP call:** TX1 acquires lock + inserts payment_attempt(processing) + commits. PSP call happens with no open transaction (timeout configurable via `PSP_TIMEOUT_SECS`, default 10s). TX2 records result. Why: holding a transaction for 10s with 10 connections = complete service deadlock.

### (a) Two concurrent POST /pay, same invoice

Request A acquires row lock → inserts payment_attempt(processing) → commits TX1 → calls PSP → commits TX2 (status=paid). Request B blocked on `FOR UPDATE` → unblocks → reads `paid` → returns 409 `invoice_already_paid`. **One PSP call. One charge.**

### (b) tok_timeout (PSP sleeps 30s, our timeout is 10s)

TX1 commits with `processing`. reqwest times out at 10s. TX2 marks payment `failed`, invoice `failed`. API returns `"Payment processor unavailable, please retry"`. Client can retry with new idempotency key.

**Did PSP actually charge?** Yes — tok_timeout returns success after 30s. We never saw it. Production fix: pass `payment_attempt.id` as PSP idempotency key + reconciliation worker.

### (c) PSP success, service crashes before DB write

Payment_attempt stuck in `processing`. PSP has the money. On retry: no cached idempotency record, proceeds as new → potential double charge. **Known gap.** Fix: PSP-level idempotency key + reconciliation job querying `processing` payments older than 5 min.

### (d) Same idempotency key, different payload

`SHA256(invoice_id + payment_token + business_id)` compared to stored hash. Mismatch → HTTP 422 `idempotency_mismatch`. No PSP call.

### (e) POST /pay on paid invoice

`SELECT FOR UPDATE` → `can_attempt_payment()` returns false → HTTP 409 `invoice_already_paid`. No PSP call. Lock held for microseconds.

---

## 4. Webhook Design

**Signing:** HMAC-SHA256 of raw JSON payload with per-business secret. Headers: `X-Webhook-Signature`, `X-Webhook-Timestamp` (reject if >5 min old), `X-Webhook-Id` (deduplication).

**Events:** `invoice.created`, `invoice.paid`, `invoice.payment_failed`.

**Retry:** Exponential backoff: 0s, 2s, 4s, 8s, 16s. Max attempts configurable via `WEBHOOK_MAX_ATTEMPTS` (default 5). ~30s total budget. After exhaustion: `status = 'failed'`, preserved for manual replay.

**Outbox pattern:** Webhook events inserted in same transaction as payment state update. Background tokio task polls every `WEBHOOK_POLL_INTERVAL_SECS` (default 1s) with `FOR UPDATE SKIP LOCKED`. If transaction rolls back → webhook never exists. If commits → delivery guaranteed (at-least-once).

**Why not fire-and-forget:** Crash = webhook lost forever. No retry. No audit trail.

**Reconciliation:** Businesses poll API for current state (source of truth). In production: add `POST /webhooks/{id}/replay` endpoint.

---

## 5. API Key Model

- **Two-tier auth:** API key (`sk_test_xxx`) exchanges for session token (`tok_xxx`) via `POST /api/auth/token`. Protected routes accept either format.
- **Storage:** Both key and token are SHA256-hashed in DB. Plaintext never stored.
- **Token expiry:** Configurable via `TOKEN_EXPIRY_HOURS` (default 24h). Stored in `auth_tokens` table.
- **Revocation:** `api_keys.revoked_at` — immediate effect. Auth endpoint returns specific `api_key_revoked` error code.
- **Blast radius:** Key scoped to one business. Attacker can read that business's customers but can't access other tenants.
- **Rotation:** Issue new key → grace period → revoke old. `revoked_at` column supports this.

---

## 6. What I Cut and Why

1. **Refunds** — Separate state machine (requested → processing → settled). Would model as `refunds` table with FK to `payment_attempts`. Requires PSP refund endpoint not in mock spec.
2. **Rate limiting** — Would implement as leaky bucket counter in Postgres. Orthogonal to payment correctness. Doesn't demonstrate payment thinking.
3. **Reconciliation worker** — Most painful cut. Would poll `processing` payments older than 5 min against PSP status API. Can't implement without PSP "get status" endpoint.
4. **Webhook registration API** — Standard CRUD. Hardcoded `webhook_url` in seed data.
5. **Partial payments** — Would require `remaining_cents` tracking and splitting across multiple successful attempts. Out of scope per spec.

---

## 7. Production Readiness Gap

1. **Reconciliation job** — Crash-after-PSP-success (3c) leaves money charged but unrecorded. Fix: cron querying `processing` payments older than 5 min against PSP. Single scariest gap.

2. **Observability** — Zero metrics. Need: `payment_success_rate` gauge (alert <95%), `psp_latency_seconds` histogram (alert P99 >5s), structured JSON logs with trace_id.

3. **Connection pool exhaustion under timeouts** — `MAX_DB_CONNECTIONS` (default 10), concurrent tok_timeout payments = service unavailability. Fix: semaphore limiting concurrent PSP calls, or separate pools for reads vs payment writes.
