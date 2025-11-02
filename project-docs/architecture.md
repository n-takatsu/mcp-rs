# mcp-rs Architecture

## Overview

`mcp-rs` follows a layered architecture designed for production scalability, maintainability, and extensibility. The system is built around the Model Context Protocol (MCP) JSON-RPC specification with a focus on AI agent integration.

## Architecture Layers

### 1. Application Layer (`src/main.rs`)
- **Responsibility**: Entry point, configuration loading, server lifecycle
- **Components**: Main server startup, signal handling, graceful shutdown
- **Dependencies**: Configuration, Server layers

### 2. API Layer (`src/server.rs`)
- **Responsibility**: HTTP server, JSON-RPC protocol implementation
- **Components**: Axum HTTP server, JSON-RPC request routing, response handling
- **Dependencies**: Handler layer
- **Features**: Timeout handling, error serialization, request logging

### 3. Service Layer (`src/handlers/`)
- **Responsibility**: Business logic, external API integration
- **Components**: WordPress handler, future handlers (GitHub, Database, etc.)
- **Dependencies**: MCP protocol types, Configuration
- **Pattern**: Plugin-based architecture with `McpHandler` trait

### 4. Core Layer (`src/mcp/`)
- **Responsibility**: MCP protocol implementation, type definitions
- **Components**: Protocol types, error handling, serialization
- **Dependencies**: Serde, JSON-RPC types
- **Standards**: Strict MCP JSON-RPC 2.0 compliance

### 5. Infrastructure Layer (`src/config.rs`, `src/error.rs`)
- **Responsibility**: Configuration management, error handling, logging
- **Components**: TOML configuration, environment variables, structured errors
- **Dependencies**: External crates (config, thiserror, tracing)

## Handler Plugin Architecture

### McpHandler Trait
```rust
#[async_trait]
pub trait McpHandler: Send + Sync {
    async fn initialize(&self, params: InitializeParams) -> Result<serde_json::Value, McpError>;
    async fn list_tools(&self) -> Result<Vec<Tool>, McpError>;
    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError>;
    async fn list_resources(&self) -> Result<Vec<Resource>, McpError>;
    async fn read_resource(&self, params: ResourceReadParams) -> Result<serde_json::Value, McpError>;
}
```

### WordPress Handler Implementation

The WordPress handler (`src/handlers/wordpress.rs`) provides 27 comprehensive tools:

#### Content Management (10 tools)
- Complete CRUD operations for posts and pages
- Advanced post creation with SEO metadata
- Post scheduling and status management
- Embedded content support (YouTube, social media)

#### Media Management (7 tools)
- Full media library CRUD operations
- Base64 and multipart file upload support
- Accessibility features (alt text, captions, descriptions)
- Featured image management

#### Taxonomy Management (8 tools)
- Categories: hierarchical taxonomy with parent/child relationships
- Tags: flat taxonomy for content labeling
- Complete CRUD operations for both taxonomies
- Post-taxonomy integration

#### Integration Tools (2 tools)
- Health check and diagnostics
- Comment management

## Configuration Architecture

### Hierarchical Configuration
1. **Default Values**: Hardcoded fallbacks
2. **TOML Configuration**: `mcp-config.toml` file
3. **Environment Variables**: Override any TOML setting

### Configuration Structure
```toml
[server]
host = "0.0.0.0"
port = 3000

[handlers.wordpress]
url = "https://wordpress-site.com"
username = "admin"
password = "app_password"
timeout_seconds = 30
enabled = true
```

### Environment Override Pattern
- `WORDPRESS_URL` overrides `handlers.wordpress.url`
- `WORDPRESS_USERNAME` overrides `handlers.wordpress.username`
- `WORDPRESS_PASSWORD` overrides `handlers.wordpress.password`

## Error Handling Architecture

### Error Types Hierarchy
```rust
#[derive(thiserror::Error, Debug)]
pub enum McpError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    
    #[error("HTTP error: {0}")]
    Http(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
}
```

### Error Flow
1. **Handler Level**: Specific errors (HTTP, validation, business logic)
2. **MCP Level**: Protocol-specific error transformation
3. **JSON-RPC Level**: Standard JSON-RPC error response formatting
4. **HTTP Level**: HTTP status code mapping

## Performance Architecture

### Async-First Design
- Built on Tokio runtime for high concurrency
- All I/O operations are non-blocking
- Connection pooling for external APIs
- Request timeout handling

### Retry Logic
```rust
// Exponential backoff with jitter
let delay = Duration::from_millis(base_delay_ms * 2_u64.pow(attempt) + random_jitter);
```

### Resource Management
- Automatic connection cleanup
- Memory-efficient JSON streaming
- Configurable timeouts per handler

## Security Architecture

### Authentication
- WordPress: Application Password support
- Secure credential storage (environment variables preferred)
- No plaintext password logging

### Input Validation
- JSON schema validation for all tool inputs
- Type-safe deserialization with Serde
- SQL injection prevention (parameterized queries)

### Transport Security
- HTTPS enforcement for external APIs
- TLS certificate validation
- Request/response logging (excluding sensitive data)

## Testing Architecture

### Test Organization
```
examples/
├── wordpress_embed_test.rs          # Embedded content functionality
├── wordpress_media_crud_test.rs     # Media management operations
├── wordpress_post_crud_test.rs      # Post CRUD operations
├── wordpress_advanced_post_test.rs  # Advanced post features
├── wordpress_categories_tags_test.rs # Taxonomy management
└── wordpress_posts_with_taxonomy_test.rs # Integration tests
```

### Test Patterns
- **Integration Tests**: Full workflow testing with real API calls
- **Unit Tests**: Individual function testing with mocked dependencies
- **Error Path Testing**: Comprehensive error condition coverage

## Scalability Considerations

### Horizontal Scaling
- Stateless handler design
- No session storage requirements
- Load balancer friendly

### Vertical Scaling
- Efficient memory usage with streaming
- Connection pooling
- Configurable concurrency limits

### Handler Isolation
- Each handler operates independently
- Failure in one handler doesn't affect others
- Plugin-based architecture allows selective enabling

## Monitoring and Observability

### Structured Logging
```rust
use tracing::{info, warn, error};

info!(
    "WordPress post created: ID={}, Title=\"{}\"", 
    post.id, 
    post.title.rendered
);
```

### Health Checks
- WordPress connectivity verification
- API endpoint availability testing
- Configuration validation

### Metrics (Planned v0.3.0)
- Request/response time tracking
- Error rate monitoring
- Resource utilization metrics

## Roadmap Implementation

### Phase 1: Core Stability (v0.1.0-alpha) ✅
- WordPress handler completion
- Configuration system maturity  
- Error handling standardization
- Comprehensive testing

### Phase 2: Transport Expansion (v0.2.0)
- Stdio transport implementation
- Plugin dynamic loading
- Enhanced configuration management

### Phase 3: Ecosystem Growth (v0.3.0)
- WebSocket transport support
- GitHub API handler
- Database integration handler
- Performance monitoring

### Phase 4: Production Readiness (v1.0.0)
- Security audit and hardening
- Comprehensive documentation
- Container support
- Plugin ecosystem

## Design Principles

1. **Type Safety**: Leverage Rust's type system for correctness
2. **Performance**: Async-first with efficient resource usage
3. **Extensibility**: Plugin architecture for easy feature addition
4. **Reliability**: Comprehensive error handling and retry logic
5. **Maintainability**: Clear separation of concerns and documentation
6. **AI-Agent Optimized**: Designed specifically for AI interaction patterns

## Contributing Guidelines

### Code Organization
- Place handlers in `src/handlers/`
- Follow the `McpHandler` trait pattern
- Use structured error types
- Include comprehensive tests

### Configuration
- Support TOML configuration
- Provide environment variable overrides
- Include sensible defaults
- Document all options

### Testing
- Write integration tests for new handlers
- Include error path testing
- Provide example usage
- Test with real APIs when possible

### Documentation
- Update README.md for new features
- Include code examples
- Document configuration options
- Maintain architecture documentation