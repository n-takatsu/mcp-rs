# Real-time Editing Development Guide

## Overview

This guide provides comprehensive information for developers working on the MCP-RS real-time collaborative editing system. It covers architecture, development workflows, testing strategies, and deployment procedures.

## Quick Start

## Prerequisites

- **Rust**: 1.70+ (2021 edition)
- **Cargo**: Latest stable version
- **Git**: For version control
- **IDE**: VS Code with rust-analyzer recommended

## Development Setup

```bash

## Clone the repository

git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs

## Install dependencies and build

cargo build

## Run tests to verify setup

cargo test

## Start the real-time editing demo

cargo run --example axum_websocket_server

## Open browser to http://localhost:3000/

```

## Project Structure

```
mcp-rs/
├── src/
│   ├── session/                    

## Session management system

│   │   ├── manager.rs             

## SessionManager - central orchestrator

│   │   ├── storage.rs             

## Storage abstractions and implementations

│   │   ├── types.rs               

## Core session types and enums

│   │   ├── websocket_handler.rs   

## WebSocket connection handling

│   │   ├── middleware_basic.rs    

## HTTP session middleware

│   │   └── security_integration_basic.rs 

## Security layer

│   ├── server/                     

## Server implementations

│   ├── handlers/                   

## MCP protocol handlers

│   └── lib.rs                     

## Main library interface

├── examples/
│   ├── axum_websocket_server.rs   

## Real-time editing WebSocket server

│   ├── basic_session_usage.rs     

## Session management examples

│   └── websocket_realtime_editing.rs 

## WebSocket demo

├── tests/                          

## Test suites

├── docs/                          

## Technical documentation

├── static/                        

## Static web assets

└── demo-policies/                 

## Security policy configurations

```

## Architecture Overview

## Real-time Editing System

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Web Client    │    │  Axum WebSocket │    │ Session Manager │
│                 │◄──►│     Server      │◄──►│                 │
│  JavaScript     │    │                 │    │  CRUD Ops      │
│  WebSocket API  │    │  Connection Mgmt│    │  State Mgmt     │
│  Demo UI        │    │  Authentication │    │  Filtering      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                │                        │
                                ▼                        ▼
                       ┌─────────────────┐    ┌─────────────────┐
                       │ Security Middle │    │ Memory Storage  │
                       │                 │    │                 │
                       │ Input Validation│    │ HashMap Based   │
                       │ Session Verify  │    │ Thread Safe     │
                       │ Event Logging   │    │ Concurrent      │
                       └─────────────────┘    └─────────────────┘
```

## Core Components

1. **Session Management**: Enterprise-grade session lifecycle management
2. **WebSocket Server**: Real-time communication with connection handling
3. **Security Layer**: Multi-layer security with input validation
4. **Storage Layer**: Pluggable storage with memory implementation
5. **API Layer**: RESTful session management API

## Development Workflow

## Feature Development

1. **Branch Creation**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Development Cycle**
   ```bash
   

## Make changes

   

## Run tests continuously

   cargo watch -x test
   
   

## Check code quality

   cargo clippy
   cargo fmt
   
   

## Run specific test suites

   cargo test session
   cargo test websocket
   ```

3. **Quality Gates**
   ```bash
   

## All tests must pass

   cargo test --all
   
   

## No clippy warnings

   cargo clippy -- -D warnings
   
   

## Code formatting

   cargo fmt --check
   
   

## Security audit

   cargo audit
   ```

## Code Organization

### Session Management Module

**Core Types** (`src/session/types.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub state: SessionState,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Pending,     // Created but not activated
    Active,      // Ready for WebSocket connections
    Suspended,   // Temporarily disabled
    Expired,     // TTL exceeded
    Invalidated, // Manually invalidated
}
```

**Manager Layer** (`src/session/manager.rs`):
```rust
impl SessionManager {
    // Core CRUD operations
    pub async fn create_session(&self, user_id: String) -> Result<Session, SessionError>;
    pub async fn get_session(&self, id: &SessionId) -> Result<Option<Session>, SessionError>;
    pub async fn activate_session(&self, id: &SessionId) -> Result<Option<Session>, SessionError>;
    pub async fn delete_session(&self, id: &SessionId) -> Result<bool, SessionError>;
    
    // Advanced operations
    pub async fn list_sessions(&self, filter: &SessionFilter) -> Result<Vec<Session>, SessionError>;
}
```

**Storage Layer** (`src/session/storage.rs`):
```rust
#[async_trait]
pub trait SessionStorage: Send + Sync + std::fmt::Debug {
    async fn create(&self, session: Session) -> Result<Session, SessionError>;
    async fn get(&self, id: &SessionId) -> Result<Option<Session>, SessionError>;
    async fn update(&self, session: Session) -> Result<Session, SessionError>;
    async fn delete(&self, id: &SessionId) -> Result<bool, SessionError>;
    async fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, SessionError>;
}
```

### WebSocket Integration

**WebSocket Handler** (`src/session/websocket_handler.rs`):
```rust
impl SessionWebSocketHandler {
    pub async fn handle_websocket_connection(
        &self,
        ws: WebSocketUpgrade,
        headers: HeaderMap,
        addr: Option<SocketAddr>,
    ) -> Result<Response, Response>;
    
    async fn process_message(
        &self,
        message: &str,
        session_id: Option<&SessionId>,
    ) -> Result<WebSocketMessageProtocol, SessionError>;
}
```

**Message Protocol**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessageProtocol {
    pub id: String,
    pub message_type: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}
```

## Testing Strategy

### Test Organization

```
tests/
├── session_current_tests.rs        

## Core session functionality

├── basic_session_tests.rs          

## Basic CRUD operations  

├── integration_tests.rs            

## Integration test suite

└── mod.rs                          

## Test module organization

```

### Test Categories

**Unit Tests**: Test individual components
```rust
#[tokio::test]
async fn test_session_creation() {
    let manager = SessionManager::new();
    let session = manager.create_session("test_user".to_string()).await.unwrap();
    
    assert_eq!(session.user_id, "test_user");
    assert_eq!(session.state, SessionState::Pending);
}
```

**Integration Tests**: Test component interaction
```rust
#[tokio::test]
async fn test_websocket_session_flow() {
    // Create session
    let manager = SessionManager::new();
    let session = manager.create_session("user1".to_string()).await.unwrap();
    
    // Activate session
    let activated = manager.activate_session(&session.id).await.unwrap().unwrap();
    assert_eq!(activated.state, SessionState::Active);
    
    // Test WebSocket connection (would use actual WebSocket client)
    // ...
}
```

**Performance Tests**: Validate performance characteristics
```rust
#[tokio::test]
async fn test_concurrent_session_operations() {
    let manager = Arc::new(SessionManager::new());
    let mut handles = vec![];
    
    // Create 100 concurrent sessions
    for i in 0..100 {
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            manager_clone.create_session(format!("user_{}", i)).await
        });
        handles.push(handle);
    }
    
    // All should succeed
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }
}
```

## Error Handling

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(String),
    
    #[error("Invalid session state: {0}")]
    InvalidState(String),
    
    #[error("Session expired: {0}")]
    Expired(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Security violation: {0}")]
    SecurityViolation(String),
}
```

### Error Handling Patterns

```rust
// Graceful error handling with detailed logging
match session_manager.get_session(&session_id).await {
    Ok(Some(session)) => {
        debug!("Session found: {}", session.id.as_str());
        // Continue processing
    }
    Ok(None) => {
        warn!("Session not found: {}", session_id.as_str());
        return Err(SessionError::NotFound(session_id.as_str().to_string()));
    }
    Err(e) => {
        error!("Session retrieval failed: {}", e);
        return Err(e);
    }
}
```

## Performance Guidelines

### Async/Await Best Practices

```rust
// Good: Use async/await for I/O operations
pub async fn create_session(&self, user_id: String) -> Result<Session, SessionError> {
    let session = Session::new(user_id);
    self.storage.create(session).await  // Async storage operation
}

// Good: Concurrent operations where possible
pub async fn get_multiple_sessions(&self, ids: Vec<SessionId>) -> Vec<Option<Session>> {
    let futures: Vec<_> = ids.iter().map(|id| self.get_session(id)).collect();
    futures::future::join_all(futures).await
}
```

### Memory Management

```rust
// Use Arc for shared ownership
let manager = Arc::new(SessionManager::new());

// Use channels for communication between tasks
let (tx, rx) = tokio::sync::mpsc::channel(100);

// Efficient string handling
let session_id = SessionId::new();  // Uses UUID internally
let user_id = user_id.into();       // Take ownership when possible
```

## Security Guidelines

### Input Validation

```rust
pub async fn validate_input(&self, input: &str, context: &str) -> Result<bool, SessionError> {
    // Length validation
    if input.len() > 10000 {
        return Ok(false);
    }
    
    // XSS pattern detection
    let dangerous_patterns = ["<script", "javascript:", "data:", "vbscript:", "on"];
    for pattern in &dangerous_patterns {
        if input.to_lowercase().contains(pattern) {
            return Ok(false);
        }
    }
    
    Ok(true)
}
```

### Session Security

```rust
// Always validate session state for WebSocket connections
pub async fn validate_websocket_session(&self, session_id: &SessionId) -> Result<bool, SessionError> {
    match self.session_manager.get_session(session_id).await? {
        Some(session) => Ok(session.state == SessionState::Active),
        None => Ok(false),
    }
}
```

## Configuration Management

### Development Configuration

```toml

## config/development.toml

[session]
ttl_hours = 1           

## Short TTL for development

max_per_user = 10       

## More sessions for testing

[security]
enable_rate_limiting = false  

## Disabled for development

verbose_errors = true         

## Detailed error messages

[logging]
level = "debug"
enable_file_logging = false
```

### Production Configuration

```toml

## config/production.toml  

[session]
ttl_hours = 24          

## Full 24-hour sessions

max_per_user = 5        

## Limited sessions per user

[security]
enable_rate_limiting = true   

## Full rate limiting

verbose_errors = false        

## Minimal error exposure

require_tls = true           

## Force HTTPS/WSS

[logging] 
level = "info"
enable_file_logging = true
```

## Deployment

### Docker Development

```dockerfile
FROM rust:1.70

WORKDIR /app
COPY . .

RUN cargo build --release

EXPOSE 3000

CMD ["cargo", "run", "--release", "--example", "axum_websocket_server"]
```

### Docker Compose

```yaml
version: '3.8'
services:
  mcp-rs:
    build: .
    ports:
      - "3000:3000"
    environment:
      - RUST_LOG=info
      - CONFIG_FILE=/app/config/production.toml
    volumes:
      - ./config:/app/config
    restart: unless-stopped
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mcp-rs-realtime-editing
spec:
  replicas: 3
  selector:
    matchLabels:
      app: mcp-rs
  template:
    metadata:
      labels:
        app: mcp-rs
    spec:
      containers:
      - name: mcp-rs
        image: mcp-rs:v0.15.0-realtime-editing
        ports:
        - containerPort: 3000
        env:
        - name: RUST_LOG
          value: "info"
        resources:
          requests:
            memory: "256Mi"
            cpu: "100m"
          limits:
            memory: "512Mi" 
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 3000
          initialDelaySeconds: 5
          periodSeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: mcp-rs-service
spec:
  selector:
    app: mcp-rs
  ports:
  - protocol: TCP
    port: 80
    targetPort: 3000
  type: LoadBalancer
```

## Monitoring and Observability

### Structured Logging

```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(self))]
pub async fn create_session(&self, user_id: String) -> Result<Session, SessionError> {
    info!("Creating session for user: {}", user_id);
    
    let session = Session::new(user_id);
    match self.storage.create(session).await {
        Ok(session) => {
            info!("Session created successfully: {}", session.id.as_str());
            Ok(session)
        }
        Err(e) => {
            error!("Session creation failed: {}", e);
            Err(e)
        }
    }
}
```

### Metrics Collection

```rust
pub struct SessionMetrics {
    pub total_sessions: u64,
    pub active_sessions: u64,
    pub expired_sessions: u64,
    pub sessions_created_today: u64,
    pub average_session_duration: Duration,
    pub total_bytes_transferred: u64,
    pub calculated_at: DateTime<Utc>,
}

impl SessionManager {
    pub async fn get_metrics(&self) -> Result<SessionMetrics, SessionError> {
        // Calculate metrics from session storage
        // ...
    }
}
```

### Health Checks

```rust
pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "mcp-rs-realtime-editing",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    }))
}
```

## Contributing Guidelines

### Code Style

- **Formatting**: Use `cargo fmt` for consistent formatting
- **Linting**: Address all `cargo clippy` warnings
- **Documentation**: Document all public APIs with examples
- **Testing**: Maintain test coverage above 80%

### Pull Request Process

1. **Feature Branch**: Create feature branch from `main`
2. **Implementation**: Implement feature with tests
3. **Quality Check**: Pass all quality gates
4. **Documentation**: Update relevant documentation
5. **Review**: Submit PR for code review
6. **Merge**: Merge after approval and CI success

### Commit Messages

Use conventional commit format:
```
feat: add real-time collaborative editing support
fix: resolve session expiration edge case  
docs: update WebSocket API documentation
test: add comprehensive session lifecycle tests
```

## Troubleshooting

### Common Issues

**Compilation Errors**:
```bash

## Clean build artifacts

cargo clean

## Update dependencies

cargo update

## Check for conflicting versions

cargo tree --duplicates
```

**Test Failures**:
```bash

## Run specific failing test

cargo test test_name -- --nocapture

## Run with debug output

RUST_LOG=debug cargo test

## Check for test isolation issues

cargo test -- --test-threads=1
```

**Runtime Issues**:
```bash

## Enable debug logging

RUST_LOG=debug cargo run --example axum_websocket_server

## Check port conflicts

netstat -tlnp | grep 3000

## Verify configuration

cargo run -- --check-config
```

## Advanced Topics

### Custom Storage Implementation

```rust
use async_trait::async_trait;

#[derive(Debug)]
pub struct RedisSessionStorage {
    client: redis::Client,
}

#[async_trait]
impl SessionStorage for RedisSessionStorage {
    async fn create(&self, session: Session) -> Result<Session, SessionError> {
        let mut conn = self.client.get_async_connection().await
            .map_err(|e| SessionError::Storage(e.to_string()))?;
        
        let serialized = serde_json::to_string(&session)
            .map_err(SessionError::Serialization)?;
        
        conn.set(session.id.as_str(), serialized).await
            .map_err(|e| SessionError::Storage(e.to_string()))?;
        
        Ok(session)
    }
    
    // Implement other trait methods...
}
```

### WebSocket Message Extensions

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSocketMessageType {
    Heartbeat,
    RealtimeEdit,
    CursorPosition,
    UserPresence,
    DocumentSync,
}

impl WebSocketHandler {
    pub async fn handle_cursor_position(&self, payload: serde_json::Value) -> Result<(), SessionError> {
        // Handle cursor position updates
        // Broadcast to other users in same document
    }
    
    pub async fn handle_user_presence(&self, payload: serde_json::Value) -> Result<(), SessionError> {
        // Handle user presence updates
        // Track who's currently editing
    }
}
```

This development guide provides comprehensive coverage of working with the MCP-RS real-time editing system. For additional details, refer to the technical documentation in the `/docs` directory.