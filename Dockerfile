# ── Stage 1: Build ────────────────────────────────────────────────────────────
FROM rust:1.86-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY arc-x402/Cargo.toml arc-x402/
COPY crates/media-access/Cargo.toml crates/media-access/
COPY crates/access-gated-server/Cargo.toml crates/access-gated-server/
COPY examples/seller/Cargo.toml examples/seller/
COPY examples/buyer/Cargo.toml examples/buyer/
COPY examples/generate-keys/Cargo.toml examples/generate-keys/

# Stub out all crate src dirs so Cargo can resolve the dependency graph
RUN mkdir -p \
    arc-x402/src \
    crates/media-access/src \
    crates/access-gated-server/src \
    examples/seller/src \
    examples/buyer/src \
    examples/generate-keys/src && \
    echo "fn main(){}" > arc-x402/src/lib.rs && \
    echo "fn main(){}" > crates/media-access/src/lib.rs && \
    echo "fn main(){}" > crates/access-gated-server/src/main.rs && \
    echo "fn main(){}" > examples/seller/src/main.rs && \
    echo "fn main(){}" > examples/buyer/src/main.rs && \
    echo "fn main(){}" > examples/generate-keys/src/main.rs

# Cache dependencies
RUN cargo build --release --bin access-gated-server 2>/dev/null || true

# Copy real source and build for real
COPY arc-x402/src arc-x402/src/
COPY crates/media-access/src crates/media-access/src/
COPY crates/access-gated-server/src crates/access-gated-server/src/

RUN touch \
    arc-x402/src/lib.rs \
    crates/media-access/src/lib.rs \
    crates/access-gated-server/src/main.rs && \
    cargo build --release --bin access-gated-server

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -ms /bin/sh appuser

WORKDIR /app

COPY --from=builder /build/target/release/access-gated-server /app/access-gated-server

# SQLite data directory — mount a volume here in production
RUN mkdir -p /app/data && chown -R appuser:appuser /app

USER appuser

EXPOSE 3001

ENV DATABASE_URL=sqlite:///app/data/access.db \
    PORT=3001 \
    ARC_RPC_URL=https://rpc.testnet.arc.network \
    CHUNK_PRICE_ATOMIC=1000 \
    BUY_PRICE_ATOMIC=5000000

CMD ["/app/access-gated-server"]
