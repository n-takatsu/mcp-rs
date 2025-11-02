# Getting Started with MCP-RS

This guide will help you get MCP-RS up and running quickly.

## Prerequisites

- Rust 1.70 or later
- Git for cloning the repository

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/RedRing2020/mcp-rs.git
cd mcp-rs

# Build in release mode
cargo build --release

# The binary will be available at ./target/release/mcp-rs
```

### Development Build

```bash
# For development with debug symbols
cargo build

# Run tests
cargo test

# Run with debug logging
cargo run -- --debug
```

## Basic Usage

### STDIO Transport (Standard Mode)

For use with MCP clients like Claude Desktop:

```bash
./target/release/mcp-rs --stdio
```

### TCP Transport (Development Mode)

For development and testing:

```bash
./target/release/mcp-rs --bind-address 127.0.0.1 --port 8080
```

## Configuration

### Basic Configuration File

Create a `config.toml` file:

```toml
[server]
name = "mcp-rs-server"
version = "1.0.0"

[logging]
level = "info"
format = "json"
output = "stdout"

[plugins]
enabled = ["wordpress", "github"]

[plugins.wordpress]
api_url = "https://your-site.com/wp-json/wp/v2"
username = "your-username"
password = "your-app-password"

[plugins.github]
token = "your-github-token"
default_owner = "your-username"
```

### Environment Variables

```bash
export MCP_LOG_LEVEL=debug
export MCP_GITHUB_TOKEN=your-token
export MCP_WORDPRESS_API_URL=https://your-site.com/wp-json/wp/v2
```

## Plugin System

### Enable Specific Plugins

```bash
# Enable only WordPress plugin
./target/release/mcp-rs --enable-plugin wordpress

# Enable multiple plugins
./target/release/mcp-rs --enable-plugin wordpress --enable-plugin github
```

### Plugin Configuration

Each plugin can be configured independently through the configuration file or environment variables.

## Next Steps

- [Configuration Guide](configuration.html) - Detailed configuration options
- [Plugin Development](plugin-development.html) - Creating custom plugins
- [Examples](examples.html) - Common usage patterns
- [Architecture Overview](../architecture/overview.html) - Understanding the system design