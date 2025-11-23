# MCP-RS Demo Page

**Interactive Real-time Collaborative Editing Demo**

<div align="center">

![Demo Status](https://img.shields.io/badge/Demo-Live-brightgreen?style=for-the-badge&logo=play)
![Real-time](https://img.shields.io/badge/Real--time-Editing-blue?style=for-the-badge&logo=edit)

[🎮 **Launch Demo**](https://github.com/n-takatsu/mcp-rs/blob/main/static/demo.html) • [📖 **Documentation**](https://n-takatsu.github.io/mcp-rs/docs/) • [🔧 **Source Code**](https://github.com/n-takatsu/mcp-rs)

</div>

---

## 🎬 Experience Real-time Collaboration

Our interactive demo showcases the power of MCP-RS real-time collaborative editing system. See how multiple users can collaborate seamlessly with sub-millisecond synchronization.

## 🚀 Quick Demo Setup

**Option 1: Run Locally (Recommended)**

```bash

## Clone the repository

git clone https://github.com/your-org/mcp-rs.git
cd mcp-rs

## Start the server

cargo run --bin main

## Open the demo

open http://localhost:8080/demo.html

```

**Option 2: Try Online Demo**
- [**Live Demo →**](http://localhost:8080/demo.html) (Requires local server)

---

## 🎯 Demo Features

## 📝 Real-time Collaborative Editing

<div style="border: 1px solid #ddd; border-radius: 8px; padding: 20px; margin: 20px 0;">

**Dual Editor Experience**
- **Left Editor**: Primary collaborative text editor
- **Right Editor**: Secondary editor showing real-time synchronization
- **Live Updates**: Type in either editor and see instant updates in both

**What to Try:**
1. Open multiple browser tabs
2. Type different content in each tab
3. Watch real-time synchronization across all tabs
4. Experience sub-second latency and smooth editing

</div>

## 🔧 API Testing Tools

<div style="border: 1px solid #ddd; border-radius: 8px; padding: 20px; margin: 20px 0;">

**Built-in REST API Tester**
- **Session Management**: Create, read, update, delete sessions
- **Real-time Testing**: Test WebSocket connections live
- **Response Viewer**: See formatted JSON responses
- **Error Handling**: Test error conditions and edge cases

**Available API Endpoints:**
- `POST /api/sessions` - Create new session
- `GET /api/sessions` - List all sessions
- `GET /api/sessions/{id}` - Get specific session
- `PUT /api/sessions/{id}` - Update session
- `DELETE /api/sessions/{id}` - Delete session
- `GET /api/health` - System health check

</div>

## 📊 Live Monitoring Dashboard

<div style="border: 1px solid #ddd; border-radius: 8px; padding: 20px; margin: 20px 0;">

**Real-time Metrics**
- **Connection Status**: Live WebSocket connection monitoring
- **Message Statistics**: Real-time message throughput and latency
- **Session Information**: Active sessions and user counts
- **Performance Metrics**: Memory usage and system health

**Connection Logs**
- WebSocket connection events
- Message send/receive logs
- Error and warning messages
- Performance timing data

</div>

---

## 📱 Demo Interface Guide

## Main Interface Components

```

┌─────────────────────────────────────────────────────────────────┐
│                    MCP-RS Real-time Demo                        │
├─────────────────────────────────────────────────────────────────┤
│ Session Controls: [Create Session] [Connect] [Disconnect]       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐    ┌─────────────────┐                    │
│  │ Editor 1        │    │ Editor 2        │                    │
│  │                 │    │                 │                    │
│  │ [Real-time      │    │ [Synchronized   │                    │
│  │  collaborative  │◄──►│  content        │                    │
│  │  editing area]  │    │  editing area]  │                    │
│  │                 │    │                 │                    │
│  └─────────────────┘    └─────────────────┘                    │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐    ┌─────────────────┐                    │
│  │ API Test Panel  │    │ Connection Log  │                    │
│  │                 │    │                 │                    │
│  │ [REST API       │    │ [Real-time      │                    │
│  │  endpoint       │    │  connection     │                    │
│  │  testing tools] │    │  monitoring]    │                    │
│  │                 │    │                 │                    │
│  └─────────────────┘    └─────────────────┘                    │
└─────────────────────────────────────────────────────────────────┘

```

## Step-by-Step Demo Instructions

### 🎯 1. Basic Collaboration Test

1. **Start the Server**

   ```bash

   ```

2. **Open the Demo**
   - Navigate to `http://localhost:8080/demo.html`
   - Click "Create Session" to create a new collaborative session

3. **Test Real-time Editing**
   - Type in the left editor
   - See instant updates in the right editor
   - Experience sub-millisecond synchronization

### 🎯 2. Multi-User Collaboration

1. **Open Multiple Tabs**
   - Open 2-3 browser tabs with the same demo page
   - Use the same session ID in all tabs

2. **Collaborative Editing**
   - Type different content in each tab
   - Watch real-time updates across all tabs
   - Test simultaneous editing and conflict handling

### 🎯 3. API Testing

1. **Create Sessions**
   - Use the API test panel to create new sessions
   - Test different session configurations
   - View JSON responses in real-time

2. **Session Management**
   - List all active sessions
   - Update session metadata
   - Delete unused sessions

### 🎯 4. Performance Monitoring

1. **Connection Monitoring**
   - Watch connection status in the log panel
   - Monitor WebSocket message flow
   - Observe latency and throughput metrics

2. **Load Testing**
   - Open many tabs simultaneously
   - Test high-frequency message sending
   - Monitor system performance under load

---

## 🎮 Interactive Demo Scenarios

## Scenario 1: Document Collaboration

**Simulate Google Docs-style collaboration:**

1. Create a session for document editing
2. Open multiple tabs (simulate multiple users)
3. Type a document collaboratively
4. Observe real-time cursor positions and edits
5. Test conflict resolution with simultaneous edits

## Scenario 2: Chat Application

**Simulate real-time chat functionality:**

1. Use the editors as chat message inputs
2. Send messages by typing and pressing Enter
3. Watch messages appear instantly in other tabs
4. Test message ordering and delivery

## Scenario 3: Live Dashboard

**Simulate real-time dashboard updates:**

1. Use one tab to send status updates
2. Watch updates appear in other tabs instantly
3. Test high-frequency updates (every second)
4. Monitor performance under load

## Scenario 4: Multiplayer Game State

**Simulate game state synchronization:**

1. Use JSON format for game state updates
2. Send position/action updates rapidly
3. Watch state synchronize across tabs
4. Test latency and consistency

---

## 📊 Demo Performance Metrics

## Expected Performance

| Metric | Typical Value | Excellent Value |
|--------|---------------|-----------------|
| **Latency** | < 2ms | < 1ms |
| **Throughput** | 5,000+ msg/sec | 8,000+ msg/sec |
| **Connection Setup** | < 10ms | < 5ms |
| **Memory Usage** | < 1MB/session | < 0.5KB/session |

## Real-time Measurements

The demo displays live performance metrics:

- **Message Latency**: Round-trip time for WebSocket messages
- **Connection Status**: WebSocket connection state
- **Throughput**: Messages per second
- **Memory Usage**: Client-side memory consumption
- **Error Rate**: Failed message delivery rate

---

## 🔧 Customizing the Demo

## Configuration Options

The demo can be customized by modifying the configuration:

```javascript

const config = {
    serverUrl: 'ws://localhost:8080/ws',
    maxMessageSize: 10240, // 10KB
    reconnectInterval: 1000, // 1 second
    heartbeatInterval: 5000, // 5 seconds
    enableLogging: true,
    logLevel: 'info'
};

```

## Adding Custom Features

**Custom Message Types:**

```javascript

function handleCustomMessage(message) {
    switch(message.type) {
        case 'cursor_position':
            updateCursorPosition(message.data);
            break;
        case 'user_presence':
            updateUserPresence(message.data);
            break;
        default:
            console.log('Unknown message type:', message.type);
    }
}

```

**Custom UI Elements:**
- Add user presence indicators
- Show real-time cursor positions
- Display typing indicators
- Add emoji reactions

---

## 🛠️ Troubleshooting

## Common Issues

### Connection Problems

**Issue**: WebSocket connection fails

```

1. Ensure the server is running: cargo run --bin main
2. Check the server URL: http://localhost:8080
3. Verify firewall settings allow port 8080
4. Check browser console for error messages

```

**Issue**: Sessions not syncing

```

1. Verify all tabs use the same session ID
2. Check WebSocket connection status
3. Look for error messages in the connection log
4. Try creating a new session

```

### Performance Issues

**Issue**: High latency or slow updates

```

1. Check system resources (CPU, memory)
2. Reduce message frequency
3. Close unnecessary browser tabs
4. Restart the server

```

**Issue**: Memory usage growing over time

```

1. Refresh browser tabs periodically
2. Clear browser cache
3. Restart the demo server
4. Check for JavaScript memory leaks

```

## Debug Information

Enable debug logging to get detailed information:

```javascript

localStorage.setItem('mcp-rs-debug', 'true');
location.reload();

```

This will show:
- Detailed WebSocket message logs
- Performance timing information
- Internal state changes
- Error stack traces

---

## 🎓 Learning Outcomes

After using the demo, you will understand:

## Technical Concepts

- ✅ **WebSocket Communication**: Real-time bidirectional communication
- ✅ **Session Management**: Stateful collaboration sessions
- ✅ **Event-driven Architecture**: Message-based system design
- ✅ **Performance Optimization**: Low-latency real-time systems

## Practical Skills

- ✅ **API Integration**: REST API usage patterns
- ✅ **Real-time Development**: Building collaborative features
- ✅ **Performance Monitoring**: Measuring and optimizing systems
- ✅ **Error Handling**: Robust error handling in real-time systems

## System Design

- ✅ **Scalability Patterns**: Designing for multiple users
- ✅ **Security Considerations**: Secure real-time communications
- ✅ **Monitoring & Observability**: System health and metrics
- ✅ **Production Deployment**: Real-world deployment considerations

---

## 🔗 Next Steps

## Continue Learning

1. **Read the Documentation**
   - [WebSocket API Reference](https://github.com/n-takatsu/mcp-rs/blob/main/docs/websocket-api.md)
   - [Session Management Guide](https://github.com/n-takatsu/mcp-rs/blob/main/docs/session-management-architecture.md)
   - [Development Guide](https://github.com/n-takatsu/mcp-rs/blob/main/project-docs/realtime-editing-development-guide.md)

2. **Explore the Code**
   - Browse the [source code](https://github.com/your-org/mcp-rs)
   - Study the implementation patterns
   - Contribute improvements

3. **Build Your Own Application**
   - Use MCP-RS in your projects
   - Integrate with your existing systems
   - Share your experience with the community

## Get Support

- 💬 [Join our Discord](https://discord.gg/mcp-rs)
- 📧 [Email Support](mailto:support@mcp-rs.dev)
- 🐛 [Report Issues](https://github.com/your-org/mcp-rs/issues)
- 💡 [Request Features](https://github.com/your-org/mcp-rs/discussions)

---

<div align="center">

**🎮 Ready to Experience Real-time Collaboration?**

[**Launch the Demo →**](http://localhost:8080/demo.html)

</div>