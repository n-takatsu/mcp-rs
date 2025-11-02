# mcp-rs

Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

## Overview

`mcp-rs` provides a robust, type-safe implementation of the MCP (Model Context Protocol) in Rust. It enables AI agents to interact with various services through a standardized JSON-RPC interface. The library features strong completion support and structural suggestions, making it ideal for use with GitHub Copilot and other AI coding assistants.

## Features

### 🚀 Core Capabilities

- **JSON-RPC 2.0 Server**: Full-featured JSON-RPC server implementation using `axum`
- **Type-Safe Protocol**: Strongly-typed message definitions using `serde_json`
- **Trait-Based Abstraction**: MCP protocol abstracted through the `McpProtocol` trait for easy customization
- **Async/Await**: Built on `tokio` for high-performance async operations
- **Client Library**: Ready-to-use client for connecting to MCP servers

### 🛠️ Protocol Support

- **Tools**: Define and execute tools with JSON schema validation
- **Resources**: Expose and read resources with URI-based access
- **Prompts**: Create and retrieve prompts with argument support
- **Error Handling**: Comprehensive error types with JSON-RPC error codes

### 🔌 Integration Examples

- **WordPress**: Full WordPress REST API integration example
- **Basic Server**: Simple example showing core functionality

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

## Examples

Run the examples:

```bash
# Basic server with echo and add tools
cargo run --example basic_server

# WordPress integration server
WORDPRESS_URL=http://your-wordpress-site.com cargo run --example wordpress_server
```

## Project Structure

```
mcp-rs/
├── src/
│   ├── lib.rs          # Library entry point
│   ├── error.rs        # Error types and handling
│   ├── types.rs        # JSON-RPC and MCP type definitions
│   ├── protocol.rs     # McpProtocol trait and implementations
│   ├── server.rs       # Axum-based JSON-RPC server
│   └── client.rs       # MCP client implementation
├── examples/
│   ├── basic_server.rs     # Simple example server
│   └── wordpress_server.rs # WordPress integration example
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
- **API Integration**: Connect AI agents to REST APIs
- **Tool Orchestration**: Create composable tools for complex workflows
- **Resource Management**: Expose file systems, databases, or other resources
- **Prompt Templates**: Define reusable prompt templates for AI interactions

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

