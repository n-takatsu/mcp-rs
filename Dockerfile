# ========================================
# Multi-stage Dockerfile for mcp-rs
# Target: <50MB image size, <30s build time
# Security: Non-root, minimal distroless base
# ========================================

# Stage 1: Builder - Rust compilation
FROM rust:1.83-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install nightly toolchain for edition2024 support
RUN rustup toolchain install nightly && \
    rustup default nightly

WORKDIR /app

# Copy all source files
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

# Metadata
LABEL org.opencontainers.image.title="mcp-rs" \
      org.opencontainers.image.description="Model Context Protocol Server (Rust)" \
      org.opencontainers.image.version="0.15.0" \
      org.opencontainers.image.vendor="n-takatsu" \
      org.opencontainers.image.source="https://github.com/n-takatsu/mcp-rs"

WORKDIR /app

# Copy binary from builder
COPY --from=builder --chown=nonroot:nonroot /app/target/release/mcp-rs /app/mcp-rs

# Non-root user (distroless defaults to nonroot uid 65532)
USER nonroot:nonroot

# Expose default ports
EXPOSE 3000 3001

# Set environment variables
ENV RUST_LOG=info \
    MCP_SERVER_PORT=3000

# Default command: STDIO transport
ENTRYPOINT ["/app/mcp-rs"]
CMD ["--transport", "stdio"]
