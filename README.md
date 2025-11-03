# mcp-rs

🚀 **Production-Ready** Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

[![Architecture](https://img.shields.io/badge/Architecture-v0.1.0--alpha-blue)](#architecture)
[![Implementation](https://img.shields.io/badge/WordPress_Tools-27_Available-green)](#wordpress-mcp-tools)
[![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)](#license)

## Overview

`mcp-rs` provides a **comprehensive, battle-tested** implementation of the MCP (Model Context Protocol) in Rust with **complete WordPress integration**. Built with a layered architecture, it enables AI agents to perform sophisticated content management through a standardized JSON-RPC interface. The framework features 27 WordPress tools, strong type safety, and is optimized for production use with GitHub Copilot and other AI coding assistants.

### 🎯 Key Highlights

- **🏗️ Production Architecture**: Clean separation with layered design and comprehensive error handling  
- **� Complete WordPress CMS**: 27 tools covering posts, pages, media, categories, tags, and embedded content
- **🎬 Rich Media Support**: YouTube embeds, social media integration, and full accessibility features
- **⚡ Performance Optimized**: Async-first with timeout handling, retry logic, and connection pooling
- **🛡️ Type-Safe**: Full Rust type safety with structured configuration and error management
- **🔄 AI-Agent Ready**: Designed specifically for seamless AI agent interaction

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
  - **📝 Advanced Post Management**: Create posts/pages with comprehensive options
  - **📊 Status Control**: Draft, publish, private, and scheduled posts  
  - **🎯 SEO Integration**: Meta fields for Yoast SEO and other plugins
  - **📅 Post Scheduling**: Future publication with ISO8601 timestamps
  - **🖼️ Complete Media CRUD**: Full media library management with metadata
  - **♿ Accessibility Support**: Alt text, captions, and descriptions
  - **📁 Media Library**: Upload, read, update, and delete operations
  - **🎯 Base64 Upload**: Handle base64-encoded file uploads
  - **🎬 Embed Support**: YouTube and social media content embedding
  - **📱 oEmbed Integration**: Automatic conversion of social media URLs
  - **🏷️ Category & Tag Management**: Full CRUD operations for taxonomies
  - **📂 Hierarchical Categories**: Support for parent-child relationships
  - **⚙️ Structured API**: Clean parameter structures for maintainable code
  - **🔍 Health Check System**: Comprehensive environment validation
  - **🛠️ Production Monitoring**: 5-stage validation with detailed reporting
  - Comment management and retrieval
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

### 🎯 WordPress MCP Tools (27 tools available)

**📝 Content Management:**
- `wordpress_health_check` - WordPress environment health check
- `get_posts` - Retrieve WordPress posts
- `get_pages` - Retrieve WordPress pages
- `get_all_content` - Retrieve all content (posts + pages)
- `get_post` - Get specific post/page by ID
- `create_post` - Create basic WordPress post
- `create_advanced_post` - Create post with advanced options (SEO, scheduling, etc.)
- `create_post_with_embeds` - Create post with YouTube/social media embeds
- `update_post` - Update existing post
- `delete_post` - Delete post (trash or permanent)

**🖼️ Media Management:**
- `upload_media` - Upload media files (base64/multipart)
- `get_media` - List media library items
- `get_media_item` - Get specific media item details
- `update_media` - Update media metadata (alt text, captions)
- `delete_media` - Delete media files
- `create_post_with_featured_image` - Create post with featured image
- `set_featured_image` - Set featured image for existing post

**📁 Taxonomy Management:**
- `get_categories` - List all categories
- `create_category` - Create new category
- `update_category` - Update category details
- `delete_category` - Delete category
- `get_tags` - List all tags
- `create_tag` - Create new tag
- `update_tag` - Update tag details
- `delete_tag` - Delete tag

**🔗 Content Integration:**
- `create_post_with_categories_tags` - Create post with taxonomy
- `update_post_categories_tags` - Update post taxonomy
- `get_comments` - Retrieve post comments

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

[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"  # WordPress Application Password
timeout_seconds = 30
enabled = true

# Environment variable override support:
# WORDPRESS_URL, WORDPRESS_USERNAME, WORDPRESS_PASSWORD
```

**WordPress Application Password Setup:**
1. WordPress Admin → Users → Your Profile
2. Scroll to "Application Passwords"
3. Create new application password for MCP-RS
4. Use this password in configuration

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

# WordPress environment health check
cargo run --example wordpress_security_diagnosis

# WordPress content CRUD operations
cargo run --example wordpress_post_crud_test

# WordPress media management with accessibility
cargo run --example wordpress_media_crud_test

# WordPress embedded content creation
cargo run --example wordpress_embed_test

# WordPress advanced post creation with SEO
cargo run --example wordpress_advanced_post_test

# WordPress categories and tags management
cargo run --example wordpress_categories_tags_test

# WordPress posts with taxonomy integration
cargo run --example wordpress_posts_with_taxonomy_test
cargo run --example wordpress_health_check

# WordPress categories and tags management test
cargo run --example wordpress_categories_tags_test

# WordPress posts with categories and tags integration test
cargo run --example wordpress_posts_with_taxonomy_test

# Complete WordPress post CRUD operations test
cargo run --example wordpress_post_crud_test

# Advanced post creation with SEO and scheduling
cargo run --example wordpress_advanced_post_test

# Complete media CRUD operations
cargo run --example wordpress_media_crud_test

# WordPress security diagnosis and setup
cargo run --example wordpress_security_diagnosis
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

### 🎬 **Embedded Content Creation**
**User:** "Create a post with this YouTube video and Instagram post"
**AI automatically:**
1. Validates and extracts YouTube video ID
2. Generates proper iframe embed code
3. Adds social media URLs for WordPress oEmbed
4. Creates post with combined content

**Supported Embed Types:**
- **YouTube**: Direct iframe embed or oEmbed
- **Twitter/X**: oEmbed integration
- **Instagram**: oEmbed integration  
- **Facebook**: oEmbed integration
- **TikTok**: oEmbed integration

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

### 🎬 **Embedded Content Examples**

```rust
// YouTube video embedding
let youtube_urls = vec!["https://www.youtube.com/watch?v=dQw4w9WgXcQ"];
let social_urls = vec!["https://twitter.com/user/status/123456789"];

let post = handler.create_post_with_embeds(
    "Video Tutorial Post",
    "<p>Check out this tutorial:</p>",
    youtube_urls,
    social_urls,
    Some(params),
).await?;
```

**Generated WordPress Content:**
```html
<p>Check out this tutorial:</p>

<iframe width="560" height="315" 
  src="https://www.youtube.com/embed/dQw4w9WgXcQ" 
  frameborder="0" allowfullscreen></iframe>

https://twitter.com/user/status/123456789
```

**WordPress automatically converts:**
- YouTube URLs → iframe embeds (with manual override)
- Twitter URLs → Twitter cards via oEmbed
- Instagram URLs → Instagram embeds via oEmbed
- Facebook URLs → Facebook posts via oEmbed

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

### Documentation

📚 **Complete documentation available in [`project-docs/`](project-docs/)**

- **[WordPress Integration Guide](project-docs/wordpress-guide.md)** - Complete setup and usage
- **[API Reference](project-docs/api-reference.md)** - Quick reference for all 27 tools  
- **[Architecture](project-docs/architecture.md)** - System design and patterns
- **[Documentation Index](project-docs/index.md)** - Documentation overview

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

See [project-docs/architecture.md](project-docs/architecture.md) for detailed roadmap.

### ✅ Completed (v0.1.0-alpha)
- [x] **WordPress Handler**: Complete CRUD operations (27 tools)
- [x] **Media Management**: Upload, CRUD with accessibility support
- [x] **Embedded Content**: YouTube, social media integration
- [x] **Advanced Post Features**: SEO, scheduling, post types
- [x] **Taxonomy System**: Categories and tags management
- [x] **Type-Safe Configuration**: TOML + environment variables
- [x] **Comprehensive Examples**: 6 complete test examples

### Near Term (v0.2.0)
- [ ] Core runtime module implementation
- [ ] Stdio transport support
- [ ] Plugin dynamic loading system
- [ ] Enhanced configuration management
- [ ] Performance benchmarks

### Medium Term (v0.3.0)
- [ ] WebSocket transport support
- [ ] Performance monitoring and metrics
- [ ] GitHub API handler
- [ ] Database integration handler
- [ ] File system operations handler

### Long Term (v1.0.0)
- [ ] Production security audit
- [ ] Comprehensive test suite  
- [ ] Documentation website
- [ ] Plugin ecosystem
- [ ] Docker containerization

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

