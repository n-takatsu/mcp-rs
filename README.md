# mcp-rs

🚀 Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

[![Architecture](https://img.shields.io/badge/Architecture-v0.1.0--alpha-blue)](#architecture)
[![Implementation](https://img.shields.io/badge/Implementation-v0.1.0--dev-orange)](#implementation-status)
[![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)](#license)

## Overview

`mcp-rs` provides a robust, extensible, plugin-oriented implementation of the MCP (Model Context Protocol) in Rust. Built with a layered architecture, it enables AI agents to interact with various services through a standardized JSON-RPC interface. The framework features comprehensive WordPress integration, strong type safety, and is optimized for use with GitHub Copilot and other AI coding assistants.

### Key Highlights

- **🏗️ Layered Architecture**: Clean separation with Application, API, Service, Core, and Infrastructure layers
- **🔌 Plugin-Oriented Design**: Extensible handler system with dynamic configuration
- **⚡ Production Ready**: Timeout handling, retry logic, comprehensive error handling
- **🛡️ Type-Safe**: Full Rust type safety with serde-based configuration
- **🔄 Async-First**: Built on tokio for high-performance concurrent operations

## Features

### 🚀 Core Capabilities

- **JSON-RPC 2.0 Server**: Full-featured JSON-RPC server implementation using `axum`
- **Plugin Architecture**: Handler-based system with `McpHandler` trait for extensibility
- **Type-Safe Configuration**: TOML-based configuration with environment variable override
- **Production-Ready Error Handling**: Comprehensive error types with structured logging
- **Async/Await**: Built on `tokio` for high-performance async operations
- **Hot Configuration**: Dynamic configuration reloading capabilities

### 🛠️ Protocol Support

- **Tools**: Define and execute tools with JSON schema validation
- **Resources**: Expose and read resources with URI-based access
- **Prompts**: Create and retrieve prompts with argument support
- **Error Handling**: Comprehensive error types with JSON-RPC error codes
- **Transport Abstraction**: Pluggable transport layer (stdio, HTTP, WebSocket planned)

### 🔌 Current Integrations

- **✅ WordPress**: Full WordPress REST API integration with advanced features:
  - Post/Page management (CRUD operations)
  - **🖼️ Featured Image Support**: Upload and set featured images
  - **📁 Media Library**: Complete media upload and management
  - **🎯 Base64 Upload**: Handle base64-encoded file uploads
  - Comment management and retrieval
  - Tag and category operations
  - Timeout and retry handling with exponential backoff
  - Secure authentication with application passwords
  - **📋 Multipart Uploads**: Efficient file upload handling

### 🔄 Planned Integrations

- **GitHub**: Repository and issue management
- **Custom APIs**: Generic REST API handler template
- **File System**: Local file operations
- **Database**: SQL database integration

## Implementation Status

### ✅ Completed (v0.1.0-alpha)
- **WordPress API Handler**: Complete with featured image and media upload support
- **Media Management**: Base64 upload, multipart form handling, featured image setting
- **Configuration Management**: TOML + environment variable hierarchy
- **MCP Protocol Foundation**: JSON-RPC + handler trait system
- **Error Handling**: thiserror-based type-safe error management
- **Structured Logging**: tracing-based logging system
- **HTTP Communication**: reqwest + timeout + exponential backoff retry

### 🔄 In Development
- Core runtime module (application lifecycle)
- Transport abstraction layer
- Plugin dynamic loading system
- Performance monitoring and metrics

### 🎯 Roadmap
- **v0.2.0**: Core module implementation, stdio transport
- **v0.3.0**: WebSocket transport, metrics, performance optimization
- **v1.0.0**: Production readiness, security audit, ecosystem

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mcp-rs = "0.1.0"
```

## Quick Start

### Configuration Setup

Create a `mcp-config.toml` file:

```toml
[server]
host = "0.0.0.0"
port = 3000

[wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"
timeout_seconds = 30
```

### Running the Server

```bash
# Build the project
cargo build

# Run with default configuration
cargo run

# Run with specific config file
cargo run -- --config custom-config.toml

# Run with environment override
WORDPRESS_URL=https://mysite.com cargo run
```
### Creating a Custom Handler

```rust
use async_trait::async_trait;
use mcp_rs::{
    mcp::{McpHandler, types::*}, 
    error::{McpError, Result}
};
use serde_json::{json, Value};

pub struct CustomHandler {
    // Your handler state
}

#[async_trait]
impl McpHandler for CustomHandler {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        Ok(vec![
            Tool {
                name: "custom_tool".to_string(),
                description: "My custom tool".to_string(),
                input_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string" }
                    }
                })),
            }
        ])
    }

    async fn call_tool(&self, name: &str, args: Option<Value>) -> Result<Value> {
        match name {
            "custom_tool" => {
                // Implement your tool logic
                Ok(json!({"result": "success"}))
            }
            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }

    // Implement other trait methods as needed...
}
```

### Integrating with MCP Server

```rust
use mcp_rs::{server::McpServer, config::Config};

#[tokio::main]
async fn main() {
    // Load configuration
    let config = Config::load("mcp-config.toml").unwrap();
    
    // Create server with handlers
    let mut server = McpServer::new();
    
    // Add your custom handler
    server.add_handler("custom", Box::new(CustomHandler::new()));
    
    // Start server
    server.serve(config.server.address()).await.unwrap();
}
```
```

## Examples

Run the examples:

```bash
# WordPress integration timeout test
cargo run --example http_timeout_test

# WordPress API connection test
cargo run --example wordpress_test

# Security diagnosis for WordPress
cargo run --example wordpress_security_diagnosis

# Timeout handling test (v2)
cargo run --example timeout_test_v2
```

## MCP Usage Examples

Once the server is running, AI agents can interact with WordPress through natural language:

### 📝 **Content Creation**
**User:** "Create a blog post about Rust programming"
**AI automatically:**
1. Uses `create_post` tool
2. Generates appropriate title and content
3. Returns post URL and ID

### 🖼️ **Featured Image Workflow**
**User:** "Upload this image and create a post with it as featured image"
**AI automatically:**
1. Uses `upload_media` tool with base64 image data
2. Uses `create_post_with_featured_image` with returned media ID
3. Creates post with proper featured image

### 🔄 **Content Management**
**User:** "Show me recent posts and their comments"
**AI automatically:**
1. Uses `get_posts` to retrieve recent posts
2. Uses `get_comments` for each post
3. Presents organized summary

### 🎯 **Advanced Workflows**
**User:** "Add a featured image to post #123"
**AI automatically:**
1. Uploads provided image using `upload_media`
2. Uses `set_featured_image` to update existing post
3. Confirms successful update

## Architecture

### Layered Architecture

```
┌─────────────────────────────────────────────────────┐
│ Application Layer                                   │
│ ├── main.rs (Entry point)                          │
│ └── CLI/Server startup                              │
├─────────────────────────────────────────────────────┤
│ API Layer                                           │
│ ├── mcp/ (Model Context Protocol)                  │
│ ├── protocol.rs (Protocol definitions)             │
│ └── JSON-RPC interface                             │
├─────────────────────────────────────────────────────┤
│ Service Layer                                       │
│ ├── handlers/ (Feature implementations)            │
│ │   └── wordpress.rs (✅ Implemented)             │
│ └── plugins/ (Dynamic plugin system) [🔄 Planned] │
├─────────────────────────────────────────────────────┤
│ Core Layer [🔄 Planned]                            │
│ ├── runtime.rs (Async runtime management)          │
│ ├── registry.rs (Handler/plugin registry)          │
│ └── context.rs (Execution context)                 │
├─────────────────────────────────────────────────────┤
│ Infrastructure Layer                                │
│ ├── transport/ (Communication abstraction) [🔄]    │
│ ├── config/ (Configuration management) ✅          │
│ └── error.rs (Error handling) ✅                   │
└─────────────────────────────────────────────────────┘
```

### Design Principles

1. **Plugin-Oriented Architecture**: Unified interface via `McpHandler` trait
2. **Transport Abstraction**: Multi-protocol support (stdio, HTTP, WebSocket)
3. **Configuration-Driven**: TOML-based feature control and settings
4. **Async-First**: tokio-based high-performance communication
5. **Type Safety**: Strong typing with serde-based configuration

## Project Structure

```
mcp-rs/
├── src/
│   ├── lib.rs              # Library entry point
│   ├── main.rs             # Application entry point
│   ├── error.rs            # Error types and handling ✅
│   ├── types.rs            # MCP type definitions ✅
│   ├── protocol.rs         # Protocol implementations ✅
│   ├── server.rs           # JSON-RPC server ✅
│   ├── config.rs           # Configuration management ✅
│   ├── handlers/           # Service layer implementations
│   │   ├── mod.rs          # Handler module organization
│   │   └── wordpress.rs    # WordPress API handler ✅
│   └── mcp/               # MCP protocol core
│       ├── mod.rs         # MCP module organization
│       ├── server.rs      # MCP server implementation ✅
│       ├── types.rs       # MCP type definitions ✅
│       └── error.rs       # MCP-specific errors ✅
├── examples/              # Example implementations
│   ├── http_timeout_test.rs        # Timeout handling test
│   ├── wordpress_test.rs           # WordPress integration
│   ├── wordpress_security_diagnosis.rs  # Security features
│   └── timeout_test_v2.rs          # Advanced timeout test
├── docs/                  # Documentation
│   └── architecture.md    # Architecture design document
├── mcp-config.toml.example # Configuration template
└── Cargo.toml            # Dependencies and metadata
```

### JSON-RPC Communication

All communication uses JSON-RPC 2.0 format:

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "wordpress_create_post",
    "arguments": {
      "title": "Hello World",
      "content": "This is my first post!"
    }
  },
  "id": 1
}
```

### WordPress Handler Features

Current WordPress integration includes:

- **Post Management**: Create, read, update, delete posts and pages
- **Media Handling**: Upload and manage media files
- **Comment System**: Full comment management capabilities
- **Taxonomy Support**: Categories and tags operations
- **Authentication**: Application password authentication
- **Error Handling**: Comprehensive retry and timeout logic
- **Logging**: Detailed operation logging with tracing

## GitHub Copilot Integration

This codebase is optimized for AI-assisted development:

### 1. Natural Language to Code
- **Architecture-Aware**: "Add a GitHub handler similar to WordPress" → generates structured handler
- **Type-Safe Suggestions**: Leverages Rust's type system for accurate completions
- **Configuration-Driven**: "Add timeout settings" → suggests appropriate config structure

### 2. Pattern Recognition
- **Handler Pattern**: Consistent `McpHandler` trait implementations
- **Error Handling**: Unified error patterns with `McpError` and `Result<T>`
- **Async Patterns**: Proper async/await usage with tokio

### 3. Project Context Understanding
- **Layered Architecture**: Suggestions respect architectural boundaries
- **Configuration Management**: TOML-first configuration patterns
- **Logging Integration**: Structured logging with tracing crate

## Development

### Building

```bash
# Build for development
cargo build

# Build for release
cargo build --release

# Check code without building
cargo check
```

### Code Quality

```bash
# Run tests
cargo test

# Check formatting
cargo fmt --check

# Run clippy lints
cargo clippy

# Check for security vulnerabilities
cargo audit
```

### Configuration

Copy the example configuration:

```bash
cp mcp-config.toml.example mcp-config.toml
# Edit mcp-config.toml with your settings
```

## Use Cases

### AI Agent Integration
- **WordPress Automation**: Content creation and management
- **Multi-Platform Publishing**: Cross-platform content distribution
- **API Orchestration**: Complex multi-step workflows

### Development Workflows
- **GitHub Integration**: Repository and issue management (planned)
- **CI/CD Automation**: Build and deployment integration
- **Documentation Generation**: Automated docs from code

### Enterprise Applications
- **Content Management**: Large-scale WordPress operations
- **Security Monitoring**: Automated security checks and reporting
- **Performance Analytics**: System monitoring and optimization

## Contributing

We welcome contributions! Please see our contribution guidelines:

### Development Process

1. **Fork and Clone**: Fork the repository and clone locally
2. **Create Branch**: Create a feature branch for your changes
3. **Write Tests**: Add tests for new functionality
4. **Follow Standards**: Use `cargo fmt` and `cargo clippy`
5. **Update Docs**: Update documentation and README as needed
6. **Submit PR**: Create a pull request with detailed description

### Architecture Compliance

Please ensure contributions align with our layered architecture:
- **Handlers** go in `src/handlers/`
- **Core functionality** follows the plugin pattern
- **Configuration** uses TOML with environment override
- **Errors** use the `McpError` type system

## Roadmap

See [docs/architecture.md](docs/architecture.md) for detailed roadmap.

### Near Term (v0.2.0)
- [ ] Core runtime module implementation
- [ ] Stdio transport support
- [ ] Plugin dynamic loading system
- [ ] Enhanced configuration management

### Medium Term (v0.3.0)
- [ ] WebSocket transport support
- [ ] Performance monitoring and metrics
- [ ] GitHub API handler
- [ ] Database integration handler

### Long Term (v1.0.0)
- [ ] Production security audit
- [ ] Comprehensive test suite
- [ ] Documentation website
- [ ] Plugin ecosystem

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/) for performance and safety
- Uses [tokio](https://tokio.rs/) for async runtime
- JSON-RPC server powered by [axum](https://github.com/tokio-rs/axum)
- HTTP client based on [reqwest](https://github.com/seanmonstar/reqwest)
- Configuration management with [config](https://github.com/mehcode/config-rs)

---

**🚀 Ready to integrate AI agents with your services? Get started with mcp-rs today!**

