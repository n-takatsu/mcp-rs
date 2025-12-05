# Multi-stage build for MCP-RS production deployment
# Stage 1: Build
FROM rust:1.83-slim-bullseye AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    libmariadb-dev \
    libsqlite3-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY benches ./benches
COPY examples ./examples
COPY tests ./tests
COPY configs ./configs

# Build release binary
RUN cargo build --release --bin mcp-rs-server

# Stage 2: Runtime
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    libmariadb3 \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1001 -s /bin/bash mcp

# Create necessary directories
RUN mkdir -p /app/configs /app/logs /var/log/mcp-rs && \
    chown -R mcp:mcp /app /var/log/mcp-rs

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/mcp-rs-server /usr/local/bin/mcp-rs-server

# Copy configuration files
COPY --from=builder /app/configs /app/configs

# Set permissions
RUN chmod +x /usr/local/bin/mcp-rs-server && \
    chown -R mcp:mcp /app

# Switch to non-root user
USER mcp

# Expose ports
EXPOSE 3000 8080 8443

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Set environment variables
ENV RUST_LOG=info \
    MCP_SERVER_PORT=3000 \
    MCP_CONFIG_PATH=/app/configs/production/main.toml

# Run the application
CMD ["mcp-rs-server", "--config", "/app/configs/production/main.toml"]
