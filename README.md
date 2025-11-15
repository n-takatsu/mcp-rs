# mcp-rs

🚀 **Production-Ready** Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

> **[English](README.md)** | **[日本語](README.ja.md)**

[![Version](https://img.shields.io/badge/Version-v0.15.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.15.0)
[![Architecture](https://img.shields.io/badge/Architecture-Production--Ready-green)](#architecture)
[![Implementation](https://img.shields.io/badge/WordPress_Tools-27_Available-green)](#wordpress-mcp-tools)
[![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)](#license)

## Overview

`mcp-rs` provides a **comprehensive, battle-tested** implementation of the MCP (Model Context Protocol) in Rust with **complete WordPress integration**. Built with a layered architecture, it enables AI agents to perform sophisticated content management through a standardized JSON-RPC interface. The framework features 27 WordPress tools, enterprise-grade security, strong type safety, and is optimized for production use with GitHub Copilot and other AI coding assistants.

## 🎯 Who Is This For?

### **AI Developers** 🤖
Building Claude Desktop apps, GPT integrations, or custom AI agents? mcp-rs provides production-ready Model Context Protocol implementation with comprehensive WordPress tooling.

### **Enterprise WordPress Teams** 🏢  
Managing large-scale WordPress deployments? Get enterprise-grade security, automated content management, and seamless CI/CD integration.

### **DevOps Engineers** ⚙️
Automating WordPress operations? 27 battle-tested tools with comprehensive health checks, monitoring, and production-ready error handling.

### **Rust Enthusiasts** 🦀
Want to contribute to a high-quality Rust codebase? Join our 205+ test, zero-warning project with clean architecture and comprehensive documentation.

### **Security Teams** 🔒
Need WordPress security automation? 6-layer enterprise security architecture with SQL injection protection, XSS prevention, and comprehensive audit logging.

## 🚀 Key Features

### **Core Capabilities**
- **JSON-RPC 2.0 Server**: Full-featured JSON-RPC server implementation using `axum`
- **Multi-Transport Support**: STDIO, HTTP, and WebSocket communication protocols
- **Plugin Architecture**: Handler-based system with `McpHandler` trait for extensibility
- **Type-Safe Configuration**: TOML-based configuration with environment variable override
- **Production-Ready Error Handling**: Comprehensive error types with structured logging
- **Async/Await**: Built on `tokio` for high-performance async operations

### **WordPress Integration (27 Tools)**
- **Posts & Pages Management**: Full CRUD operations with SEO integration
- **Advanced Media Management**: Upload, resize, organize with base64 support
- **Categories & Tags Management**: Hierarchical support with bulk operations
- **Comment System**: Complete comment management and retrieval
- **YouTube & Social Embeds**: Rich media integration with security validation
- **User Management**: Role-based access control and user operations

### **Enterprise Security (6-Layer Architecture)**
- **AES-GCM-256 Encryption**: Military-grade encryption with PBKDF2 key derivation
- **SQL Injection Protection**: Real-time detection of 11 attack patterns
- **XSS Prevention**: Advanced protection against 14 XSS attack vectors
- **Rate Limiting**: Token bucket algorithm with DDoS protection
- **TLS Enforcement**: TLS 1.2+ with certificate validation
- **Audit Logging**: Comprehensive security event tracking

### **Technical Excellence**
- **Async Architecture**: Built on Tokio for high-performance concurrency
- **Type Safety**: 100% memory-safe Rust implementation
- **Comprehensive Testing**: 205+ tests with 100% pass rate
- **Zero Warnings**: Clean codebase with zero clippy warnings
- **Production Ready**: Optimized build profiles and error handling

## 📊 Quality Metrics

| Metric | Value |
|--------|-------|
| **Total Tests** | 205+ (100% passing) |
| **Code Coverage** | Comprehensive |
| **Security Tests** | 6-layer validation |
| **Performance** | Production optimized |
| **Documentation** | Complete API docs |

## 🚀 Quick Start

### Prerequisites
- Rust 1.70+ (2021 edition)
- WordPress site with Application Passwords enabled
- Network access to WordPress REST API

### Installation

#### 🚨 Important Notice for Claude Desktop Users

**Claude Desktop uses STDIO (standard input/output) communication. Log messages mixed with standard output will break communication, so always use the dedicated configuration.**

```bash
# For Claude Desktop (Important: Use dedicated config)
cargo run -- --config configs/production/claude-desktop.toml

# For Web UI (HTTP access)
cargo run -- --config configs/development/http-transport.toml
```

> 📖 See [Claude Desktop Integration Guide](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md) for detailed configuration.

#### Option 1: Interactive Setup (Recommended)

```bash
# Clone repository
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs

# Build release version
cargo build --release

# Run interactive configuration setup
./target/release/mcp-rs --setup-config
```

Interactive setup features:
- 📝 User-friendly question format
- 🔍 Real-time connection testing  
- ⚡ Automatic configuration file generation
- 🛡️ Security setting recommendations

#### Option 2: Manual Configuration

```bash
# Generate sample configuration file
./target/release/mcp-rs --generate-config

# Edit configuration
cp mcp-config.toml.example mcp-config.toml
# Edit mcp-config.toml with your WordPress details

# Run with custom config
./target/release/mcp-rs --config mcp-config.toml
```

### Basic Configuration

Create `mcp-config.toml`:

```toml
[wordpress]
base_url = "https://your-wordpress-site.com"
username = "your-username"
password = "your-application-password"  # WordPress Application Password

[server]
transport_type = "stdio"  # For Claude Desktop
# transport_type = "http"  # For Web UI
# bind_addr = "127.0.0.1:8080"  # HTTP mode only

[logging]
level = "error"  # Minimal logging for Claude Desktop
# level = "info"  # Detailed logging for development
```

## 🏗️ Architecture

```
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│   AI Agent/Client   │───▶│    MCP Server       │───▶│   WordPress Site    │
│  (Claude Desktop,   │    │   (mcp-rs)          │    │   (REST API)        │
│   Custom AI, etc)   │    │                     │    │                     │
└─────────────────────┘    └─────────────────────┘    └─────────────────────┘
                                      │
                                      ▼
                           ┌─────────────────────┐
                           │   Security Layer    │
                           │ • SQL Injection     │
                           │ • XSS Protection    │
                           │ • Rate Limiting     │
                           │ • Encryption        │
                           └─────────────────────┘
```

## 📚 Documentation

- [🚀 Quick Start Guide](./project-docs/quick-start.md)
- [🔧 Configuration Reference](./project-docs/configuration.md)
- [🔒 Security Guide](./project-docs/security.md)
- [🏗️ Architecture Overview](./project-docs/architecture.md)
- [📝 API Reference](./project-docs/api-reference.md)
- [🔄 Claude Desktop Integration](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md)

## 🛠️ Development

### Building from Source

```bash
# Development build
cargo build

# Production build with optimizations
cargo build --release

# Run tests
cargo test

# Run with specific configuration
cargo run -- --config configs/development/http-transport.toml
```

### Examples

The project includes comprehensive examples demonstrating various features:

- **Core Examples**: Located in `examples/` directory
- **Database Examples**: Located in `examples.disabled/` directory
  - `mysql_engine_test.rs`: MySQL database engine testing (requires `database` feature)
  - To use: `cargo run --example mysql_engine_test --features database,mysql-backend`

> **Note**: Database-dependent examples are moved to `examples.disabled/` to ensure CI stability when the `database` feature is not enabled by default.

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test module
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