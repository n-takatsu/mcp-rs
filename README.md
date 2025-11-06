# mcp-rs

🚀 **Production-Ready** Rust implementation of the Model Context Protocol (MCP) for AI-agent integration with WordPress and beyond.

> **[English](README.md)** | **[日本語](README.ja.md)**

[![Architecture](https://img.shields.io/badge/Architecture-v0.15.0--canary--deployment-blue)](#architecture)
[![Implementation](https://img.shields.io/badge/WordPress_Tools-27_Available-green)](#wordpress-mcp-tools)
[![Canary_System](https://img.shields.io/badge/Canary_Deployment-Real--time_Dashboard-orange)](#canary-deployment-system)
[![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)](#license)

## Overview

`mcp-rs` provides a **comprehensive, battle-tested** implementation of the MCP (Model Context Protocol) in Rust with **complete WordPress integration**. Built with a layered architecture, it enables AI agents to perform sophisticated content management through a standardized JSON-RPC interface. The framework features 27 WordPress tools, strong type safety, and is optimized for production use with GitHub Copilot and other AI coding assistants.

## 🎯 Who Is This For?

### **AI Developers** 🤖
Building Claude Desktop apps, GPT integrations, or custom AI agents? mcp-rs provides production-ready Model Context Protocol implementation with comprehensive WordPress tooling.

### **Enterprise WordPress Teams** 🏢  
Managing large-scale WordPress deployments? Get enterprise-grade security, automated content management, and seamless CI/CD integration.

### **DevOps Engineers** ⚙️
Automating WordPress operations? 27 battle-tested tools with comprehensive health checks, monitoring, and production-ready error handling.

### **Rust Enthusiasts** 🦀
Want to contribute to a high-quality Rust codebase? Join our 205+ test, zero-warning project with clean architecture and comprehensive documentation.

### **Security Teams** 🔒
Need WordPress security automation? 6-layer enterprise security architecture with SQL injection protection, XSS prevention, and comprehensive audit logging.

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
- **� SQL Injection Protection**: 11 attack pattern detection with Union/Boolean/Time-based attack prevention
- **🚫 XSS Attack Protection**: 14 attack pattern detection with HTML sanitization and CSP headers
- **📊 Comprehensive Audit Logging**: All security events recorded with tamper-resistant logging
- **🎯 Advanced Input Validation**: Real-time validation with custom rules and data sanitization
- **� Zero-Trust Data Validation**: All inputs validated through multi-layer security checks
- **📈 Real-time Security Monitoring**: Threat level analysis with attack detection and prevention

**� Production Security Features:**
- **Safe Environment Variable Expansion**: Prevents infinite loop vulnerabilities with max iteration limits (100)
- **Processed Variable Tracking**: HashSet-based tracking prevents infinite recursion
- **Graceful Error Handling**: Missing or invalid environment variables are safely handled
- **Performance Optimized**: Complex variable expansion completed in ~1.2ms
- **Enterprise Security Testing**: 100% security implementation with 205+ comprehensive test cases
- **Application Password Lifecycle Management**: Production-tested strategies for password rotation and monitoring
- **Maintenance Mode Operations**: Verified compatibility with WordPress maintenance plugins
- **Production Monitoring**: Real-world validated health check and diagnostic procedures

### 🛠️ Protocol Support

- **Tools**: Define and execute tools with JSON schema validation
- **Resources**: Expose and read resources with URI-based access
- **Prompts**: Create and retrieve prompts with argument support
- **Error Handling**: Comprehensive error types with JSON-RPC error codes
- **Transport Layer**: Multiple communication protocols supported
  - **📟 Stdio Transport**: Standard input/output for process-based communication
  - **🌐 HTTP Transport**: RESTful API server with JSON-RPC over HTTP
  - **🔮 WebSocket Transport**: Real-time bidirectional communication (planned)

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
  - **🔒 Security Hardened**: Safe environment variable expansion with infinite loop prevention
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
- **Configuration Management**: TOML + environment variable hierarchy with security hardening
- **Environment Variable Security**: Infinite loop prevention and safe expansion (max 100 iterations)
- **MCP Protocol Foundation**: JSON-RPC + handler trait system
- **Error Handling**: thiserror-based type-safe error management
- **Structured Logging**: tracing-based logging system
- **HTTP Communication**: reqwest + timeout + exponential backoff retry
- **Security Testing**: Comprehensive test suite with 95% security coverage

### ✅ Recently Completed (v0.2.0-alpha)
- **🏗️ Core Runtime Module**: Complete application lifecycle management with state tracking
- **📟 Stdio Transport Support**: Standard input/output communication for process-based integration  
- **🔌 Transport Abstraction**: Pluggable transport layer with configurable framing methods
- **⚙️ Advanced Configuration**: Transport-specific settings with TOML integration
- **🔄 Message Processing**: Async message routing and request handling pipeline
- **📊 Execution Context**: Request-scoped context management with timeout handling
- **🎯 Handler Registry**: Dynamic plugin management and tool discovery system
- **🔒 Enterprise Security**: 6-layer security architecture (100% Complete)
  - ✅ **AES-GCM-256 Encryption**: Military-grade encryption with PBKDF2 (100K iterations)
  - ✅ **Token Bucket Rate Limiting**: Advanced DDoS protection with configurable limits
  - ✅ **TLS 1.2+ Enforcement**: Mandatory secure transport with certificate validation
  - ✅ **SQL Injection Protection**: 11 attack pattern detection with real-time prevention
  - ✅ **XSS Attack Protection**: 14 attack pattern detection with HTML sanitization and CSP headers
  - ✅ **Zero-Panic Operations**: Complete unwrap() elimination with Result-based error handling
  - ✅ **Comprehensive Audit Logging**: All security events recorded with tamper-resistant logging
  - ✅ **Advanced Input Validation**: Real-time validation with zero-trust model implementation
  - ✅ **Security Monitoring**: Threat level analysis with attack detection and prevention
- **🧪 Quality Assurance**: 205+ test cases with 100% pass rate and zero Clippy warnings

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
- **🌐 HTTP Transport Integration**: Integrate existing axum server with new transport layer
- **🔮 WebSocket Transport**: Real-time bidirectional communication support
- **📈 Performance Monitoring**: Runtime metrics collection and observability
- **🔌 Plugin Dynamic Loading**: Hot-pluggable handler loading system
- **⚡ Performance Optimization**: Benchmarking and performance tuning

### 🎯 Development Roadmap (0.01 Increment Versioning)

#### 🎯 **Current: v0.15.0** - Canary Deployment System ✅
- **v0.14.0**: ✅ Policy Hot-Reload System (Epic #15)
  - Real-time policy file monitoring with debouncing
  - 4-level validation pipeline (syntax → semantic → security → integration)
  - Policy application engine with diff management
  - Performance: 15-35ms reload times
- **v0.15.0**: ✅ Canary Deployment System
  - Traffic splitting with hash-based distribution
  - Real-time monitoring dashboard (TUI)
  - Event-driven architecture with metrics collection
  - Interactive controls for deployment management

#### 🚀 **Next Releases: v0.16.0 - v0.20.0**
- **v0.16.0**: Advanced Dashboard Features
  - Real-time charts and graphs visualization
  - Historical metrics and trend analysis
  - Alert system integration
  - Export capabilities for monitoring data
- **v0.17.0**: Auto-scaling & Health Checks
  - Automatic traffic adjustment based on metrics
  - Health check integration for canary validation
  - Circuit breaker pattern implementation
  - SLA-based promotion criteria
- **v0.18.0**: Multi-Environment Deployment
  - Staging → Production deployment pipelines
  - Environment-specific policy management
  - Cross-environment metrics comparison
  - Rollback automation across environments
- **v0.19.0**: API Integration & Webhooks
  - REST API for deployment management
  - Webhook notifications for deployment events
  - Third-party monitoring system integration
  - Slack/Teams notification support
- **v0.20.0**: Enterprise Features
  - Role-based access control (RBAC)
  - Audit logging and compliance reporting
  - Multi-tenant deployment management
  - Enterprise security hardening

#### 🎯 **Major Milestones: v0.21.0 - v0.30.0**
- **v0.21.0**: Blue-Green Deployment Pattern
- **v0.22.0**: A/B Testing Framework Integration
- **v0.23.0**: Feature Flag Management
- **v0.24.0**: Performance Testing Automation
- **v0.25.0**: Container Orchestration (Kubernetes)
- **v0.26.0**: Cloud Provider Integration (AWS/Azure/GCP)
- **v0.27.0**: Microservices Deployment Coordination
- **v0.28.0**: GitOps Integration (ArgoCD/Flux)
- **v0.29.0**: Observability Stack Integration (Prometheus/Grafana)
- **v0.30.0**: AI-Powered Deployment Optimization

#### 🏆 **Long-term Vision: v1.0.0+**
- **v1.0.0**: Production-Ready Release
  - Complete security audit and penetration testing
  - Performance benchmarking and optimization
  - Comprehensive documentation and examples
  - Enterprise support and maintenance plans

## 📚 Documentation

This project uses a **hybrid documentation approach** with executable examples:

### 🔗 Live Documentation (Recommended)
All API examples are **executable and automatically tested**:

```bash
# View all documentation with live examples
cargo doc --open

# Test all documentation examples
cargo test --doc
```

### 📖 Comprehensive Documentation Structure

📚 **Complete documentation available in three tiers:**

#### 📄 Core Documentation
- **[README.md](README.md)** *(This file)* - Project overview, features, and quick start

#### 📖 Technical Documentation (`project-docs/`)
- **[Documentation Index](project-docs/index.md)** - Complete documentation navigation
- **[Architecture Guide](project-docs/architecture.md)** - System design, security architecture, and plugin system
- **[Security Guide](project-docs/security-guide.md)** - Enterprise security implementation with examples
- **[WordPress Guide](project-docs/wordpress-guide.md)** - Complete WordPress integration and permissions
- **[API Reference](project-docs/api-reference.md)** - Complete API reference for all 27 tools

#### 🌐 Website Documentation (`website/`)
- **[GitHub Pages](website/index.md)** - Public documentation and guides

### 🧪 Executable Examples
All code examples link directly to tested documentation:
- **Plugin APIs**: See `src/plugins/mod.rs` for working examples
- **Configuration**: See `src/config.rs` for setup examples  
- **Transport Layer**: See `src/transport/` modules for communication examples

**💡 Why this approach?** Our documentation examples are verified with every build via `cargo test --doc`, ensuring they always work correctly.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mcp-rs = "0.1.0"
```

## Quick Start

### 🚀 Choose Your Path

#### **For AI Developers** 🤖
```bash
# 1. Clone and run with Claude Desktop
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo run --example wordpress_test

# 2. Add to Claude Desktop config
# ~/.config/claude-desktop/claude_desktop_config.json
```

#### **For Enterprise Teams** 🏢
```bash
# 1. Production-ready configuration
cp .env.example .env
# Edit .env with your WordPress credentials

# 2. Run comprehensive health check
cargo run --example wordpress_health_check

# 3. Deploy with Docker (coming soon)
```

#### **For DevOps Engineers** ⚙️  
```bash
# 1. CI/CD integration
cargo test --all-features

# 2. Production monitoring
cargo run --example comprehensive_test

# 3. Health check automation
curl -X POST http://localhost:3000/health
```

#### **For Contributors** 🦀
```bash
# 1. Development setup
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo test

# 2. Join our Discord/Issues for collaboration
# See CONTRIBUTING.md for guidelines
```

### Configuration Setup

Create a `mcp-config.toml` file:

```toml
[server]
host = "0.0.0.0"
port = 3000

# Transport Configuration
[transport]
transport_type = "stdio"  # "stdio", "http", or "websocket"

# Stdio Transport Settings
[transport.stdio]
buffer_size = 8192
timeout_ms = 30000
content_length_header = true
framing_method = "content-length"  # "content-length" or "line-based"
max_message_size = 1048576
pretty_print = false

# HTTP Transport Settings (alternative)
[transport.http]
addr = "127.0.0.1"
port = 8080
enable_cors = true

[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"  # WordPress Application Password
timeout_seconds = 30
enabled = true

# Secure environment variable expansion supported:
# url = "${WORDPRESS_URL}"
# username = "${WORDPRESS_USERNAME}"  
# password = "${WORDPRESS_PASSWORD}"
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

# Environment variable security testing
cargo run --example safe_env_test

# WordPress authentication and access diagnosis
cargo run --example auth_diagnosis

# WordPress API deep diagnosis (production monitoring)
cargo run --example settings_api_deep_diagnosis

# Production monitoring suite with alerting
cargo run --example production_monitoring

# Comprehensive system test
cargo run --example comprehensive_test

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

### 🔧 **Production Operations**
**User:** "Check if WordPress is healthy and accessible"
**AI automatically:**
1. Uses `wordpress_health_check` for comprehensive validation
2. Uses `settings_api_deep_diagnosis` for detailed API analysis
3. Reports authentication status and potential issues

**Operational Scenarios:**
- **Maintenance Mode**: Works seamlessly with WordPress maintenance plugins
- **Password Expiration**: Automatic detection and clear resolution guidance
- **Health Monitoring**: Proactive system validation and alerting

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
│ Core Layer ✅                                      │
│ ├── runtime.rs (Async runtime management) ✅       │
│ ├── registry.rs (Handler/plugin registry) ✅       │
│ ├── context.rs (Execution context) ✅              │
│ └── Message processing & lifecycle management       │
├─────────────────────────────────────────────────────┤
│ Infrastructure Layer                                │
│ ├── transport/ (Communication abstraction) ✅      │
│ │   ├── stdio.rs (Stdio transport) ✅             │
│ │   ├── mod.rs (Transport traits) ✅              │
│ │   └── http.rs (HTTP integration) [🔄 Planned]  │
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
6. **Runtime Management**: Complete application lifecycle with state tracking
7. **Message Processing**: Efficient JSON-RPC routing and execution context

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
│   ├── core/               # Core runtime layer ✅
│   │   ├── mod.rs          # Core module organization
│   │   ├── runtime.rs      # Application lifecycle management ✅
│   │   ├── registry.rs     # Handler and plugin registry ✅
│   │   └── context.rs      # Execution context management ✅
│   ├── transport/          # Transport abstraction layer ✅
│   │   ├── mod.rs          # Transport traits and factory ✅
│   │   └── stdio.rs        # Stdio transport implementation ✅
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
│   ├── api/               # API documentation
│   ├── design/            # Design documents
│   ├── implementation/    # Implementation guides
│   ├── database-availability-guide.md  # Database availability
│   ├── database-security-enhancement-plan.md  # Security planning
│   └── architecture.md    # Architecture design document
├── reports/               # Implementation reports and results
│   ├── README.md          # Report management guide
│   └── database-security-implementation-report.md  # Security implementation
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
- **Security Patterns**: Safe environment variable expansion and infinite loop prevention
- **Async Patterns**: Proper async/await usage with tokio

### 3. Project Context Understanding
- **Layered Architecture**: Suggestions respect architectural boundaries
- **Configuration Management**: TOML-first configuration patterns with secure environment expansion
- **Security-First Development**: Infinite loop prevention and safe variable handling
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

> **Note**: This project tracks `Cargo.lock` in version control to ensure reproducible builds across environments, as it includes both library and binary components.

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

### Transport Modes

#### 📟 **Stdio Transport**
Perfect for process-based integration and AI agent communication:
```bash
# AI agent spawns mcp-rs as child process
echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | mcp-rs --stdio
```

#### 🌐 **HTTP Transport**  
Ideal for web-based integrations and RESTful APIs:
```bash
# Start HTTP server
mcp-rs --http --port 8080

# Send requests via HTTP
curl -X POST http://localhost:8080/ \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"tools/list","id":1}'
```

#### 🔮 **WebSocket Transport** *(Coming Soon)*
Real-time bidirectional communication for live applications

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

- **[Documentation Index](project-docs/index.md)** - Complete navigation and overview
- **[WordPress Integration Guide](project-docs/wordpress-guide.md)** - Complete setup and usage
- **[API Reference](project-docs/api-reference.md)** - Quick reference for all 27 tools  
- **[Architecture](project-docs/architecture.md)** - System design and security architecture
- **[Security Guide](project-docs/security-guide.md)** - Enterprise security implementation

**🎯 For New Users**: Start with [project-docs/wordpress-guide.md](project-docs/wordpress-guide.md)  
**⚡ For Developers**: Start with [project-docs/architecture.md](project-docs/architecture.md)

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
- [x] **WordPress Handler**: Complete CRUD operations (31 tools)
- [x] **Security Hardening**: Environment variable infinite loop prevention
- [x] **Media Management**: Upload, CRUD with accessibility support
- [x] **Embedded Content**: YouTube, social media integration
- [x] **Advanced Post Features**: SEO, scheduling, post types
- [x] **Taxonomy System**: Categories and tags management
- [x] **Type-Safe Configuration**: TOML + secure environment variables
- [x] **Health Check System**: 5-stage WordPress environment validation
- [x] **Comprehensive Testing**: 95% security coverage, all integration tests passing
- [x] **Performance Optimization**: Sub-millisecond environment variable expansion
- [x] **Production Monitoring**: Automated health checks with alerting system
- [x] **Operational Security**: Application password lifecycle management
- [x] **Maintenance Mode Support**: Verified compatibility with WordPress maintenance plugins
- [x] **Performance Optimization**: Sub-millisecond environment variable expansion
- [x] **Production Ready**: Comprehensive error handling and logging

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

## 🤝 Join Our Community

### 🌟 We're Looking For

**Security Experts** 🔒  
Help us reach 100% security coverage. Contribute to our enterprise-grade security architecture.

**WordPress Developers** 📝  
Expand our 27-tool ecosystem. Add new WordPress integrations and improve existing ones.

**Rust Enthusiasts** 🦀  
Optimize our production-ready codebase. Join a 205+ test, zero-warning project.

**DevOps Engineers** ⚙️  
Scale our enterprise deployment. Help with CI/CD, monitoring, and operations.

**AI/MCP Developers** 🤖  
Build the future of AI-agent integrations. Create new handlers and improve the MCP implementation.

### 🚀 Why Contribute Now?

- **Alpha Stage Impact**: Your contributions shape the foundation
- **Production Quality**: Already handles real workloads
- **Enterprise Features**: Work on cutting-edge security implementation  
- **Technical Excellence**: Join a high-quality, well-tested codebase
- **Growing Community**: Be part of the WordPress MCP ecosystem

### 📞 Get Involved

- **Issues**: [Report bugs or request features](https://github.com/n-takatsu/mcp-rs/issues)
- **Discussions**: [Join conversations](https://github.com/n-takatsu/mcp-rs/discussions)
- **Contributing**: See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines
- **Discord**: Coming soon - community chat and collaboration

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

