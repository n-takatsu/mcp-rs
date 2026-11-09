# mcp-rs

🚀 **Production-Ready** Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

> **[English](README.md)** | **[日本語](README.ja.md)**

[![Version](https://img.shields.io/badge/Version-v0.19.2-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.19.2)
![Architecture](https://img.shields.io/badge/Architecture-Production--Ready-green)
![Implementation](https://img.shields.io/badge/WordPress_Tools-32_Available-green)
![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)

## Overview

`mcp-rs` provides a **comprehensive, battle-tested** implementation of the MCP (Model Context Protocol) in Rust with **complete WordPress integration**. Built with a layered architecture, it enables AI agents to perform sophisticated content management through a standardized JSON-RPC interface. The framework features 32 WordPress tools, enterprise-grade security, strong type safety, and is optimized for production use with GitHub Copilot and other AI coding assistants.

## 🎯 Who Is This For?

### AI Developers 🤖

Building Claude Desktop apps, GPT integrations, or custom AI agents? mcp-rs provides production-ready Model Context Protocol implementation with comprehensive WordPress tooling.

### Enterprise WordPress Teams 🏢

Managing large-scale WordPress deployments? Get enterprise-grade security, automated content management, and seamless CI/CD integration.

### DevOps Engineers ⚙️

Automating WordPress operations? 32 battle-tested tools with comprehensive health checks, monitoring, and production-ready error handling.

### Rust Enthusiasts 🦀

Want to contribute to a high-quality Rust codebase? Join our 205+ test, zero-warning project with clean architecture and comprehensive documentation.

### Security Teams 🔒

Exposing mcp-rs over HTTP? Use API keys or OAuth 2.1 tokens, limit the tools each caller can use, and keep an audit log of every tool call.

## 🚀 Key Features

## **Core Capabilities**

- **JSON-RPC 2.0 Server**: Full-featured JSON-RPC server implementation using `axum`
- **Multi-Transport Support**: STDIO, HTTP, and WebSocket communication protocols
- **Plugin Architecture**: Handler-based system with `McpHandler` trait for extensibility
- **Type-Safe Configuration**: TOML-based configuration with environment variable override
- **Production-Ready Error Handling**: Comprehensive error types with structured logging
- **Async/Await**: Built on `tokio` for high-performance async operations

## **WordPress Integration (32 Tools)**

- **Posts & Pages Management**: Create, read (full editable source), update, and delete posts and pages, with SEO metadata and scheduling
- **Media Management**: Upload with base64 (type, extension, and content validation), list, update metadata, delete, and set featured images
- **Categories & Tags Management**: Create, update, and delete, with hierarchical categories
- **Comments**: Retrieve comments
- **YouTube & Social Embeds**: Rich media integration with security validation
- **Site Settings**: Read and update site settings and the front page configuration
- **Health Check**: Verify connectivity, authentication, permissions, and media upload capability

### Live Demo Site

The `examples/wordpress_site_builder_demo.rs` example builds and updates the public WordPress demo site at [redring.jp](https://redring.jp/). It is a real-world showcase of how mcp-rs can bootstrap a site and organize content in production.

## **Security**

- **Encrypted Credentials**: The WordPress Application Password can be stored encrypted with AES-GCM-256 (PBKDF2 key derivation), or read from an environment variable
- **HTTP Mode Authentication**: API keys, or OAuth 2.1 access tokens (JWT) issued by an external authorization server
- **Per-Caller Tool Limits**: Allow each API key or token only the tools it needs (for example `@read_only`)
- **TLS/mTLS**: HTTPS with a minimum TLS version, and optional client certificates
- **Network Policy, Anti-Replay and Intrusion Detection** for the HTTP/WebSocket listener
- **Rate Limiting** of tool calls
- **Audit Logging**: Who called which tool, and the result

## **Database Integration (Phase 1: MySQL)**

> **Note**: The database handler is library code behind the optional `database` feature. It is not connected to the `mcp-rs` binary yet, so MCP clients cannot use it ([#453](https://github.com/n-takatsu/mcp-rs/issues/453)).

- **Parameterized Queries**: SQL injection prevention through parameter binding
- **Transaction Management**: Full ACID compliance with savepoint support
- **Multiple Isolation Levels**: READ UNCOMMITTED through SERIALIZABLE
- **Type-Safe Operations**: Rust type system enforces data safety
- **Comprehensive Testing**: 45+ tests covering security and performance

## **Technical Excellence**

- **Async Architecture**: Built on Tokio for high-performance concurrency
- **Type Safety**: 100% memory-safe Rust implementation
- **Comprehensive Testing**: 205+ tests with 100% pass rate
- **Zero Warnings**: Clean codebase with zero Rust linter warnings
- **Production Ready**: Optimized build profiles and error handling

## 📊 Quality Metrics

| Metric | Value |
| ------ | ----- |
| **Total Tests** | 260+ (100% passing) |
| **Code Coverage** | Comprehensive |
| **Security Tests** | 8-layer validation |
| **Performance** | Production optimized |
| **Documentation** | Complete API docs |

## 🚀 Quick Start

## Prerequisites

- Rust 1.70+ (2021 edition)
- WordPress site with Application Passwords enabled
- Network access to WordPress REST API

## Installation

### 🚨 Important Notice for Claude Desktop Users

**Claude Desktop uses STDIO (standard input/output) communication. Log messages mixed with standard output will break communication, so always use the dedicated configuration.**

```bash

## For Claude Desktop (Important: Use dedicated config)

cargo run -- --config configs/production/claude-desktop.toml

## For Web UI (HTTP access)

cargo run -- --config configs/development/http-transport.toml
```

> 📖 See [Claude Desktop Integration Guide](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md) for detailed configuration.

### Claude Code

Claude Code also talks to mcp-rs over STDIO and uses the same configuration file. Build mcp-rs, then register it with absolute paths. Everything after `--` is the mcp-rs command line.

```bash
cargo build --release

claude mcp add mcp-rs --scope user \
  -e WORDPRESS_URL=https://your-wordpress-site.com \
  -e WORDPRESS_USERNAME=your_username \
  -e WORDPRESS_PASSWORD="your application password" \
  -- /path/to/mcp-rs/target/release/mcp-rs --config /path/to/mcp-rs/configs/production/claude-desktop.toml
```

Check the connection with `claude mcp list`, or `/mcp` inside Claude Code. Values passed with `-e` are stored in Claude Code's settings, so do not use `--scope project` (which writes them to the shared `.mcp.json`) with real credentials.

> 📖 See [Claude Code Integration Guide](./project-docs/CLAUDE_CODE_INTEGRATION.md) (Japanese) for `.mcp.json`, encrypted credentials, and troubleshooting.

### Setup

```bash
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo build --release
```

Then start mcp-rs with a config file. Ready-made config files are in `configs/production/`:

- `claude-desktop.toml`: STDIO, for Claude Desktop and Claude Code. WordPress credentials are read from the `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD` environment variables
- `web-ui.toml`: HTTP, for web clients

```bash
./target/release/mcp-rs --config configs/production/claude-desktop.toml
```

`mcp-rs --version` prints the installed version, and `mcp-rs --help` lists the options (v0.19.1 and later). mcp-rs does not start when it gets an unknown option or `--config` without a path (v0.19.2 and later; v0.19.1 only logged a warning), so check the `args` of the mcp-rs entry in your MCP client's settings if it stops starting.

From an MCP client (for example, ask Claude), call the `mcp_rs_status` tool to see the version, the config file in use, where the log and audit log are written, and the state of each plugin (v0.19.2 and later).

To write your own config file instead, start from the minimal example below.

## Basic Configuration

Create `mcp-config.toml`:

```toml
[transport]
type = "stdio"          # "http" for HTTP mode (see [transport.http])

[server]
log_level = "warn"      # Logs go to a file, never to standard output in STDIO mode

[handlers.wordpress]
enabled = true
url = "https://your-wordpress-site.com"
username = "your-username"
password = "your-application-password"   # WordPress Application Password
```

Then run `./target/release/mcp-rs --config mcp-config.toml`. To keep the password out of the file, use `${WORDPRESS_PASSWORD}` or [encrypted credentials](https://n-takatsu.github.io/mcp-rs/docs/security/#encrypted-wordpress-credentials). mcp-rs does not start if a `${...}` variable is not set.

### HTTP TLS/mTLS Configuration (Optional)

When using HTTP transport with TLS and client certificate authentication (mTLS), set:

```toml
[transport]
transport_type = "http"

[transport.http]
addr = "127.0.0.1"
port = 8080
tls_enabled = true
tls_cert_path = "./certs/server.crt"
tls_key_path = "./certs/server.key"
mtls_enabled = true
mtls_ca_cert_path = "./certs/ca.crt"
enforce_https = true
min_tls_version = "1.3"
hsts_enabled = true
```

Notes:

- `mtls_enabled = true` requires `tls_enabled = true`
- `mtls_enabled = true` requires `mtls_ca_cert_path`
- Clients without a valid certificate signed by the configured CA are rejected at TLS handshake

### WebSocket

WebSocket is not a separate transport. In HTTP mode, set `enable_websocket_upgrade = true` to also accept MCP requests on `/ws`:

```toml
[transport]
transport_type = "http"

[transport.http]
enable_websocket_upgrade = true
```

## 🏗️ Architecture

```text
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│   AI Agent/Client   │───▶│    MCP Server       │───▶│   WordPress Site    │
│  (Claude Desktop,   │    │   (mcp-rs)          │    │   (REST API)        │
│   Custom AI, etc)   │    │                     │    │                     │
└─────────────────────┘    └─────────────────────┘    └─────────────────────┘
                                      │
                                      ▼
                           ┌─────────────────────┐
                           │   Security Layer    │
                           │ • Authentication    │
                           │ • Tool Limits       │
                           │ • Rate Limiting     │
                           │ • Audit Log         │
                           └─────────────────────┘
```

## 📚 Documentation

- [🚀 Quick Start Guide](./project-docs/quick-start.md)
- [🔧 Configuration Reference](./project-docs/configuration.md)
- [🔒 Security Guide](./project-docs/security.md)
- [🛡️ RBAC (Role-Based Access Control)](./docs/RBAC.md)
- [🏗️ Architecture Overview](./project-docs/architecture.md)
- [📝 API Reference](./project-docs/api-reference.md)
- [🔄 Claude Desktop Integration](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md)
- [🔄 Claude Code Integration](./project-docs/CLAUDE_CODE_INTEGRATION.md)
- [🐳 Docker Guide](./docs/DOCKER_GUIDE.md) ⭐ NEW

## 🔐 RBAC Quick Start

The database handler includes a Role-Based Access Control (RBAC) system for fine-grained security. Like the rest of the database handler, it is not connected to the `mcp-rs` binary yet ([#453](https://github.com/n-takatsu/mcp-rs/issues/453)).

### Key Features

- **Role Hierarchy**: Inheritance-based permission management
- **Time-Based Access**: Business hours, emergency access, break periods
- **IP Restrictions**: CIDR-based network access control
- **Column-Level Security**: Read/write permissions per column
- **Row-Level Security**: Ownership-based access control
- **Data Masking**: Full, Partial, Hash, and Tokenize strategies

### Basic Usage

```rust
use mcp_rs::handlers::database::integrated_security::*;

// Assign role to user
security_manager.assign_user_role("user123", "developer").await?;

// Check access (automatically enforces all RBAC policies)
let context = QueryContext::new();
context.insert("user_id", "user123");
context.insert("client_ip", "10.0.1.50");

let decision = security_manager.check_authentication_and_authorization(
    "user123",
    "SELECT * FROM users",
    &context
).await?;

// Apply data masking
let masked_email = security_manager.apply_data_masking(
    "user123",
    "users",
    "email",
    "john.doe@example.com"
).await?;
// Result for 'support' role: "joh***************"
// Result for 'admin' role: "john.doe@example.com"
```

### Configuration Example

```toml
[security.rbac]
enabled = true
default_role = "read_only"

[security.rbac.roles.developer]
description = "Developer with limited write access"
permissions = { "dev_db.*" = ["Read", "Write"] }

[security.rbac.time_based_access]
enabled = true
timezone = "UTC"

[security.rbac.time_based_access.business_hours.Monday]
start = "09:00:00"
end = "17:00:00"

[security.rbac.ip_restrictions]
enabled = true
allowed_ranges = ["10.0.0.0/8", "192.168.1.0/24"]
```

📖 **Full Documentation**: See [RBAC Guide](./docs/RBAC.md) for comprehensive configuration and usage examples.

## 🛠️ Development

## Building from Source

```bash

## Development build

cargo build

## Production build with optimizations

cargo build --release

## Run tests

cargo test

## Run with specific configuration

cargo run -- --config configs/development/http-transport.toml
```

## Code Examples

The project includes comprehensive examples demonstrating various features:

- **Core Examples**: Located in `examples/` directory
- **Database Examples**: Located in `examples.disabled/` directory
  - `mysql_engine_test.rs`: MySQL database engine testing (requires `database` feature)
  - To use: `cargo run --example mysql_engine_test --features database,mysql-backend`

> **Note**: Database-dependent examples are moved to `examples.disabled/` to ensure CI stability when the `database` feature is not enabled by default.

## Testing

```bash

## Run all tests

cargo test

## Run with output

cargo test -- --nocapture  # Show test output (no-capture flag) # cSpell:ignore nocapture

## Run specific test module

cargo test wordpress_api
```

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is dual-licensed under:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

You may choose either license at your option.

## 🙏 Acknowledgments

- [Model Context Protocol](https://modelcontextprotocol.io/) by Anthropic
- [WordPress REST API](https://developer.wordpress.org/rest-api/)
- The Rust community for excellent tooling and libraries

## 📞 Support

- 📖 [Documentation](./docs/)
- 🐛 [Issue Tracker](https://github.com/n-takatsu/mcp-rs/issues)
- 💬 [Discussions](https://github.com/n-takatsu/mcp-rs/discussions)

---

Built with ❤️ in Rust 🦀
