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

WORKDIR /app

# Copy dependency manifests first for better caching
COPY Cargo.toml Cargo.lock ./
RUN mkdir .cargo 2>/dev/null || true
COPY .cargo .cargo 2>/dev/null || true

# Create dummy source to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn lib() {}" > src/lib.rs

# Build dependencies only (cached layer)
RUN cargo build --release && \
    rm -rf src target/release/deps/mcp_rs* target/release/mcp-rs*

# Copy actual source code
COPY src ./src
COPY benches ./benches
COPY examples ./examples
COPY tests ./tests

# Build the actual application
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

# Copy configuration files
COPY --chown=nonroot:nonroot configs /app/configs
COPY --chown=nonroot:nonroot demo-policies /app/demo-policies

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
