# Invoice & Payment Service

Minimal, correct invoice and payment backend built with Rust, Axum, and PostgreSQL.

## Demo Video

> **[Loom Video Link](https://loom.com/share/YOUR_VIDEO_ID_HERE)** — 5-10 minute walkthrough covering architecture, live demo, state machine, and failure mode handling.

## Quick Start

```bash
docker compose up --build
```

Service available at `http://localhost:8080`. No manual steps required.

## Authentication

1. Get a token:
```bash
curl -X POST http://localhost:8080/api/auth/token \
  -H "X-API-Key: sk_test_abc123"
```

2. Use the returned `tok_xxx` token on all other requests:
```
Authorization: Bearer tok_xxx
```

## API Examples

### Create Customer
```bash
curl -X POST http://localhost:8080/api/customers \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com", "name": "Jane Doe"}'
```

### Create Invoice
```bash
curl -X POST http://localhost:8080/api/invoices \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "<customer_uuid>",
    "invoice_number": "INV-2024-001",
    "line_items": [
      {"description": "Widget", "quantity": 2, "unit_price_cents": 1500}
    ]
  }'
```

### Pay Invoice (Success)
```bash
curl -X POST http://localhost:8080/api/invoices/<invoice_id>/pay \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: unique-key-123" \
  -d '{"payment_token": "tok_success"}'
```

### Pay Invoice (Failure)
```bash
curl -X POST http://localhost:8080/api/invoices/<invoice_id>/pay \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: unique-key-456" \
  -d '{"payment_token": "tok_card_declined"}'
```

### Payment Tokens
| Token | Behavior |
|-------|----------|
| `tok_success` | Returns `succeeded` after ~100ms |
| `tok_insufficient_funds` | Returns `failed` with code `insufficient_funds` |
| `tok_card_declined` | Returns `failed` with code `card_declined` |
| `tok_timeout` | Sleeps 30s (our client times out at 10s) |
| `tok_network_error` | Returns HTTP 500 |

## Running Tests

```bash
# Option 1: All in Docker (no local Rust needed)
docker compose --profile test up --build

# Option 2: Locally (requires Rust installed)
docker compose up -d
cargo test --test payment_tests -- --nocapture
```

### Required Tests (from spec)

| Test | What it proves |
|------|---------------|
| `test_concurrent_payments_no_double_charge` | 10 concurrent POST /pay → exactly 1 succeeds, rest get 409 |
| `test_idempotency_returns_cached_response` | Same key = same payment_id, different payload = 422 |
| `test_psp_failure_does_not_corrupt_state` | tok_timeout → invoice `failed` (not stuck), retry succeeds |

## Project Structure

```
src/
├── main.rs              # Bootstrap
├── config/
│   ├── settings.rs      # All env vars (like Django settings.py)
│   ├── database.rs      # Pool + migrations
│   └── state.rs         # AppState (shared across handlers)
├── router.rs            # Route tree + middleware layers
├── response.rs          # Common success wrapper {"status":"success","data":...}
├── errors.rs            # Common error enum {"status":"error","error":{...}}
├── extractors.rs        # Custom JSON extractor (clean validation errors)
├── validators/          # Input validation (regex email, field rules)
├── db/                  # SQL queries only (repository layer)
├── services/            # Business logic (orchestration)
├── routes/              # Thin HTTP handlers
├── middleware/          # Auth (token + API key)
├── models/              # Data types + state machine
└── workers/             # Background webhook delivery
mock-psp/src/main.rs     # Simulated payment processor
migrations/001_init.sql  # Database schema
tests/payment_tests.rs   # 3 required integration tests
```

## Key Design Decisions

1. **Money in integer cents** — no floating point anywhere
2. **Server-computed invoice totals** — never trust client
3. **User-provided invoice numbers** — validated with regex, unique per business
4. **Pessimistic locking** — `SELECT FOR UPDATE` prevents double charge
5. **Split transaction around PSP call** — don't hold DB locks during network I/O
6. **Idempotency with payload hash** — detects key reuse with different requests
7. **Webhook outbox** — atomic with payment state, delivered async
8. **Two-tier auth** — API key → session token, both SHA256-hashed
9. **Centralized settings** — all config from env vars, no scattered `env::var()` calls

See [DESIGN.md](./DESIGN.md) for detailed architecture decisions and failure analysis.
