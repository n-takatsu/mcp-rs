# MCP-RS Documentation

**Model Context Protocol implementation in Rust**

---

## Overview

MCP-RS is a robust, type-safe Rust implementation of the Model Context Protocol (MCP) designed for AI-agent integration. It provides a standardized JSON-RPC interface for AI agents to interact with various services and tools.

## Key Features

- 🚀 **High Performance**: Built on tokio for async operations
- 🔒 **Type Safety**: Leverages Rust's type system for reliable protocol handling
- 🔌 **Plugin Architecture**: Extensible handler system for custom integrations
- 🌐 **Multi-Transport**: Support for stdio, HTTP, and WebSocket protocols
- ⚙️ **Configuration-Driven**: TOML-based configuration with environment variable overrides

## Quick Navigation

### 📚 Documentation Sections

- **[Architecture](architecture/)** - System design and technical architecture
- **[API Reference](api/)** - Complete API documentation
- **[Guides](guides/)** - Implementation guides and tutorials

### 🚀 Getting Started

1. **Installation**
   ```toml
   [dependencies]
   mcp-rs = "0.1.0"
   ```

2. **Basic Usage**
   ```rust
   use mcp_rs::{
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
   # mcp-config.toml
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

### ✅ Completed Features
- WordPress API Handler with full REST API support
- Configuration management system (TOML + environment variables)
- MCP protocol implementation with JSON-RPC
- Error handling and logging infrastructure
- HTTP communication with timeout and retry logic

### 🔄 In Development
- Core application modules
- Transport abstraction layer
- Plugin system for dynamic loading
- Performance monitoring and metrics

### 📋 Planned Features
- WebSocket transport support
- stdio transport for CLI integration
- Advanced plugin ecosystem
- Comprehensive monitoring and observability

## Architecture Overview

```
┌─────────────────────────────────────────┐
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

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

**Last Updated**: November 3, 2025  
**Version**: v0.1.0 (Development)