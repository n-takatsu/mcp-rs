#!/bin/bash
set -e

echo "🚀 Setting up MCP-RS development environment..."

# Update package lists
sudo apt-get update

# Install additional development tools
echo "📦 Installing development tools..."
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libpq-dev \
    libmysqlclient-dev \
    sqlite3 \
    libsqlite3-dev \
    curl \
    wget \
    git \
    jq \
    tree \
    htop \
    vim \
    postgresql-client \
    mysql-client \
    redis-tools

# Install Rust components
echo "🦀 Installing Rust components..."
rustup component add clippy rustfmt rust-src
rustup target add wasm32-unknown-unknown

# Install additional Rust tools
echo "🔧 Installing Rust development tools..."
cargo install --locked \
    cargo-watch \
    cargo-expand \
    cargo-audit \
    cargo-outdated \
    cargo-tree \
    cargo-benchcmp \
    sqlx-cli \
    diesel_cli --no-default-features --features postgres,mysql,sqlite

# Install documentation tools
echo "📚 Installing documentation tools..."
cargo install mdbook mdbook-mermaid

# Install global Node.js packages for web development
echo "📱 Installing Node.js packages for web UI..."
npm install -g \
    typescript \
    @types/node \
    prettier \
    eslint \
    @typescript-eslint/parser \
    @typescript-eslint/eslint-plugin

# Set up Git configuration (if not already configured)
if [ -z "$(git config --global user.name)" ]; then
    echo "⚙️  Setting up Git configuration..."
    git config --global user.name "MCP-RS Developer"
    git config --global user.email "developer@mcp-rs.dev"
    git config --global init.defaultBranch main
    git config --global core.autocrlf input
    git config --global pull.rebase false
fi

# Create necessary directories
echo "📁 Creating project directories..."
mkdir -p ~/.cargo/registry
mkdir -p logs
mkdir -p data

# Set up environment variables
echo "🌍 Setting up environment variables..."
cat >> ~/.bashrc << 'EOF'

# MCP-RS Development Environment
export RUST_BACKTRACE=1
export RUST_LOG=debug
export CARGO_TARGET_DIR=/tmp/target
export PATH="$HOME/.cargo/bin:$PATH"

# Database URLs for development
export DATABASE_URL="postgresql://postgres:password@localhost:5432/mcp_rs_dev"
export MYSQL_URL="mysql://root:password@localhost:3306/mcp_rs_dev"
export MONGODB_URL="mongodb://localhost:27017/mcp_rs_dev"
export REDIS_URL="redis://localhost:6379"

# MCP Server configuration
export MCP_SERVER_PORT=3000
export MCP_LOG_LEVEL=debug

# Aliases for common commands
alias mcpbuild='cargo build --all-features'
alias mcptest='cargo test --all-features'
alias mcpclippy='cargo clippy --all-targets --all-features -- -D warnings'
alias mcprun='cargo run --features database -- --transport stdio'
alias mcprunhttp='cargo run --features database -- --transport http --port 3000'
alias mcpbench='cargo bench --features database'
alias mcpdoc='cargo doc --all-features --open'
alias mcpaudit='cargo audit'
alias mcpupdate='cargo update'
alias mcpclean='cargo clean'

# Quick project navigation
alias cdmcp='cd /workspaces/mcp-rs'
alias cdsrc='cd /workspaces/mcp-rs/src'
alias cdtest='cd /workspaces/mcp-rs/tests'
alias cdbench='cd /workspaces/mcp-rs/benches'
alias cddocs='cd /workspaces/mcp-rs/docs'
alias cdweb='cd /workspaces/mcp-rs/web-ui'

EOF

# Set up zsh configuration if zsh is installed
if command -v zsh &> /dev/null; then
    cat >> ~/.zshrc << 'EOF'

# MCP-RS Development Environment
export RUST_BACKTRACE=1
export RUST_LOG=debug
export CARGO_TARGET_DIR=/tmp/target
export PATH="$HOME/.cargo/bin:$PATH"

# Database URLs for development
export DATABASE_URL="postgresql://postgres:password@localhost:5432/mcp_rs_dev"
export MYSQL_URL="mysql://root:password@localhost:3306/mcp_rs_dev"
export MONGODB_URL="mongodb://localhost:27017/mcp_rs_dev"
export REDIS_URL="redis://localhost:6379"

# MCP Server configuration
export MCP_SERVER_PORT=3000
export MCP_LOG_LEVEL=debug

# Aliases for common commands
alias mcpbuild='cargo build --all-features'
alias mcptest='cargo test --all-features'
alias mcpclippy='cargo clippy --all-targets --all-features -- -D warnings'
alias mcprun='cargo run --features database -- --transport stdio'
alias mcprunhttp='cargo run --features database -- --transport http --port 3000'
alias mcpbench='cargo bench --features database'
alias mcpdoc='cargo doc --all-features --open'
alias mcpaudit='cargo audit'
alias mcpupdate='cargo update'
alias mcpclean='cargo clean'

# Quick project navigation
alias cdmcp='cd /workspaces/mcp-rs'
alias cdsrc='cd /workspaces/mcp-rs/src'
alias cdtest='cd /workspaces/mcp-rs/tests'
alias cdbench='cd /workspaces/mcp-rs/benches'
alias cddocs='cd /workspaces/mcp-rs/docs'
alias cdweb='cd /workspaces/mcp-rs/web-ui'

EOF
fi

# Build the project to cache dependencies
echo "🏗️  Building project and caching dependencies..."
cd /workspaces/mcp-rs
cargo fetch
cargo build --all-features

# Run initial checks
echo "🔍 Running initial project checks..."
cargo clippy --all-targets --all-features -- -D warnings || echo "⚠️  Clippy warnings found - please fix them"
cargo test --all-features || echo "⚠️  Some tests failed - please check them"

# Create development database setup script
echo "🗄️  Creating database setup script..."
cat > scripts/setup-dev-db.sh << 'EOF'
#!/bin/bash
set -e

echo "Setting up development databases..."

# PostgreSQL setup
echo "Setting up PostgreSQL..."
sudo service postgresql start
sudo -u postgres createdb mcp_rs_dev 2>/dev/null || echo "PostgreSQL database already exists"

# MySQL setup  
echo "Setting up MySQL..."
sudo service mysql start
mysql -u root -ppassword -e "CREATE DATABASE IF NOT EXISTS mcp_rs_dev;" 2>/dev/null || echo "MySQL database setup failed"

# MongoDB setup
echo "Setting up MongoDB..."
sudo service mongod start 2>/dev/null || echo "MongoDB service not available"

# Redis setup
echo "Setting up Redis..."
sudo service redis-server start 2>/dev/null || echo "Redis service not available"

echo "Development databases setup complete!"
EOF

chmod +x scripts/setup-dev-db.sh

# Create quick start script
cat > scripts/dev-start.sh << 'EOF'
#!/bin/bash
set -e

echo "🚀 Starting MCP-RS development session..."

# Start databases
./scripts/setup-dev-db.sh

# Show project status
echo "📊 Project Status:"
echo "  Rust version: $(rustc --version)"
echo "  Cargo version: $(cargo --version)"
echo "  Project location: $(pwd)"
echo "  Git branch: $(git branch --show-current 2>/dev/null || echo 'unknown')"

# Show useful commands
echo ""
echo "🛠️  Useful Development Commands:"
echo "  mcpbuild     - Build the project"
echo "  mcptest      - Run tests"
echo "  mcpclippy    - Run clippy checks"
echo "  mcprun       - Run MCP server (STDIO)"
echo "  mcprunhttp   - Run MCP server (HTTP)"
echo "  mcpbench     - Run benchmarks"
echo "  mcpdoc       - Generate and open documentation"
echo ""
echo "📁 Quick Navigation:"
echo "  cdmcp        - Go to project root"
echo "  cdsrc        - Go to source directory"
echo "  cdtest       - Go to tests directory"
echo "  cdbench      - Go to benchmarks directory"
echo ""
echo "✅ Development environment ready!"
EOF

chmod +x scripts/dev-start.sh

# Final setup message
echo "✅ MCP-RS development environment setup complete!"
echo ""
echo "🎉 You can now:"
echo "  • Use 'mcpbuild' to build the project"
echo "  • Use 'mcptest' to run tests"
echo "  • Use 'mcprun' to start the MCP server"
echo "  • Use './scripts/dev-start.sh' for a quick development session"
echo ""
echo "Happy coding! 🦀"