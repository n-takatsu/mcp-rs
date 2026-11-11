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
- **Request Dispatch**: `McpServer` lists tools from every handler and routes each call to the handler that provides it, in both STDIO and HTTP modes
- **Multi-Transport Support**: Stdio, HTTP, and WebSocket communication protocols
- **WebSocket**: In HTTP mode, MCP requests are also accepted on `/ws` (`enable_websocket_upgrade = true`)
- **Plugins**: Add tools from other MCP servers, run as separate processes. mcp-rs relays their tools with rate limiting and audit logging. See [Plugins]({{ site.baseurl }}/docs/plugins/)
- **Type-Safe Configuration**: TOML-based configuration with environment variable override
- **Production-Ready Error Handling**: Comprehensive error types with structured logging
- **Async/Await**: Built on `tokio` for high-performance async operations

## 🔒 Enterprise-Grade Security (100% Complete - 7 Layers)

- **🔐 RBAC (Role-Based Access Control)**: Fine-grained access control with role hierarchy, column-level permissions, and data masking
- **🔐 AES-GCM-256 Encryption**: Military-grade encryption with PBKDF2 key derivation (100K iterations)
- **⚡ Token Bucket Rate Limiting**: Advanced DDoS protection with configurable limits
- **🔒 TLS 1.2+ Enforcement**: Mandatory secure transport with certificate validation
- **📊 Comprehensive Audit Logging**: All security events recorded with tamper-resistant logging

## 🗄️ Database Integration

- **PostgreSQL**: Connect one database with a `[database]` section in the configuration file
- **Read-Only by Default**: `execute_query` and `get_schema`; queries run in a read-only transaction that is rolled back
- **Writes Only When Allowed**: `execute_command` (one INSERT, UPDATE, or DELETE) only with `allow_write = true`
- **Typed Parameters**: `$1`, `$2`, … are bound, never inserted into the SQL text
- **Limits**: Rows per result, time per statement, and connections

See the [Database Integration Guide](./database.md).

## 🎯 WordPress Integration (32 Tools)

- **📝 Content Management**: Complete post and page management with SEO support
- **🖼️ Media Management**: Upload, manage, and set featured images
- **🏷️ Taxonomy Management**: Categories and tags with hierarchical support
- **🎬 Rich Content**: YouTube embeds and social media integration
- **⚙️ Site Management**: Site settings and front page configuration

## Quick Navigation

## 📚 Documentation Sections

- **[Architecture]({{ site.baseurl }}/docs/architecture/)** - System design and technical architecture
- **[API Reference]({{ site.baseurl }}/docs/api/)** - Complete API documentation
- **[Security]({{ site.baseurl }}/docs/security/)** - Enterprise-grade security features
- **[Database Guide]({{ site.baseurl }}/docs/database/)** - Multi-engine database integration
- **[Guides]({{ site.baseurl }}/docs/guides/)** - Implementation guides and tutorials
- **[WordPress Integration]({{ site.baseurl }}/docs/wordpress/)** - WordPress REST API integration
- **[Plugins]({{ site.baseurl }}/docs/plugins/)** - Add tools from other MCP servers

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
│ └── handlers/ (Feature Implementations)│
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
**Version**: v0.22.0
