FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --workspace

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/invoice-service /usr/local/bin/
COPY --from=builder /app/target/release/mock-psp /usr/local/bin/
COPY migrations /app/migrations
WORKDIR /app
