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
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin master_fiber_agent --bin fiber-agent-daemon

# ==========================================
# STAGE 3: MFA supervisor droplet
# ==========================================
FROM debian:bookworm-slim AS runtime-mfa
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --home-dir /app --shell /usr/sbin/nologin mfa
COPY --from=builder /app/target/release/master_fiber_agent /usr/local/bin/master_fiber_agent
USER mfa
EXPOSE 1025
ENTRYPOINT ["/usr/local/bin/master_fiber_agent"]

# ==========================================
# STAGE 4: Treasury Hub droplet (fiber-agent sidecar)
# ==========================================
FROM debian:bookworm-slim AS runtime-hub
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates sqlite3 curl && rm -rf /var/lib/apt/lists/*
RUN useradd --system --create-home --home-dir /app --shell /usr/sbin/nologin sidecar
COPY --from=builder /app/target/release/fiber-agent-daemon /usr/local/bin/fiber-agent-daemon
USER sidecar
EXPOSE 19444
ENTRYPOINT ["/usr/local/bin/fiber-agent-daemon"]
