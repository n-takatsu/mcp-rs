# ========================================
# Multi-stage Dockerfile for mcp-rs
# Runtime: distroless/static, non-root. mcp-rs is one statically linked
# executable (musl), so the image has no C library, OpenSSL or libstdc++
# (#631: their Debian packages made most of the image scanner's alerts)
# ========================================

# Stage 1: Builder
# Build with the minimum supported Rust version (rust-version in
# Cargo.toml, #516). CI checks that the two match, so raise both together.
FROM rust:1.98-slim-bookworm AS builder

# musl-tools gives musl-gcc, which builds the C parts of the dependencies
# (ring, aws-lc-sys, the bundled SQLite) for the musl target. The image is
# built natively for each platform (linux/amd64, linux/arm64), so the target
# follows the build machine
RUN apt-get update && apt-get install -y --no-install-recommends \
    musl-tools \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add "$(uname -m)-unknown-linux-musl"

WORKDIR /app

# Copy all source files. benches, examples and tests are not built, but
# Cargo.toml names some of them, so Cargo needs them to read the manifest.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches
COPY examples ./examples
COPY tests ./tests
# サーバーの情報のアイコン（#570）。include_bytes!で埋め込む
COPY website/assets/favicon-32.png website/assets/apple-touch-icon.png website/assets/favicon.svg ./website/assets/

# Build the application. The os-keyring feature (#533) is left out: a
# container has no OS credential store, so credentials come from environment
# variables (${VAR}) or encrypted_credentials. The database engines are the
# same as the default build (PostgreSQL, MySQL, MariaDB, SQLite, Redis,
# MongoDB; Redis and MongoDB were missing until PR #822's review)
RUN target="$(uname -m)-unknown-linux-musl" \
    && cargo build --release --target "$target" --bin mcp-rs --no-default-features --features database,mysql-backend,redis-backend,mongodb-backend \
    && cp "target/$target/release/mcp-rs" /app/mcp-rs \
    && strip /app/mcp-rs

# The runtime image has no C library, so stop here if the executable still
# needs a shared library or a dynamic loader (a static-pie has neither)
RUN if readelf -d /app/mcp-rs | grep -q NEEDED || readelf -l /app/mcp-rs | grep -q interpreter; then \
        echo "mcp-rs is not statically linked" >&2; exit 1; \
    fi

# ========================================
# Stage 2: Runtime - distroless/static (CA certificates, tzdata, nonroot user)
FROM gcr.io/distroless/static-debian12:nonroot

# The version label is added by the release workflow (docker/metadata-action).
LABEL org.opencontainers.image.title="mcp-rs" \
      org.opencontainers.image.description="Model Context Protocol Server (Rust)" \
      org.opencontainers.image.vendor="n-takatsu" \
      org.opencontainers.image.source="https://github.com/n-takatsu/mcp-rs"

WORKDIR /app

# Copy binary from builder
COPY --from=builder --chown=nonroot:nonroot /app/mcp-rs /app/mcp-rs

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
