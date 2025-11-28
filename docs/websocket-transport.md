# WebSocket Transport Implementation

## Overview

Complete implementation of WebSocket transport for mcp-rs with full bidirectional support, connection management, heartbeat, and comprehensive error handling.

## Features

### ✅ Implemented

- **Bidirectional Communication**: Full-duplex WebSocket communication
- **Server & Client Modes**: Can act as WebSocket server or client
- **Message Handling**:
  - JSON-RPC request/response serialization
  - Asynchronous message queues with backpressure
  - Binary and text message support
- **Connection Management**:
  - Automatic connection pooling
  - Connection state tracking
  - Graceful shutdown
- **Heartbeat**: Ping-Pong heartbeat mechanism (configurable interval)
- **Statistics**: Message count, byte count, connection uptime tracking
- **Error Handling**: Comprehensive error handling with proper error types

### 🔄 To Be Implemented (Phase 2)

- **TLS/WSS Support**: Secure WebSocket connections
- **Origin Validation**: Cross-origin request security
- **Authentication Integration**: Token-based authentication
- **Rate Limiting Integration**: Per-connection rate limiting
- **CSRF Protection**: CSRF token validation
- **Automatic Reconnection**: Client-side auto-reconnect with exponential backoff
- **Compression**: WebSocket permessage-deflate extension

## Configuration

```rust
use mcp_rs::transport::websocket::WebSocketConfig;

let config = WebSocketConfig {
    url: "ws://127.0.0.1:8082".to_string(),
    server_mode: true,              // true for server, false for client
    timeout_seconds: Some(30),      // Connection timeout
    use_tls: false,                 // Enable TLS/WSS (TODO)
    heartbeat_interval: 30,         // Heartbeat interval (0 to disable)
    max_reconnect_attempts: 5,      // Max reconnection attempts (TODO)
    reconnect_delay: 5,             // Reconnection delay in seconds (TODO)
    max_message_size: 16 * 1024 * 1024,  // 16MB max message size
    max_connections: 100,           // Max concurrent connections (server mode)
};
```

## Usage Example

### Server Mode

```rust
use mcp_rs::transport::websocket::{WebSocketConfig, WebSocketTransport};
use mcp_rs::transport::Transport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = WebSocketConfig {
        url: "ws://127.0.0.1:8082".to_string(),
        server_mode: true,
        ..Default::default()
    };

    let mut transport = WebSocketTransport::new(config)?;
    transport.start().await?;

    // Handle messages...

    transport.stop().await?;
    Ok(())
}
```

### Client Mode

```rust
let config = WebSocketConfig {
    url: "ws://127.0.0.1:8082".to_string(),
    server_mode: false,
    ..Default::default()
};

let mut transport = WebSocketTransport::new(config)?;
transport.start().await?;

// Send/receive messages...

transport.stop().await?;
```

## Demo

Run the WebSocket transport demo:

```bash
# Terminal 1 - Start server
cargo run --example websocket_transport_demo -- server

# Terminal 2 - Start client
cargo run --example websocket_transport_demo -- client
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│         WebSocketTransport                          │
├─────────────────────────────────────────────────────┤
│                                                     │
│  ┌──────────────┐  ┌──────────────┐               │
│  │ Connection   │  │ Message      │               │
│  │ Manager      │  │ Processor    │               │
│  └──────────────┘  └──────────────┘               │
│                                                     │
│  ┌──────────────┐  ┌──────────────┐               │
│  │ Incoming     │  │ Outgoing     │               │
│  │ Queue        │  │ Queue        │               │
│  └──────────────┘  └──────────────┘               │
│                                                     │
│  ┌──────────────────────────────────────────────┐  │
│  │         Heartbeat & Statistics              │  │
│  └──────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

## Performance

- **Throughput**: Designed for high-throughput scenarios
- **Latency**: Minimal latency with direct tokio integration
- **Memory**: Efficient with bounded message queues
- **Scalability**: Supports 100+ concurrent connections

## Testing

```bash
# Run all tests
cargo test websocket

# Run with logging
RUST_LOG=debug cargo test websocket

# Run clippy
cargo clippy --all-features -- -D warnings
```

## Roadmap

### Phase 1: Core Functionality ✅

- [x] WebSocket connection handling
- [x] Message send/receive
- [x] Server and client modes
- [x] Heartbeat mechanism
- [x] Connection statistics

### Phase 2: Security (Next)

- [ ] TLS/WSS support
- [ ] Origin validation
- [ ] Authentication integration
- [ ] Rate limiting

### Phase 3: Reliability

- [ ] Automatic reconnection
- [ ] Connection pooling (advanced)
- [ ] Load balancing
- [ ] Circuit breaker

### Phase 4: Optimization

- [ ] Compression support
- [ ] Message batching
- [ ] Zero-copy optimizations
- [ ] Performance benchmarks

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.

## License

MIT OR Apache-2.0
