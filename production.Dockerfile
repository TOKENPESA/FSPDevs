# ==========================================
# STAGE 1: Recipe Planner (Cargo Chef Optimization)
# ==========================================
FROM rust:1.78-slim AS planner
WORKDIR /app
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ==========================================
# STAGE 2: Dependency Cacher & Builder
# ==========================================
FROM rust:1.78-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev git clang make && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
# Build and cache dependencies only
RUN cargo chef cook --release --recipe-path recipe.json

# Copy full workspace source and compile production targets
COPY . .
RUN cargo build --release --bin master-fiber-agent --bin fiber-agent

# ==========================================
# STAGE 3: Hardened Runtime (MFA Profile)
# ==========================================
FROM debian:bookworm-slim AS runtime-mfa
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates ufw && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/master-fiber-agent /usr/local/bin/master-fiber-agent
ENTRYPOINT ["/usr/local/bin/master-fiber-agent"]

# ==========================================
# STAGE 4: Hardened Runtime (Treasury Hub Profile)
# ==========================================
FROM debian:bookworm-slim AS runtime-hub
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates sqlite3 ufw && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/fiber-agent /usr/local/bin/fiber-agent
ENTRYPOINT ["/usr/local/bin/fiber-agent"]