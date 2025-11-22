# WebSocket API Documentation

## Overview

The MCP-RS WebSocket API provides real-time collaborative editing capabilities through session-based WebSocket connections. This document covers the complete API surface, message protocols, and integration examples.

## Architecture

## System Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Web Client    │    │  Axum WebSocket │    │ Session Manager │
│                 │◄──►│     Server      │◄──►│                 │
│  - Demo UI      │    │                 │    │  - CRUD Ops     │
│  - JavaScript   │    │  - Connection   │    │  - State Mgmt   │
│  - WebSocket    │    │  - Auth         │    │  - Filtering    │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                │                        │
                                ▼                        ▼
                       ┌─────────────────┐    ┌─────────────────┐
                       │ Security Middle │    │ Memory Storage  │
                       │                 │    │                 │
                       │  - Input Valid. │    │  - HashMap      │
                       │  - Session Ver. │    │  - Concurrent   │
                       │  - Event Log    │    │  - Thread Safe  │
                       └─────────────────┘    └─────────────────┘
```

## Session Lifecycle

```
   create_session()
        │
        ▼
┌─────────────┐    activate_session()    ┌─────────────┐
│   Pending   │─────────────────────────►│   Active    │
└─────────────┘                         └─────────────┘
        │                                        │
        │            WebSocket                   │
        │           Connection                   │
        │                                       │
        ▼                                       ▼
┌─────────────┐                         ┌─────────────┐
│   Expired   │                         │ Invalidated │
└─────────────┘                         └─────────────┘
```

## REST API Endpoints

## Base URL

```
http://localhost:3000
```

## Session Management

### Create Session

**POST** `/api/sessions`

```json
{
  "user_id": "string",
  "client_info": "string (optional)"
}
```

**Response:**
```json
{
  "session_id": "uuid-string",
  "state": "Active",
  "websocket_url": "ws://localhost:3000/ws?session_id={session_id}"
}
```

### Get Session Info

**GET** `/api/sessions/{session_id}`

**Response:**
```json
{
  "session_id": "uuid-string",
  "user_id": "string",
  "state": "Active",
  "created_at": "2025-11-07T10:00:00Z",
  "expires_at": "2025-11-08T10:00:00Z"
}
```

### Activate Session

**POST** `/api/sessions/{session_id}/activate`

**Response:**
```json
{
  "session_id": "uuid-string",
  "user_id": "string", 
  "state": "Active",
  "created_at": "2025-11-07T10:00:00Z",
  "expires_at": "2025-11-08T10:00:00Z"
}
```

## Health Check

**GET** `/health`

**Response:**
```json
{
  "status": "healthy",
  "service": "mcp-rs-realtime-editing",
  "timestamp": "2025-11-07T10:00:00Z",
  "version": "0.15.0"
}
```

## WebSocket API

## Connection

**WebSocket URL:**
```
ws://localhost:3000/ws?session_id={session_id}
```

**Headers:**
```
Authorization: Bearer {session_id}
x-session-id: {session_id}
```

## Message Protocol

All WebSocket messages follow this JSON structure:

```json
{
  "id": "unique-message-id",
  "type": "message_type",
  "payload": {
    // Type-specific data
  },
  "timestamp": "2025-11-07T10:00:00Z"
}
```

## Message Types

### 1. Heartbeat

**Client → Server:**
```json
{
  "id": "msg-001",
  "type": "heartbeat",
  "payload": {},
  "timestamp": "2025-11-07T10:00:00Z"
}
```

**Server → Client:**
```json
{
  "id": "msg-001",
  "type": "heartbeat_ack",
  "payload": {
    "server_time": "2025-11-07T10:00:00Z"
  },
  "timestamp": "2025-11-07T10:00:00Z"
}
```

### 2. Real-time Edit

**Client → Server:**
```json
{
  "id": "edit-001",
  "type": "realtime_edit",
  "payload": {
    "content": "Updated document content...",
    "cursor_position": 125,
    "user_id": "user1",
    "operation": "insert",
    "change_delta": {
      "position": 125,
      "insert": "new text"
    }
  },
  "timestamp": "2025-11-07T10:00:00Z"
}
```

**Server → Other Clients:**
```json
{
  "id": "edit-001-broadcast",
  "type": "realtime_edit",
  "payload": {
    "content": "Updated document content...",
    "cursor_position": 125,
    "user_id": "user1",
    "operation": "insert",
    "change_delta": {
      "position": 125,
      "insert": "new text"  
    }
  },
  "session_info": {
    "session_id": "session-uuid",
    "user_id": "user1",
    "timestamp": "2025-11-07T10:00:00Z"
  },
  "timestamp": "2025-11-07T10:00:00Z"
}
```

### 3. Session Events

**Session Connected:**
```json
{
  "id": "session-001",
  "type": "session_connected",
  "payload": {
    "session_id": "session-uuid",
    "user_id": "user1",
    "connection_count": 1
  },
  "timestamp": "2025-11-07T10:00:00Z"
}
```

**Session Disconnected:**
```json
{
  "id": "session-002", 
  "type": "session_disconnected",
  "payload": {
    "session_id": "session-uuid",
    "user_id": "user1",
    "reason": "client_disconnect"
  },
  "timestamp": "2025-11-07T10:00:00Z"
}
```

### 4. Error Messages

```json
{
  "id": "error-001",
  "type": "error",
  "payload": {
    "code": "INVALID_SESSION",
    "message": "Session not found or expired",
    "details": {
      "session_id": "invalid-session-id"
    }
  },
  "timestamp": "2025-11-07T10:00:00Z"
}
```

## Error Codes

| Code | Description | Recovery |
|------|-------------|----------|
| `INVALID_SESSION` | Session not found/expired | Create new session |
| `UNAUTHORIZED` | Invalid authentication | Re-authenticate |
| `RATE_LIMITED` | Too many messages | Slow down requests |
| `MESSAGE_TOO_LARGE` | Message exceeds size limit | Reduce message size |
| `INVALID_MESSAGE` | Malformed JSON/structure | Fix message format |
| `SERVER_ERROR` | Internal server error | Retry connection |

## Client Integration Examples

## JavaScript/Web

```javascript
class MCPWebSocketClient {
  constructor(sessionId) {
    this.sessionId = sessionId;
    this.ws = null;
    this.isConnected = false;
  }

  async connect() {
    const wsUrl = `ws://localhost:3000/ws?session_id=${this.sessionId}`;
    this.ws = new WebSocket(wsUrl);
    
    this.ws.onopen = () => {
      this.isConnected = true;
      console.log('Connected to MCP-RS WebSocket');
      this.startHeartbeat();
    };
    
    this.ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      this.handleMessage(message);
    };
    
    this.ws.onclose = () => {
      this.isConnected = false;
      console.log('Disconnected from MCP-RS WebSocket');
    };
  }

  sendEdit(content, cursorPosition) {
    if (!this.isConnected) return;
    
    const message = {
      id: crypto.randomUUID(),
      type: 'realtime_edit',
      payload: {
        content: content,
        cursor_position: cursorPosition,
        user_id: this.userId
      },
      timestamp: new Date().toISOString()
    };
    
    this.ws.send(JSON.stringify(message));
  }

  handleMessage(message) {
    switch (message.type) {
      case 'realtime_edit':
        this.onEdit(message.payload);
        break;
      case 'heartbeat_ack':
        console.log('Heartbeat acknowledged');
        break;
      case 'error':
        console.error('WebSocket error:', message.payload);
        break;
    }
  }

  startHeartbeat() {
    setInterval(() => {
      if (this.isConnected) {
        const heartbeat = {
          id: crypto.randomUUID(),
          type: 'heartbeat',
          payload: {},
          timestamp: new Date().toISOString()
        };
        this.ws.send(JSON.stringify(heartbeat));
      }
    }, 30000); // Every 30 seconds
  }

  onEdit(payload) {
    // Override this method to handle incoming edits
    console.log('Received edit:', payload);
  }
}
```

## Rust Client

```rust
use tokio_tungstenite::{connect_async, tungstenite::Message};
use serde_json::{json, Value};
use uuid::Uuid;

pub struct MCPWebSocketClient {
    session_id: String,
}

impl MCPWebSocketClient {
    pub fn new(session_id: String) -> Self {
        Self { session_id }
    }

    pub async fn connect(&self) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("ws://localhost:3000/ws?session_id={}", self.session_id);
        let (ws_stream, _) = connect_async(&url).await?;
        
        let (mut write, mut read) = ws_stream.split();
        
        // Handle messages
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Ok(message) = serde_json::from_str::<Value>(&text) {
                        println!("Received: {}", serde_json::to_string_pretty(&message)?);
                    }
                }
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });
        
        // Send heartbeat
        let heartbeat = json!({
            "id": Uuid::new_v4().to_string(),
            "type": "heartbeat",
            "payload": {},
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        
        write.send(Message::Text(heartbeat.to_string())).await?;
        
        Ok(())
    }
}
```

## Security Considerations

## Authentication

- **Session-based**: All WebSocket connections require valid session IDs
- **Header validation**: Multiple authentication methods supported
- **Automatic expiration**: Sessions expire after 24 hours by default

## Input Validation

- **Message size limits**: Maximum 10KB per WebSocket message  
- **JSON validation**: All messages must be valid JSON
- **XSS protection**: Content is sanitized for dangerous patterns
- **Rate limiting**: Per-session message rate limiting

## Security Headers

```
Authorization: Bearer {session_id}
X-Session-ID: {session_id}
X-Client-Type: web|mobile|desktop
```

## Performance Characteristics

## Benchmarks

- **Session Creation**: ~1ms average latency
- **WebSocket Connection**: ~5ms establishment time
- **Message Throughput**: 1000+ messages/second per connection
- **Memory Usage**: ~1MB per 1000 active sessions
- **CPU Usage**: <5% at 100 concurrent connections

## Scaling Recommendations

- **Development**: Single server, memory storage
- **Production**: Load balancer + Redis session store
- **Enterprise**: Distributed session management with database persistence

## Demo Application

## Running the Demo

```bash

## Start the WebSocket server

cargo run --example axum_websocket_server

## Open browser to http://localhost:3000/

```

## Demo Features

1. **Dual Editor Interface**: Side-by-side collaborative editing
2. **Session Management**: Create and manage user sessions
3. **Real-time Sync**: Instant synchronization across editors
4. **Connection Monitoring**: Live connection status and logs
5. **API Testing**: Built-in tools for testing REST endpoints

## Demo Architecture

```
┌─────────────────┐
│   Static HTML   │  ← http://localhost:3000/
│                 │  
│  ┌─────────────┐│
│  │  Editor 1   ││  ← User 1 WebSocket
│  └─────────────┘│
│  ┌─────────────┐│ 
│  │  Editor 2   ││  ← User 2 WebSocket
│  └─────────────┘│
│                 │
│  ┌─────────────┐│
│  │ API Panel   ││  ← REST API calls  
│  └─────────────┘│
└─────────────────┘
```

## Troubleshooting

## Common Issues

**Connection Refused:**
- Ensure server is running on port 3000
- Check firewall settings
- Verify session ID is valid

**Message Not Received:**
- Check WebSocket connection status
- Verify message format matches protocol
- Check browser console for JavaScript errors

**Session Expired:**
- Sessions expire after 24 hours
- Create new session via REST API
- Check server logs for expiration events

## Debug Mode

```bash

## Run with debug logging

RUST_LOG=debug cargo run --example axum_websocket_server
```

## Monitoring

- **Health Endpoint**: `GET /health`
- **Session Metrics**: Built into session manager  
- **WebSocket Stats**: Connection count and message throughput
- **Error Logging**: Comprehensive error tracking with tracing

## Future Enhancements

## Planned Features

- **Operational Transform**: Conflict resolution for simultaneous edits
- **Cursor Tracking**: Real-time cursor position sharing
- **Document Versions**: Version control for collaborative documents
- **User Presence**: Show who's currently editing
- **Redis Backend**: Distributed session storage
- **Authentication**: Integration with OAuth/JWT providers