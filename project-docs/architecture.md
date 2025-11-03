# mcp-rs Architecture

## Overview

`mcp-rs` follows a 6-layer security-first architecture designed for enterprise production scalability, maintainability, and extensibility. The system is built around the Model Context Protocol (MCP) JSON-RPC specification with comprehensive security integration and AI agent protection.

## Core Architecture Principles

### 1. Security-First Design
- Zero-trust input validation
- Multi-layer defense systems
- Physical process isolation for plugins
- Comprehensive audit logging

### 2. Plugin Security Architecture

#### Current Risk Assessment
The traditional plugin architecture presents significant security vulnerabilities:

```
┌─────────────────────────────────────┐
│ Traditional Design (Vulnerable)      │
├─────────────────────────────────────┤
│ ┌─ MCP-RS Core ─────────────────┐   │
│ │ ┌─ Plugin A ─┐ ┌─ Plugin B ─┐ │   │
│ │ │ 悪意コード  │ │ 正常コード  │ │   │
│ │ │ ↓直接実行  │ │           │ │   │
│ │ └───────────┘ └───────────┘ │   │
│ │ ← セキュリティ境界なし →      │   │
│ └─────────────────────────────────┘   │
└─────────────────────────────────────┘
```

**Risks:**
- **Memory Contamination**: Plugins can access core memory space
- **Privilege Escalation**: Plugins run with same privileges as core
- **Data Leakage**: Access to other plugins' and core's sensitive data
- **System Destruction**: Potential to crash entire system

#### Proposed: Physical Isolation Security Architecture

```
┌─────────────────────────────────────────────────────────┐
│ Physical Security Boundaries                            │
├─────────────────────────────────────────────────────────┤
│                                                         │
│ ┌─ MCP Core Server (Protected) ─┐   Network Boundary   │
│ │ - Authentication & Authorization│       │              │
│ │ - Security Validation         │        │              │
│ │ - Rate Limiting               │        │              │
│ │ - Audit Logging               │        ▼              │
│ │ - Request Distribution        │ ┌─ Plugin Servers ─┐  │
│ └─────────────────────────────┘ │ │ ┌─ Plugin A ─┐  │  │
│           ▲                       │ │ │ Isolated   │  │  │
│           │ gRPC/HTTP API         │ │ └───────────┘  │  │
│           │                       │ │ ┌─ Plugin B ─┐  │  │
│ ┌─ API Gateway ─────────────────┐ │ │ │ Isolated   │  │  │
│ │ - TLS Termination             │ │ │ └───────────┘  │  │
│ │ - Token Validation            │ │ └─────────────────┘  │
│ │ - Request Routing             │ └─────────────────────┘  │
│ └─────────────────────────────┘                          │
│           ▲                                              │
│           │ HTTPS                                        │
│           │                                              │
│ ┌─ Client Applications ─────────┐                        │
│ │ - Claude, ChatGPT etc         │                        │
│ │ - Custom Clients              │                        │
│ └─────────────────────────────┘                        │
└─────────────────────────────────────────────────────────┘
```

## 6-Layer Security Architecture

### 1. Application Layer (`src/main.rs`) + Security Layer
- **Responsibility**: Entry point, configuration loading, server lifecycle, security initialization
- **Components**: Main server startup, signal handling, graceful shutdown, security context
- **Security Features**: Secure configuration loading, memory protection, startup verification
- **Dependencies**: Configuration, Server layers, Security modules

### 2. API Layer (`src/server.rs`) + Rate Limiting Layer
- **Responsibility**: HTTP server, JSON-RPC protocol, DDoS protection
- **Components**: Axum HTTP server, JSON-RPC routing, Token Bucket rate limiting
- **Security Features**: 
  - Token Bucket rate limiting (configurable requests/second)
  - DDoS attack protection
  - Request source validation
  - Automated IP blocking
- **Dependencies**: Handler layer, Rate limiter
- **Performance**: Timeout handling, error serialization, security logging

### 3. Service Layer (`src/handlers/`) + Input Validation Layer
- **Responsibility**: Business logic, external API integration, zero-trust validation
- **Components**: WordPress handler, security validators, input sanitizers
- **Security Features**:
  - Zero-trust input validation
  - SQL injection protection (11 attack patterns)
  - XSS attack protection (14 attack patterns) 
  - HTML sanitization with CSP headers
  - Parameter validation with type checking
- **Dependencies**: MCP protocol types, Security modules
- **Pattern**: Security-first plugin architecture with `McpHandler` trait

### 4. Core Layer (`src/core/`) + TLS/Encryption Layer
- **Responsibility**: Runtime management, execution context, cryptographic operations
- **Components**: 
  - `runtime.rs`: Secure application lifecycle and state management
  - `registry.rs`: Verified handler and plugin registration system
  - `context.rs`: Encrypted request-scoped execution context
  - `encryption.rs`: AES-GCM-256 encryption with PBKDF2 key derivation
- **Security Features**:
  - TLS 1.2+ enforcement with strong cipher suites
  - AES-GCM-256 encryption for sensitive data
  - PBKDF2 key derivation (100,000 iterations)
  - Memory protection and secure erasure
- **Dependencies**: Transport layer, Encryption modules
- **Performance**: Async secure processing, encrypted metrics, protected shutdown

### 5. Transport Layer (`src/transport/`) + Network Security Layer
- **Responsibility**: Secure communication protocols, network protection
- **Components**:
  - `mod.rs`: Secure transport traits and factory
  - `stdio.rs`: Protected standard I/O transport
  - `tls.rs`: TLS enforcement and certificate validation
- **Security Features**:
  - TLS certificate validation
  - Secure connection establishment
  - Network-level attack detection
  - Connection integrity monitoring
- **Dependencies**: Core types, TLS configuration
- **Protocols**: Secured stdio, HTTPS, WSS (WebSocket Secure)

### 6. MCP Protocol Layer (`src/mcp/`) + Audit Layer
- **Responsibility**: MCP protocol compliance, comprehensive security auditing
- **Components**: Protocol types, audit logging, compliance reporting
- **Security Features**:
  - Comprehensive audit trail (IP, timestamp, user agent)
  - Real-time security event logging
  - Compliance reporting (GDPR, SOC 2, ISO 27001)
  - Tamper-resistant log integrity
  - Security metrics collection
- **Standards**: MCP JSON-RPC 2.0 + Enterprise Security Extensions
- **Compliance**: GDPR, SOC 2 Type II, ISO 27001, PCI DSS ready

### 7. Infrastructure Layer (`src/config.rs`, `src/error.rs`) + Security Configuration
- **Responsibility**: Secure configuration management, error handling, security logging
- **Components**: TOML configuration, environment variables, structured security errors
- **Security Features**:
  - Encrypted configuration storage
  - Secure environment variable handling
  - Security policy enforcement
  - Compliance configuration management
- **Dependencies**: External crates (config, thiserror, tracing)

## Plugin System Architecture

### Dynamic Plugin System

MCP-RS provides a powerful dynamic plugin system that allows extending functionality at runtime with comprehensive security isolation.

#### Architecture Overview

```
┌─────────────────────┐    ┌─────────────────────┐
│   Plugin Registry   │────│   Plugin Loader     │
│                     │    │                     │
│ - Discovery         │    │ - Dynamic Loading   │
│ - Lifecycle Mgmt    │    │ - Search Paths      │
│ - Dependency Res.   │    │ - Safety Checks     │
└─────────────────────┘    └─────────────────────┘
           │                           │
           └───────────┬───────────────┘
                       │
          ┌─────────────────────┐
          │   Handler Registry  │
          │                     │
          │ - Handler Management│
          │ - Integration       │
          └─────────────────────┘
```

#### Security Isolation Implementation

```rust
/// Isolated Plugin Server (Physical Separation)
pub struct IsolatedPluginServer {
    /// Plugin implementation
    plugin: Box<dyn IsolatedPlugin>,
    /// Security sandbox
    sandbox: SecuritySandbox,
    /// Resource limits
    resource_limits: ResourceLimits,
}

pub trait IsolatedPlugin: Send + Sync {
    /// Initialize in isolated environment
    async fn initialize_isolated(&self, config: SandboxConfig) -> Result<(), PluginError>;
    
    /// Execute tool in sandbox
    async fn execute_tool_sandboxed(
        &self,
        tool_name: &str,
        parameters: SanitizedParameters,
    ) -> Result<SanitizedResponse, PluginError>;
}

#[derive(Debug)]
pub struct SecuritySandbox {
    /// Memory limit (MB)
    pub max_memory_mb: u64,
    /// CPU limit (%)
    pub max_cpu_percent: u8,
    /// Network access restrictions
    pub network_restrictions: NetworkPolicy,
    /// Filesystem access restrictions
    pub filesystem_restrictions: FilesystemPolicy,
    /// Execution timeout (seconds)
    pub execution_timeout_seconds: u64,
}
```

#### Plugin Communication Security

##### mTLS + JWT Authentication
```rust
/// Plugin inter-communication authentication system
pub struct PluginAuthSystem {
    /// Core server certificate (CA)
    core_ca_cert: X509Certificate,
    /// Plugin certificate validator
    plugin_cert_validator: CertificateValidator,
    /// JWT token manager
    jwt_manager: JwtTokenManager,
}

impl PluginAuthSystem {
    /// Establish secure connection with plugin server
    pub async fn establish_secure_connection(
        &self,
        plugin_endpoint: &PluginEndpoint,
    ) -> Result<SecureConnection, AuthError> {
        // 1. Establish mTLS connection
        let tls_connection = self.establish_mtls_connection(plugin_endpoint).await?;
        
        // 2. JWT token validation
        let jwt_token = self.jwt_manager.create_plugin_token(
            &plugin_endpoint.plugin_id,
            plugin_endpoint.allowed_operations.clone(),
        ).await?;
        
        Ok(SecureConnection {
            tls_connection,
            jwt_token,
            plugin_endpoint: plugin_endpoint.clone(),
        })
    }
}
```

### Core Plugin APIs

- **Plugin Discovery**: Basic plugin discovery from directories
- **Plugin Registry Usage**: Complete plugin registry lifecycle
- **Plugin Loader**: Dynamic plugin loading fundamentals
- **Search Path Management**: Managing plugin search paths

### Configuration Examples

- **Basic Configuration**: Default MCP configuration setup
- **Plugin Configuration**: Advanced plugin configuration

### Running Plugin Examples

All plugin code examples are executable and tested:

```bash
# Test all plugin documentation examples
cargo test --doc

# Test specific plugin module examples
cargo test --doc plugins

# Run plugin integration tests
cargo test plugin_integration_tests
```

## Security Integration Architecture

### Enterprise Security Stack
```rust
pub struct SecurityStack {
    // Layer 1: Encryption (AES-GCM-256 + PBKDF2)
    encryption: EncryptionManager,
    
    // Layer 2: Rate Limiting (Token Bucket + DDoS)
    rate_limiter: RateLimiter,
    
    // Layer 3: TLS/SSL (Certificate validation)
    tls_manager: TlsManager,
    
    // Layer 4: Input Protection (SQL + XSS)
    input_validator: InputValidator,
    sql_protector: SqlInjectionProtector,
    xss_protector: XssProtector,
    
    // Layer 5: Monitoring (Real-time analysis)
    threat_monitor: ThreatMonitor,
    
    // Layer 6: Audit Logging (Compliance)
    audit_logger: AuditLogger,
}
```

## Handler Plugin Architecture

### Secure McpHandler Trait
```rust
#[async_trait]
pub trait McpHandler: Send + Sync {
    async fn initialize(&self, params: InitializeParams) -> Result<serde_json::Value, McpError>;
    async fn list_tools(&self) -> Result<Vec<Tool>, McpError>;
    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError>;
    async fn list_resources(&self) -> Result<Vec<Resource>, McpError>;
    async fn read_resource(&self, params: ResourceReadParams) -> Result<serde_json::Value, McpError>;
    
    // Security Extensions
    async fn validate_security(&self, request: &Request) -> Result<SecurityValidation, McpError>;
    async fn apply_rate_limit(&self, client_id: &str) -> Result<(), McpError>;
    async fn audit_operation(&self, operation: &str, result: &str) -> Result<(), McpError>;
}
```

### WordPress Handler Implementation

The WordPress handler (`src/handlers/wordpress.rs`) provides 27 comprehensive tools with full security integration:

#### Content Management (10 tools) - 🛡️ Secured
- Complete CRUD operations for posts and pages with XSS protection
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

## Transport Architecture

### Transport Abstraction
```rust
pub trait Transport: Send + Sync {
    async fn send(&mut self, message: &str) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<String, TransportError>;
    fn info(&self) -> TransportInfo;
}
```

### Transport Factory
- **Pluggable Design**: Easy addition of new transport types
- **Configuration-Driven**: Transport selection based on configuration
- **Error Handling**: Uniform error handling across transports

### Stdio Transport Implementation
```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   MCP Client    │────▶│ Stdio Transport │────▶│   MCP Server    │
│    (Editor)     │     │   (Process)     │     │   (mcp-rs)      │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

#### Message Framing
1. **Content-Length Based**: HTTP-style headers with JSON payload
2. **Line-Based**: Simple newline-delimited JSON (fallback)

#### Flow Control
- **Async I/O**: Non-blocking stdin/stdout operations
- **Buffering**: Message buffering for performance
- **Timeout Handling**: Configurable timeouts for operations

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
    
    #[error("Transport error: {0}")]
    Transport(String),
}
```

### Error Flow
1. **Transport Level**: Connection and communication errors
2. **Handler Level**: Specific errors (HTTP, validation, business logic)
3. **MCP Level**: Protocol-specific error transformation
4. **JSON-RPC Level**: Standard JSON-RPC error response formatting

## Performance Architecture

### Async-First Design
- Built on Tokio runtime for high concurrency
- All I/O operations are non-blocking
- Transport-agnostic message processing
- Connection pooling for external APIs
- Request timeout handling

### Transport Performance
- **Stdio**: Direct process communication, minimal overhead
- **HTTP**: Connection reuse, Keep-Alive support
- **WebSocket** (planned): Persistent connections, low latency

### Message Processing
- **Concurrent Handling**: Multiple requests processed simultaneously
- **Streaming Support**: Large payload handling with streaming
- **Buffering**: Efficient message buffering for high throughput

### Retry Logic
```rust
// Exponential backoff with jitter
let delay = Duration::from_millis(base_delay_ms * 2_u64.pow(attempt) + random_jitter);
```

### Resource Management
- Automatic connection cleanup
- Transport lifecycle management
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

### Phase 2: Transport Expansion (v0.2.0-alpha) ✅
- ✅ Stdio transport implementation
- ✅ Core Runtime Module
- ✅ Transport abstraction layer
- ✅ Enhanced configuration management
- 🔄 Plugin dynamic loading (in progress)

### Phase 3: Ecosystem Growth (v0.3.0) 🎯
- WebSocket transport support
- HTTP transport enhancements
- GitHub API handler
- Database integration handler
- Performance monitoring and metrics

### Phase 4: Production Readiness (v1.0.0)
- Security audit and hardening
- Comprehensive documentation
- Container support
- Plugin ecosystem
- Production deployment guides

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