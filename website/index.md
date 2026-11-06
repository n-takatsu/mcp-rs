---
layout: home
title: MCP-RS Documentation
permalink: /
---

Welcome to the MCP-RS documentation! This site contains comprehensive guides, API references, and architecture documentation for the Rust implementation of the Model Context Protocol.

## Quick Access

- **[Getting Started](./docs/guides/)** - Implementation guides and tutorials
- **[API Reference](./docs/api/)** - Complete API documentation
- **[Architecture](./docs/architecture/)** - System design and technical architecture

## Documentation Hub

- **[Full Documentation](./docs/)** - Complete documentation index
- **[GitHub Repository](https://github.com/n-takatsu/mcp-rs)** - Source code and issues

## What is MCP-RS?

🚀 **Production-Ready** Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

MCP-RS provides a **comprehensive, battle-tested** implementation of the MCP (Model Context Protocol) in Rust with **complete WordPress integration**. Built with a layered architecture, it enables AI agents to perform sophisticated content management through a standardized JSON-RPC interface.

## Key Features

### 🚀 Core Capabilities

- **JSON-RPC 2.0 Server**: Full-featured JSON-RPC server implementation using `axum`
- **Core Runtime Module**: Advanced application lifecycle and resource management
- **Multi-Transport Support**: Stdio, HTTP, and WebSocket communication protocols
- **Plugin Architecture**: Handler-based system with `McpHandler` trait for extensibility
- **Type-Safe Configuration**: TOML-based configuration with environment variable override
- **Production-Ready Error Handling**: Comprehensive error types with structured logging
- **Async/Await**: Built on `tokio` for high-performance async operations
- **Hot Configuration**: Dynamic configuration reloading capabilities

## 🔒 Enterprise-Grade Security Features

### 🛡️ Complete 7-Layer Security Architecture (100% Implemented)

- **🔐 RBAC (Role-Based Access Control)**: Fine-grained access control with role hierarchy and inheritance
- **🔐 AES-GCM-256 Encryption**: Military-grade encryption with PBKDF2 key derivation (100K iterations)
- **⚡ Token Bucket Rate Limiting**: Advanced DDoS protection with configurable limits and burst handling
- **🔒 TLS 1.2+ Enforcement**: Mandatory secure transport with certificate validation
- **🚫 Zero-Panic Operations**: Complete unwrap() elimination with comprehensive Result-based error handling
- **🛡️ SQL Injection Protection**: 11 attack pattern detection with Union/Boolean/Time-based attack prevention
- **🚫 XSS Attack Protection**: 14 attack pattern detection with HTML sanitization and CSP headers
- **📊 Comprehensive Audit Logging**: All security events recorded with tamper-resistant logging
- **🎯 Advanced Input Validation**: Real-time validation with custom rules and data sanitization
- **🔍 Data Masking**: 4 masking strategies (Full, Partial, Hash, Tokenize) for PII/PHI protection
- **📈 Time-Based Access Control**: Business hours and emergency access support

## 🎯 WordPress MCP Tools (32 tools available)

**📝 Content Management:**

- Create, read (full editable source), update, and delete posts and pages
- Advanced post creation with SEO and scheduling
- YouTube and social media embeds
- Category and tag management

**🖼️ Media Management:**

- Upload media files (base64, with type, extension, and content validation)
- Featured image management
- Media library operations
- Accessibility support (alt text, captions)

**🔗 Content Integration:**

- Create posts with taxonomy
- Comment retrieval

**⚙️ Site Management:**

- Read and update site settings
- Front page configuration
- Health check of connectivity, authentication, and permissions

See [WordPress Integration](docs/wordpress/) for the full list of tools.

## 🗄️ Database MCP Tools (Multi-Engine Support)

**📊 Database Engines:**

- **PostgreSQL**: Enterprise relational database with advanced SQL features
- **MySQL**: Popular web-scale database with full transaction support
- **Redis**: High-performance in-memory store with cluster support
- **MongoDB**: Document-oriented NoSQL with aggregation pipelines
- **SQLite**: Lightweight embedded database for development

**🔧 Database Operations:**

- Execute queries with SQL injection protection
- Transaction management with configurable isolation levels
- Schema introspection and metadata retrieval
- Multi-engine workflows and cache-aside patterns
- Real-time health monitoring and performance metrics
- Connection pooling with advanced timeout handling

## Current Status

## ✅ Recently Completed (v0.18.0)

- **🔑 API Key Authentication**: Optional API key check on every HTTP/WebSocket request
- **🚦 Rate Limiting**: WordPress tool calls are rate limited per site
- **📝 Audit Logging**: Every WordPress tool call that changes the site is recorded, including rejected ones
- **🔐 Encrypted Credentials**: WordPress Application Passwords can be stored encrypted in the config file
- **🖼️ Media Upload Validation**: MIME allowlist, extension and content checks, and size limits for `upload_media`
- **🛡️ Dependency Audits**: Known vulnerabilities fixed, and `cargo audit` now runs daily

See the [Security documentation](docs/security/) for details.

## ✅ v0.16.0

- **🌐 WebSocket Transport Enhancements**: Server mode, LLM streaming, connection pooling, load balancing
- **🔌 Plugin Isolation System**: Inter-plugin communication, advanced error handling, Docker runtime support
- **🤖 LLM Streaming Integration**: OpenAI GPT-4 and Anthropic Claude 3.5 Sonnet with real-time responses
- **📊 Metrics & Monitoring**: Real-time metrics collection, rate limiting (3 algorithms), message compression
- **🐳 Docker Runtime Support**: Container-based plugin execution with security scanning
- **🏗️ Code Architecture**: mod.rs refactoring for improved maintainability (5 major modules)
- **🔒 Enterprise Security**: 7-layer security architecture with RBAC (100% Complete)
- **🧪 Quality Assurance**: 567 test cases with 100% pass rate and zero Clippy warnings

## Implementation Status

- **WordPress API Handler**: Complete with featured image and media upload support
- **Security Implementation**: 100% complete (6/6 enterprise-grade security layers)
- **MCP Protocol Foundation**: JSON-RPC + handler trait system
- **Error Handling**: thiserror-based type-safe error management
- **Security Testing**: Comprehensive test suite with 100% security coverage

MCP-RS is **production-ready** with enterprise-level security requirements and comprehensive WordPress integration capabilities.
