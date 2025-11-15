---
layout: page
title: Implementation Guides
permalink: /docs/guides/
---

# MCP-RS Implementation Guides

Step-by-step guides for implementing and using MCP-RS.

## Quick Start Guides

- **[WordPress Integration]({{ site.baseurl }}/docs/wordpress/)** - Complete WordPress REST API integration with media management
- **[Database Integration]({{ site.baseurl }}/docs/database/)** - Multi-engine database setup and configuration
- **[Security Setup]({{ site.baseurl }}/docs/security/)** - Enterprise-grade security configuration
- **[Custom Handler Development](#custom-handler-development)** - Create your own MCP handlers
- **[Configuration Guide](#configuration-guide)** - Configure MCP-RS for your environment

## Getting Started

### Installation and Setup

#### 1. Project Setup

Create a new Rust project and add MCP-RS as a dependency:

```bash
cargo new my-mcp-server
cd my-mcp-server
```

Add to `Cargo.toml`:
```toml
[dependencies]
mcp-rs = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
```

#### 2. Basic Server Implementation

Create a basic MCP server in `src/main.rs`:

```rust
use mcp_rs::{
    server::McpServer,
    config::Config,
    handlers::wordpress::WordPressHandler,
    mcp::McpHandler,
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::init();

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
    let address = config.server.address();
    println!("Starting MCP server on {}", address);
    server.serve(address).await?;

    Ok(())
}
```

#### 3. Configuration File

Create `mcp-config.toml`:

```toml
[server]
host = "127.0.0.1"
port = 3000

# WordPress handler configuration (optional)
[wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"
timeout_seconds = 30
```

## WordPress Integration Guide

### 1. WordPress Setup

#### Generate Application Password

1. Log into your WordPress admin dashboard
2. Go to **Users** → **Profile**
3. Scroll to **Application Passwords** section
4. Enter application name (e.g., "MCP-RS")
5. Click **Add New Application Password**
6. Copy the generated password

#### Configure WordPress Handler

Update `mcp-config.toml`:

```toml
[server]
host = "127.0.0.1"
port = 8080

[wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password_here"
timeout_seconds = 30
```
log_level = "info"

[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your-username"
password = "generated-app-password"
enabled = true
```

### 2. WordPress Handler Implementation

Create a custom server with WordPress integration:

```rust
use mcp_rs::{
    McpServer,
    config::Config,
    handlers::wordpress::WordPressHandler,
    protocol::BasicMcpProtocol,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::init();

    // Load configuration
    let config = Config::load()?;

    // Create protocol
    let mut protocol = BasicMcpProtocol::new("wordpress-server", "1.0.0");

    // Add WordPress handler if enabled
    if let Some(wp_config) = config.handlers.wordpress {
        if wp_config.enabled {
            let wp_handler = WordPressHandler::new(wp_config).await?;
            protocol.add_handler("wordpress", Box::new(wp_handler));
        }
    }

    // Start server
    let server = McpServer::new(protocol);
    let bind_addr = config.server.bind_addr.parse()?;
    println!("Starting WordPress MCP server on {}", bind_addr);
    server.serve(bind_addr).await?;

    Ok(())
}
```

### 3. Using WordPress Tools

#### Get Posts

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "wordpress_get_posts",
    "arguments": {
      "per_page": 5,
      "status": "publish"
    }
  },
  "id": 1
}
```

#### Create a Post

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "wordpress_create_post",
    "arguments": {
      "title": "My New Post",
      "content": "<p>This is the post content.</p>",
      "status": "draft"
    }
  },
  "id": 2
}
```

#### Environment Health Check

Before using WordPress tools, validate your environment:

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "wordpress_health_check",
    "arguments": {}
  },
  "id": 3
}
```

**Response Example:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "content": [{
      "type": "text",
      "text": "WordPress Health Check Report\n\n✅ Status: healthy\n\n🌐 Site Information:\n- URL: https://your-site.com\n- Name: Your Site Name\n- Version: 6.3.2\n\n👤 User Information:\n- Username: admin\n- Display Name: Site Administrator\n- Roles: administrator\n\n🔐 Capabilities:\n- publish_posts: ✅\n- upload_files: ✅\n- edit_posts: ✅\n\n📁 Media Upload:\n- Status: ✅ Supported\n- Max Upload Size: 64M\n\n⚡ Performance:\n- Site Response: 245ms\n- API Response: 89ms\n\n✅ All systems operational! WordPress is ready for MCP operations."
    }],
    "is_error": false
  },
  "id": 3
}
```

#### Upload Media with Featured Image

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "wordpress_upload_media",
    "arguments": {
      "file_data": "iVBORw0KGgoAAAANSUhEUgAA...",
      "filename": "featured-image.jpg",
      "mime_type": "image/jpeg"
    }
  },
  "id": 4
}
```

Then use the returned media ID to create a post:

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "wordpress_create_post_with_featured_image",
    "arguments": {
      "title": "Post with Featured Image",
      "content": "<p>This post has a beautiful featured image.</p>",
      "featured_media_id": 123
    }
  },
  "id": 5
}
```

### 4. Production Workflow

#### Pre-Production Checklist

1. **Environment Validation**
   ```bash
   # Run health check example
   cargo run --example wordpress_health_check
   ```

2. **Configuration Verification**
   - Verify WordPress URL is accessible
   - Test application password authentication
   - Check user permissions are sufficient

3. **Functional Testing**
   - Test post creation and retrieval
   - Verify media upload capabilities
   - Confirm featured image functionality

#### Monitoring and Maintenance

- **Regular Health Checks**: Schedule periodic environment validation
- **Performance Monitoring**: Track API response times and success rates
- **Error Handling**: Implement proper error logging and alerting
- **Backup Procedures**: Ensure WordPress content is properly backed up

## Custom Handler Development

### 1. Implement the McpHandler Trait

```rust
use async_trait::async_trait;
use mcp_rs::{
    protocol::McpHandler,
    types::{Tool, Resource, CallToolResult},
    Error, Result,
};
use serde_json::Value;

pub struct MyCustomHandler {
    // Handler configuration and state
}

#[async_trait]
impl McpHandler for MyCustomHandler {
    async fn list_tools(&self) -> Result<Vec<Tool>> {
        Ok(vec![
            Tool {
                name: "my_custom_tool".to_string(),
                description: "A custom tool implementation".to_string(),
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": {"type": "string"}
                    }
                })),
            }
        ])
    }

    async fn call_tool(&self, name: &str, args: Option<Value>) -> Result<CallToolResult> {
        match name {
            "my_custom_tool" => {
                let input = args
                    .and_then(|v| v.get("input"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");

                // Implement your tool logic here
                let result = format!("Processed: {}", input);

                Ok(CallToolResult {
                    content: vec![serde_json::json!({
                        "type": "text",
                        "text": result
                    })],
                    is_error: false,
                })
            }
            _ => Err(Error::MethodNotFound(name.to_string())),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>> {
        // Implement resource listing
        Ok(vec![])
    }

    async fn read_resource(&self, uri: &str) -> Result<String> {
        // Implement resource reading
        Err(Error::ResourceNotFound(uri.to_string()))
    }
}
```

### 2. Register the Handler

```rust
// Add to your server setup
let custom_handler = MyCustomHandler::new();
protocol.add_handler("custom", Box::new(custom_handler));
```

## Configuration Management

### 1. Configuration Structure

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct MyConfig {
    pub server: ServerConfig,
    pub my_handler: MyHandlerConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MyHandlerConfig {
    pub api_key: String,
    pub endpoint: String,
    pub timeout: u64,
    pub enabled: bool,
}
```

### 2. Environment Variable Overrides

Set environment variables to override configuration:

```bash
export MCP_SERVER_BIND_ADDR="0.0.0.0:8080"
export MCP_MY_HANDLER_API_KEY="your-api-key"
export MCP_MY_HANDLER_ENABLED="true"
```

### 3. Loading Configuration

```rust
use mcp_rs::config::Config;

// Load from file with environment overrides
let config = Config::load_from_file("config.toml")?;

// Or load from default locations
let config = Config::load()?;
```

## Error Handling Best Practices

### 1. Custom Error Types

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyHandlerError {
    #[error("API request failed: {0}")]
    ApiError(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Timeout after {0}s")]
    Timeout(u64),
}

// Convert to MCP Error
impl From<MyHandlerError> for mcp_rs::Error {
    fn from(err: MyHandlerError) -> Self {
        mcp_rs::Error::InternalError(err.to_string())
    }
}
```

### 2. Error Handling in Tools

```rust
async fn call_tool(&self, name: &str, args: Option<Value>) -> Result<CallToolResult> {
    match name {
        "my_tool" => {
            match self.execute_tool(args).await {
                Ok(result) => Ok(CallToolResult {
                    content: vec![serde_json::json!({
                        "type": "text",
                        "text": result
                    })],
                    is_error: false,
                }),
                Err(e) => Ok(CallToolResult {
                    content: vec![serde_json::json!({
                        "type": "text",
                        "text": format!("Error: {}", e)
                    })],
                    is_error: true,
                }),
            }
        }
        _ => Err(Error::MethodNotFound(name.to_string())),
    }
}
```

## Testing

### 1. Unit Testing Handlers

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_my_custom_tool() {
        let handler = MyCustomHandler::new();

        let result = handler.call_tool(
            "my_custom_tool",
            Some(json!({"input": "test"}))
        ).await.unwrap();

        assert!(!result.is_error);
        // Add more assertions
    }
}
```

### 2. Integration Testing

```rust
#[tokio::test]
async fn test_server_integration() {
    // Start test server
    let protocol = BasicMcpProtocol::new("test", "1.0.0");
    let server = McpServer::new(protocol);

    // Test with HTTP client
    // Add integration test logic
}
```

## Performance Optimization

### 1. Connection Pooling

```rust
use reqwest::Client;
use std::sync::Arc;

pub struct OptimizedHandler {
    client: Arc<Client>,
}

impl OptimizedHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap();

        Self {
            client: Arc::new(client),
        }
    }
}
```

### 2. Caching

```rust
use std::collections::HashMap;
use std::sync::RwLock;

pub struct CachedHandler {
    cache: RwLock<HashMap<String, (String, std::time::Instant)>>,
    cache_duration: Duration,
}

impl CachedHandler {
    async fn get_cached_or_fetch(&self, key: &str) -> Result<String> {
        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some((value, timestamp)) = cache.get(key) {
                if timestamp.elapsed() < self.cache_duration {
                    return Ok(value.clone());
                }
            }
        }

        // Fetch and cache
        let value = self.fetch_data(key).await?;
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(key.to_string(), (value.clone(), std::time::Instant::now()));
        }

        Ok(value)
    }
}
```

## Deployment

### 1. Docker Deployment

Create `Dockerfile`:

```dockerfile
FROM rust:1.70 AS builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/my-mcp-server /usr/local/bin/
COPY --from=builder /app/mcp-config.toml /etc/mcp/

EXPOSE 8080
CMD ["my-mcp-server"]
```

### 2. Systemd Service

Create `/etc/systemd/system/mcp-rs.service`:

```ini
[Unit]
Description=MCP-RS Server
After=network.target

[Service]
Type=simple
User=mcp
WorkingDirectory=/opt/mcp-rs
ExecStart=/opt/mcp-rs/mcp-rs
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### 3. Environment Configuration

Production environment variables:

```bash
export MCP_SERVER_BIND_ADDR="0.0.0.0:8080"
export MCP_SERVER_LOG_LEVEL="warn"
export RUST_LOG="warn"
```

## Configuration Guide

This section provides comprehensive configuration options for MCP-RS.

### Environment Variables

MCP-RS supports environment variable expansion in configuration files using the `${VAR_NAME}` syntax:

```toml
[handlers.wordpress]
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
```

#### Core Environment Variables

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `MCP_SERVER_BIND_ADDR` | Server bind address | `127.0.0.1:8080` | `0.0.0.0:3000` |
| `MCP_SERVER_LOG_LEVEL` | Logging level | `info` | `debug`, `warn`, `error` |
| `RUST_LOG` | Rust logging configuration | `info` | `mcp_rs=debug` |

#### WordPress Integration Variables

| Variable | Description | Required | Example |
|----------|-------------|----------|---------|
| `WORDPRESS_URL` | WordPress site URL | Yes | `https://example.com` |
| `WORDPRESS_USERNAME` | WordPress username | Yes | `admin` |
| `WORDPRESS_PASSWORD` | Application password | Yes | `xxxx xxxx xxxx xxxx xxxx xxxx` |

### Security Configuration

#### Rate Limiting
```toml
[handlers.wordpress.rate_limit]
max_requests_per_minute = 60
burst_size = 10
enabled = true
```

#### Timeout Settings
```toml
[handlers.wordpress]
timeout_seconds = 30
retry_attempts = 3
retry_delay_ms = 1000
```

### Deployment Configurations

#### Development
```toml
[server]
bind_addr = "127.0.0.1:8080"
log_level = "debug"

[handlers.wordpress]
timeout_seconds = 60  # Longer timeout for debugging
```

#### Production
```toml
[server]
bind_addr = "0.0.0.0:8080"
log_level = "warn"

[handlers.wordpress]
timeout_seconds = 30
rate_limit.enabled = true
rate_limit.max_requests_per_minute = 100
```

#### Docker
```toml
[server]
bind_addr = "0.0.0.0:8080"
log_level = "info"

# Use environment variables for sensitive data
[handlers.wordpress]
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
```

---

For more advanced topics and API reference, see the [API Documentation](../api/).
