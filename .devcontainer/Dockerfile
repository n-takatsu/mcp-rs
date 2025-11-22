# MCP-RS Development Environment - Lightweight Version
FROM mcr.microsoft.com/devcontainers/rust:2-1-bullseye

# Install system dependencies
# cSpell:ignore libpq libmariadb libsqlite
RUN apt-get update && apt-get install -y \
    # Build tools
    build-essential \
    pkg-config \
    libssl-dev \
    # Database libraries
    libpq-dev \
    libmariadb-dev \
    libsqlite3-dev \
    # Development tools
    curl \
    wget \
    git \
    vim \
    zsh \
    # Clean up
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js LTS
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y nodejs

# Install Rust components
RUN rustup component add clippy rustfmt rust-src # cSpell:ignore rustup clippy

# Set up environment variables
ENV RUST_BACKTRACE=1
ENV RUST_LOG=debug
ENV DATABASE_URL=postgresql://postgres:password@postgres:5432/mcp_rs_dev
ENV MYSQL_URL=mysql://root:password@mysql:3306/mcp_rs_dev
ENV MONGODB_URL=mongodb://mongodb:27017/mcp_rs_dev
ENV REDIS_URL=redis://redis:6379
ENV MCP_SERVER_PORT=3000
ENV MCP_LOG_LEVEL=debug

# Create workspace directory
WORKDIR /workspace

# Install Oh My Zsh (with cleanup check)
RUN rm -rf /root/.oh-my-zsh 2>/dev/null || true && \
    sh -c "$(curl -fsSL https://raw.github.com/ohmyzsh/ohmyzsh/master/tools/install.sh)" "" --unattended

# Default command
CMD ["/bin/zsh"]