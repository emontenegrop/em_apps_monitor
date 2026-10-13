# Build context: workspace root (em_apps_monitor/)
# docker-compose.yml must set: context: . / dockerfile: backend/Dockerfile

# ── Build stage ───────────────────────────────────────────────────────────
FROM rust:slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 1. Copy workspace manifests + frozen lockfile for reproducible dep resolution
COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml ./backend/
COPY crates/protocol/Cargo.toml ./crates/protocol/

# 2. Dummy binary to cache compiled dependencies before copying real src
RUN mkdir -p backend/src crates/protocol/src && echo 'fn main(){}' > backend/src/main.rs && \
    touch crates/protocol/src/lib.rs && \
    cargo build --release -p em_monitor || true && \
    rm -rf backend/src crates/protocol/src

# 3. Copy real source and build
COPY backend/src        ./backend/src
COPY backend/migrations ./backend/migrations
COPY crates/protocol/src ./crates/protocol/src

# Touch main.rs to invalidate cached binary and force full re-link
RUN touch backend/src/main.rs crates/protocol/src/lib.rs && cargo build --release -p em_monitor

# ── Runtime stage ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/em_monitor /usr/local/bin/em_monitor
# Migrations are embedded at compile time by sqlx::migrate!() — no runtime files needed

WORKDIR /app
ENV RUST_LOG=em_monitor=info

EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/em_monitor"]
