---
layout: home
title: MCP-RS Documentation
permalink: /
---

# MCP-RS Documentation

Welcome to the MCP-RS documentation! This site contains comprehensive guides, API references, and architecture documentation for the Rust implementation of the Model Context Protocol.

### Quick Access
- **[Getting Started](./docs/guides/)** - Implementation guides and tutorials
- **[API Reference](./docs/api/)** - Complete API documentation
- **[Architecture](./docs/architecture/)** - System design and technical architecture

### Documentation Hub
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

### 🔒 Enterprise-Grade Security Features

**🛡️ Complete 6-Layer Security Architecture (100% Implemented)**

- **🔐 AES-GCM-256 Encryption**: Military-grade encryption with PBKDF2 key derivation (100K iterations)
- **⚡ Token Bucket Rate Limiting**: Advanced DDoS protection with configurable limits and burst handling
- **🔒 TLS 1.2+ Enforcement**: Mandatory secure transport with certificate validation
- **🚫 Zero-Panic Operations**: Complete unwrap() elimination with comprehensive Result-based error handling
- **🛡️ SQL Injection Protection**: 11 attack pattern detection with Union/Boolean/Time-based attack prevention
- **🚫 XSS Attack Protection**: 14 attack pattern detection with HTML sanitization and CSP headers
- **📊 Comprehensive Audit Logging**: All security events recorded with tamper-resistant logging
- **🎯 Advanced Input Validation**: Real-time validation with custom rules and data sanitization
- **🔍 Zero-Trust Data Validation**: All inputs validated through multi-layer security checks
- **📈 Real-time Security Monitoring**: Threat level analysis with attack detection and prevention

### 🎯 WordPress MCP Tools (27 tools available)

**📝 Content Management:**
- Complete post and page management
- Advanced post creation with SEO and scheduling
- YouTube and social media embeds
- Category and tag management

**🖼️ Media Management:**
- Upload media files (base64/multipart)
- Featured image management
- Media library operations
- Accessibility support (alt text, captions)

**🔗 Content Integration:**
- Create posts with taxonomy
- Comments management
- Content relationships

### 🗄️ Database MCP Tools (Multi-Engine Support)

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

### ✅ Recently Completed (v0.2.0-alpha)
- **🏗️ Core Runtime Module**: Complete application lifecycle management with state tracking
- **� Stdio Transport Support**: Standard input/output communication for process-based integration
- **🔌 Transport Abstraction**: Pluggable transport layer with configurable framing methods
- **🔒 Enterprise Security**: 6-layer security architecture (100% Complete)
- **🧪 Quality Assurance**: 197+ test cases with 100% pass rate and zero Clippy warnings

### Implementation Status
- **WordPress API Handler**: Complete with featured image and media upload support
- **Security Implementation**: 100% complete (6/6 enterprise-grade security layers)
- **MCP Protocol Foundation**: JSON-RPC + handler trait system
- **Error Handling**: thiserror-based type-safe error management
- **Security Testing**: Comprehensive test suite with 100% security coverage

MCP-RS is **production-ready** with enterprise-level security requirements and comprehensive WordPress integration capabilities.
