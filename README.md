# mcp-rs

Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

## Overview

`mcp-rs` provides a robust, type-safe implementation of the MCP (Model Context Protocol) in Rust. It enables AI agents to interact with various services through a standardized JSON-RPC interface. The library features strong completion support and structural suggestions, making it ideal for use with GitHub Copilot and other AI coding assistants.

**Current Status**: Development Phase - WordPress integration and core infrastructure complete.

## Features

### 🚀 Core Capabilities

- **JSON-RPC 2.0 Server**: Full-featured JSON-RPC server implementation using `axum`
- **Type-Safe Protocol**: Strongly-typed message definitions using `serde_json`
- **Trait-Based Abstraction**: MCP protocol abstracted through the `McpProtocol` trait for easy customization
- **Async/Await**: Built on `tokio` for high-performance async operations
- **Configuration Management**: TOML-based configuration with environment variable overrides
- **WordPress Integration**: Complete WordPress REST API integration with authentication

### 🛠️ Protocol Support

- **Tools**: Define and execute tools with JSON schema validation
- **Resources**: Expose and read resources with URI-based access
- **Prompts**: Create and retrieve prompts with argument support
- **Error Handling**: Comprehensive error types with JSON-RPC error codes

### 🔌 Current Integrations

- **WordPress**: Full WordPress REST API integration with Application Password authentication
- **Configuration**: TOML-based configuration with hierarchical overrides

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mcp-rs = "0.1.0"
```

## Quick Start

### Creating a Basic Server

```rust
use mcp_rs::{McpServer, protocol::BasicMcpProtocol, types::Tool};
use serde_json::json;

#[tokio::main]
async fn main() {
    // Create protocol with tools
    let mut protocol = BasicMcpProtocol::new("my-server", "1.0.0");
    
    protocol.add_tool(Tool {
        name: "echo".to_string(),
        description: "Echo a message".to_string(),
        input_schema: Some(json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            }
        })),
    });

    // Start server
    let server = McpServer::new(protocol);
    server.serve(([0, 0, 0, 0], 3000)).await.unwrap();
}
```

### Configuration Setup

Create `mcp-config.toml`:

```toml
[server]
bind_addr = "127.0.0.1:8080"
stdio = false
log_level = "info"

[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your-username"
password = "your-app-password"
enabled = true
```

**Note**: The configuration file is excluded from git for security. Use environment variables for production deployment.

### Implementing Custom Protocol

```rust
use async_trait::async_trait;
use mcp_rs::{protocol::McpProtocol, types::*, Error, Result};
use serde_json::{json, Value};

struct MyProtocol;

#[async_trait]
impl McpProtocol for MyProtocol {
    async fn initialize(&self) -> Result<(ServerInfo, ServerCapabilities)> {
        Ok((
            ServerInfo {
                name: "my-server".to_string(),
                version: "1.0.0".to_string(),
            },
            ServerCapabilities::default(),
        ))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>> {
        Ok(vec![/* your tools */])
    }

    async fn call_tool(&self, name: &str, args: Option<Value>) -> Result<Value> {
        // Implement tool execution
        match name {
            "my_tool" => Ok(json!({"result": "success"})),
            _ => Err(Error::MethodNotFound(name.to_string())),
        }
    }

    // Implement other trait methods...
}
```

### Using the Client

```rust
use mcp_rs::client::McpClient;
use serde_json::json;

#[tokio::main]
async fn main() {
    let client = McpClient::new("http://localhost:3000");
    
    // Initialize connection
    client.initialize().await.unwrap();
    
    // List available tools
    let tools = client.list_tools().await.unwrap();
    
    // Call a tool
    let result = client.call_tool(
        "echo",
        Some(json!({"message": "Hello, MCP!"}))
    ).await.unwrap();
}
```

## WordPress Integration

### Setup WordPress Application Password

1. WordPress Admin → Users → Profile
2. Scroll to "Application Passwords" section
3. Add new application name (e.g., "MCP-RS")
4. Copy the generated password to your configuration

### Example WordPress Operations

```bash
# Get recent posts
curl -X POST http://localhost:8080/jsonrpc \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "wordpress_get_posts",
      "arguments": {"per_page": 5}
    },
    "id": 1
  }'

# Create a new post
curl -X POST http://localhost:8080/jsonrpc \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "wordpress_create_post",
      "arguments": {
        "title": "My New Post",
        "content": "<p>Post content here</p>",
        "status": "draft"
      }
    },
    "id": 2
  }'
```

## Examples

Run the examples:

```bash
# Basic server with echo and add tools
cargo run --example basic_server

# WordPress integration server (requires configuration)
cargo run --bin mcp-rs
```

### Configuration for Examples

Copy `mcp-config.toml.example` to `mcp-config.toml` and update with your settings:

```bash
cp mcp-config.toml.example mcp-config.toml
# Edit mcp-config.toml with your WordPress credentials
```

## Project Structure

```
mcp-rs/
├── src/
│   ├── lib.rs          # Library entry point
│   ├── main.rs         # Binary entry point
│   ├── error.rs        # Error types and handling
│   ├── types.rs        # JSON-RPC and MCP type definitions
│   ├── protocol.rs     # McpProtocol trait and implementations
│   ├── server.rs       # Axum-based JSON-RPC server
│   ├── config.rs       # Configuration management
│   ├── handlers/       # Handler implementations
│   │   ├── mod.rs      # Handler module exports
│   │   └── wordpress.rs # WordPress API handler
│   ├── mcp/            # MCP protocol implementation
│   └── [core/transport/plugins/] # Future modules
├── examples/
│   ├── basic_server.rs     # Simple example server
│   ├── timeout_test.rs     # Timeout handling example
│   └── wordpress_*.rs      # WordPress integration examples
├── docs/
│   └── architecture.md     # Architecture documentation
├── website/docs/           # GitHub Pages documentation
│   ├── index.md           # Main documentation
│   ├── api/               # API reference
│   ├── architecture/      # Architecture docs
│   └── guides/            # Implementation guides
├── mcp-config.toml.example # Configuration template
└── Cargo.toml
```

## Architecture

### JSON-RPC Messages

All communication uses JSON-RPC 2.0 format:

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "echo",
    "arguments": {"message": "Hello"}
  },
  "id": 1
}
```

### Protocol Methods

- `initialize` - Server handshake and capability negotiation
- `tools/list` - List available tools
- `tools/call` - Execute a tool
- `resources/list` - List available resources
- `resources/read` - Read a resource
- `prompts/list` - List available prompts
- `prompts/get` - Get a prompt

## GitHub Copilot Integration

This codebase is designed to work seamlessly with GitHub Copilot:

### 1. Strong Code Completion
- Natural language: "Create a JSON-RPC server for WordPress posts"
- Copilot understands the project structure and suggests appropriate types and methods
- Leverages `serde_json`, `axum`, and `reqwest` patterns

### 2. Project-Wide Context
- Copilot references existing code structure for consistent implementations
- Understands the `McpProtocol` trait and provides conforming implementations
- Suggests error handling patterns matching the project style

### 3. Natural Language Interaction
- "Add a method for WordPress commenting API" → generates full implementation
- "Add error handling for network timeouts" → suggests appropriate error types
- "Create a tool for file uploads" → provides schema and implementation

## Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Running Examples

```bash
cargo run --example basic_server
```

## Use Cases

- **WordPress Management**: Automate WordPress content creation and management
- **API Integration**: Connect AI agents to REST APIs and web services
- **Tool Orchestration**: Create composable tools for complex workflows
- **Resource Management**: Expose file systems, databases, or other resources
- **AI Agent Integration**: Standardized interface for AI-driven automation

## Documentation

- **[Complete Documentation](https://n-takatsu.github.io/mcp-rs/)** - Full documentation with guides and API reference
- **[Architecture Guide](docs/architecture.md)** - Detailed system architecture and design decisions
- **[API Reference](https://n-takatsu.github.io/mcp-rs/api/)** - Complete API documentation
- **[Implementation Guides](https://n-takatsu.github.io/mcp-rs/guides/)** - Step-by-step implementation tutorials

## Current Implementation Status

### ✅ Completed
- WordPress REST API integration with full CRUD operations
- Configuration management (TOML + environment variables)
- MCP protocol implementation (JSON-RPC 2.0)
- Error handling and structured logging
- Security features (Application Password authentication)
- Documentation and guides

### 🔄 In Development
- Core application modules
- Transport abstraction layer
- Plugin system for dynamic loading

### 📋 Planned
- stdio transport for CLI integration
- WebSocket transport for real-time communication
- Additional service integrations (GitHub, file system, databases)
- Performance monitoring and metrics

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

