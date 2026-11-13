# ========================================
# Multi-stage Dockerfile for mcp-rs
# Runtime: distroless, non-root
# ========================================

# Stage 1: Builder
# Build with the minimum supported Rust version (rust-version in
# Cargo.toml, #516). CI checks that the two match, so raise both together.
FROM rust:1.98-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy all source files. benches, examples and tests are not built, but
# Cargo.toml names some of them, so Cargo needs them to read the manifest.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches
COPY examples ./examples
COPY tests ./tests

# Build the application
RUN cargo build --release --bin mcp-rs

# Strip debug symbols to reduce size
RUN strip target/release/mcp-rs

# ========================================
# Stage 2: Runtime - Minimal distroless image
FROM gcr.io/distroless/cc-debian12:nonroot

# The version label is added by the release workflow (docker/metadata-action).
LABEL org.opencontainers.image.title="mcp-rs" \
      org.opencontainers.image.description="Model Context Protocol Server (Rust)" \
      org.opencontainers.image.vendor="n-takatsu" \
      org.opencontainers.image.source="https://github.com/n-takatsu/mcp-rs"

WORKDIR /app

# Copy binary from builder
COPY --from=builder --chown=nonroot:nonroot /app/target/release/mcp-rs /app/mcp-rs

# Non-root user (distroless defaults to nonroot uid 65532)
USER nonroot:nonroot

# HTTP mode only. The port is set in the config file ([transport.http] port,
# 8081 by default); STDIO mode uses no port.
EXPOSE 8081

# mcp-rs reads its settings from a config file. Mount one at
# /app/config/mcp-config.toml (for example -v ./config:/app/config:ro);
# without it, mcp-rs exits with "設定ファイルが存在しません".
# Logs go to standard error and to /app/logs (with a read-only root filesystem,
# /home/nonroot/.local/state/mcp-rs/logs). The audit log goes to
# /home/nonroot/.local/state/mcp-rs/audit.
ENTRYPOINT ["/app/mcp-rs"]
CMD ["--config", "/app/config/mcp-config.toml"]
