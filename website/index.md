---
layout: home
---

# MCP-RS Documentation

Welcome to the MCP-RS documentation! This site contains comprehensive guides, API references, and architecture documentation for the Rust implementation of the Model Context Protocol.

## Quick Links

- **[Getting Started]({{ site.baseurl }}/docs/guides/)** - Implementation guides and tutorials
- **[API Reference]({{ site.baseurl }}/docs/api/)** - Complete API documentation  
- **[Architecture]({{ site.baseurl }}/docs/architecture/)** - System design and technical architecture
- **[Security]({{ site.baseurl }}/docs/security/)** - Enterprise-grade security features
- **[GitHub Repository](https://github.com/n-takatsu/mcp-rs)** - Source code and issues

## What is MCP-RS?

MCP-RS is a robust, type-safe Rust implementation of the Model Context Protocol (MCP) designed for AI-agent integration. It provides a standardized JSON-RPC interface for AI agents to interact with various services and tools.

## Key Features

- 🚀 **High Performance**: Built on tokio for async operations
- 🔒 **Type Safety**: Leverages Rust's type system for reliable protocol handling  
- 🔌 **Plugin Architecture**: Extensible handler system for custom integrations
- 🌐 **Multi-Transport**: Support for stdio, HTTP, and WebSocket protocols
- ⚙️ **Configuration-Driven**: TOML-based configuration with secure environment variable expansion
- 🛡️ **Enterprise Security**: 5-layer security architecture with 171 comprehensive test cases
  - AES-GCM-256 encryption with PBKDF2 key derivation
  - Token bucket rate limiting with client isolation
  - TLS 1.2+ enforcement and certificate validation
  - SQL injection protection (11 attack patterns)
  - Zero-panic operations and memory safety
- 🖼️ **WordPress Integration**: Complete WordPress REST API support with media management
- 📝 **Content Management**: Full CRUD operations for posts, pages, and comments
- 🎯 **Featured Image Support**: Upload and manage WordPress featured images
- 🔍 **Health Monitoring**: Comprehensive environment validation and diagnostics
- 🛠️ **Production Ready**: Built-in error handling, retry logic, and performance monitoring

## Current Status

MCP-RS is currently in active development with **86% security implementation complete** (12/14 enterprise-grade security features). The project features a comprehensive 5-layer security architecture and is suitable for production environments with enterprise-level security requirements.