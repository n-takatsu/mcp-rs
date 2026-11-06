---
layout: page
title: Documentation
permalink: /docs/
---

# MCP-RS Documentation

**Model Context Protocol implementation in Rust - Production Ready**

---

## Overview

🚀 **Production-Ready** Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

MCP-RS provides a **comprehensive, battle-tested** implementation of the MCP (Model Context Protocol) in Rust with **complete WordPress integration**. Built with a layered architecture, it enables AI agents to perform sophisticated content management through a standardized JSON-RPC interface.

## Key Features

## 🚀 Core Capabilities

- **JSON-RPC 2.0 Server**: Full-featured JSON-RPC server implementation using `axum`
- **Core Runtime Module**: Advanced application lifecycle and resource management
- **Multi-Transport Support**: Stdio, HTTP, and WebSocket communication protocols
- **WebSocket Enhancements** (v0.16.0):
  - Server mode with connection management and health checks
  - LLM streaming (OpenAI GPT-4, Anthropic Claude 3.5 Sonnet)
  - Connection pooling with 3 load balancing algorithms (RoundRobin, LeastConnections, Random)
  - Real-time metrics, rate limiting (TokenBucket, LeakyBucket, SlidingWindow), and compression (gzip/deflate)
- **Plugin Isolation System** (v0.16.0):
  - Docker runtime support for container-based plugin execution
  - Inter-plugin communication with Pub/Sub messaging
  - Advanced error handling with circuit breakers and automatic retry
  - Enhanced monitoring with resource tracking and performance metrics
- **Type-Safe Configuration**: TOML-based configuration with environment variable override
- **Production-Ready Error Handling**: Comprehensive error types with structured logging
- **Async/Await**: Built on `tokio` for high-performance async operations

## 🔒 Enterprise-Grade Security (100% Complete - 7 Layers)

- **🔐 RBAC (Role-Based Access Control)**: Fine-grained access control with role hierarchy, column-level permissions, and data masking
- **🔐 AES-GCM-256 Encryption**: Military-grade encryption with PBKDF2 key derivation (100K iterations)
- **⚡ Token Bucket Rate Limiting**: Advanced DDoS protection with configurable limits
- **🔒 TLS 1.2+ Enforcement**: Mandatory secure transport with certificate validation
- **🛡️ SQL Injection Protection**: 11 attack pattern detection with parameterized queries
- **🚫 XSS Attack Protection**: 14 attack pattern detection with HTML sanitization
- **📊 Comprehensive Audit Logging**: All security events recorded with tamper-resistant logging

## 🗄️ Database Integration (Production Ready)

- **Multi-Engine Support**: PostgreSQL, MySQL, Redis, MongoDB, SQLite
- **🔄 Dynamic Engine Switching**: Zero-downtime switching with intelligent failover
- **Enterprise Security**: 6-layer security architecture for all database operations
- **Advanced Features**: Connection pooling, transaction management, schema introspection
- **Performance Monitoring**: Real-time health checks and query optimization
- **Multi-Engine Workflows**: Cache-aside patterns and hybrid data architectures

## 🎯 WordPress Integration (27 Tools)

- **📝 Content Management**: Complete post and page management with SEO support
- **🖼️ Media Management**: Upload, manage, and set featured images
- **🏷️ Taxonomy Management**: Categories and tags with hierarchical support
- **🎬 Rich Content**: YouTube embeds and social media integration

## Quick Navigation

## 📚 Documentation Sections

- **[Architecture]({{ site.baseurl }}/docs/architecture/)** - System design and technical architecture
- **[API Reference]({{ site.baseurl }}/docs/api/)** - Complete API documentation
- **[Security]({{ site.baseurl }}/docs/security/)** - Enterprise-grade security features
- **[Database Guide]({{ site.baseurl }}/docs/database/)** - Multi-engine database integration
- **[Guides]({{ site.baseurl }}/docs/guides/)** - Implementation guides and tutorials
- **[WordPress Integration]({{ site.baseurl }}/docs/wordpress/)** - WordPress REST API integration

## 🚀 Getting Started

1. **Installation**

   ```toml

   mcp-rs = "0.1.0"

   ```

2. **Basic Usage**

   ```rust

       server::McpServer,
       config::Config,
       handlers::wordpress::WordPressHandler
   };

   #[tokio::main]
   async fn main() -> Result<(), Box<dyn std::error::Error>> {
       // Load configuration
       let config = Config::load("mcp-config.toml")?;

       // Create server
       let mut server = McpServer::new();

       // Add WordPress handler if configured
       if let Some(wp_config) = config.wordpress {
           let handler = WordPressHandler::new(wp_config);
           server.add_handler("wordpress", Box::new(handler));
       }

       // Start server
       server.serve(config.server.address()).await?;
       Ok(())
   }

   ```

3. **Configuration**

   ```toml


## mcp-config.toml

   [server]
   host = "127.0.0.1"
   port = 8080

   [wordpress]
   url = "https://your-wordpress-site.com"
   username = "your_username"
   password = "your_application_password"
   timeout_seconds = 30

   ```

## Current Implementation Status

## ✅ Completed Features

- WordPress API Handler with full REST API support
- Configuration management system (TOML + environment variables)
- MCP protocol implementation with JSON-RPC
- Error handling and logging infrastructure
- HTTP communication with timeout and retry logic

## 🔄 In Development

- Core application modules
- Transport abstraction layer
- Plugin system for dynamic loading
- Performance monitoring and metrics

## 📋 Planned Features

- WebSocket transport support
- stdio transport for CLI integration
- Advanced plugin ecosystem
- Comprehensive monitoring and observability

## Architecture Overview

```

│ Application Layer                       │
│ ├── main.rs (Entry Point)              │
│ └── CLI/Server Startup                 │
├─────────────────────────────────────────┤
│ API Layer                               │
│ ├── mcp/ (Protocol Implementation)     │
│ ├── protocol.rs                        │
│ └── JSON-RPC Interface                 │
├─────────────────────────────────────────┤
│ Service Layer                           │
│ ├── handlers/ (Feature Implementations)│
│ └── plugins/ (Dynamic Plugin System)   │
├─────────────────────────────────────────┤
│ Core Layer                              │
│ ├── Runtime Management                 │
│ ├── Registry & Context                 │
│ └── Event System                       │
├─────────────────────────────────────────┤
│ Infrastructure Layer                    │
│ ├── transport/ (Communication)         │
│ ├── config/ (Configuration)            │
│ └── error/ (Error Handling)            │
└─────────────────────────────────────────┘

```

## Use Cases

- **WordPress Management**: Automate content creation and site management
- **API Integration**: Connect AI agents to REST APIs and web services
- **Tool Orchestration**: Create composable tools for complex workflows
- **Resource Management**: Expose file systems, databases, and other resources
- **AI Agent Integration**: Standardized interface for AI-driven automation

## Contributing

We welcome contributions! Please see our [contributing guidelines](https://github.com/n-takatsu/mcp-rs/blob/main/CONTRIBUTING.md) for details.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/n-takatsu/mcp-rs/blob/main/LICENSE-APACHE))
- MIT license ([LICENSE-MIT](https://github.com/n-takatsu/mcp-rs/blob/main/LICENSE-MIT))

at your option.

---

**Last Updated**: November 3, 2025
**Version**: v0.18.0
