# MCP-RS Logging Architecture

## Overview

This document defines the comprehensive logging strategy for the mcp-rs project, focusing on error tracking, bug isolation, and maintenance acceleration.

## Goals

-   **Error Tracking**: Detailed error context with correlation IDs
-   **Bug Isolation**: Structured logs for rapid issue identification
-   **Maintenance Speed**: Clear operational visibility
-   **Security Monitoring**: Access control and threat detection
-   **Performance Analysis**: Request timing and resource usage

## Architecture

### Core Logging Framework

-   **Base**: `tracing` crate with structured logging
-   **Output**: JSON format for production, human-readable for development
-   **Transport**: Console, file rotation, and optional external systems (e.g., ELK stack)

### Log Levels Strategy

-   **ERROR**: System failures, plugin crashes, security violations
-   **WARN**: Degraded performance, retry attempts, suspicious activity
-   **INFO**: Request lifecycle, plugin operations, configuration changes
-   **DEBUG**: Detailed flow control, parameter validation
-   **TRACE**: Deep debugging, protocol message details

### Correlation & Context

-   **Request ID**: Unique identifier for each MCP request
-   **Session ID**: Client session tracking
-   **Plugin Context**: Plugin name, version, operation
-   **User Context**: Authentication info (when applicable)

## Implementation Structure

### 1. Core Logging Module (`src/core/logging.rs`)

-   Log configuration and initialization
-   Context propagation utilities
-   Correlation ID management
-   Performance metrics collection

### 2. Request Tracing

-   Automatic request ID generation
-   Span creation for each operation
-   Error context preservation
-   Response time tracking

### 3. Plugin Logging Framework

-   Plugin-specific log contexts
-   Operation tracing across plugin boundaries
-   Resource access logging
-   Error propagation with context

### 4. Security & Audit Logging

-   Authentication events
-   Authorization failures
-   Resource access patterns
-   Suspicious activity detection

### 5. Performance Monitoring

-   Request duration metrics
-   Plugin execution time
-   Resource utilization
-   Error rate tracking

## Log Format Standards

### Structured Format (JSON)

```json
{
    "timestamp": "2024-11-02T10:30:00.000Z",
    "level": "INFO",
    "message": "Tool execution completed",
    "request_id": "req_123456789",
    "session_id": "sess_abcdef",
    "plugin": "wordpress",
    "operation": "get_posts",
    "duration_ms": 245,
    "user": "client_id_xyz",
    "context": {
        "tool_name": "wp_get_posts",
        "params": { "limit": 10 },
        "result_count": 5
    }
}
```

### Human-Readable Format (Development)

```
[2024-11-02T10:30:00.000Z INFO mcp_rs::wordpress] Tool execution completed
  request_id=req_123456789 session_id=sess_abcdef operation=get_posts duration_ms=245
  tool_name=wp_get_posts params={"limit": 10} result_count=5
```

## Configuration

### Environment Variables

-   `RUST_LOG`: Standard log level configuration
-   `MCP_LOG_FORMAT`: json|human (default: human for dev, json for prod)
-   `MCP_LOG_FILE`: Optional log file path
-   `MCP_REQUEST_LOGGING`: Enable/disable request logging (default: true)

### Configuration File Integration

```toml
[logging]
level = "info"
format = "json"
file = "/var/log/mcp-rs/server.log"
rotation = "daily"
request_logging = true
performance_metrics = true

[logging.plugins]
# Plugin-specific log levels
wordpress = "debug"
github = "info"
```

## Operational Features

### 1. Error Context Preservation

-   Full error chain logging
-   Stack trace capture for internal errors
-   Plugin state at error time
-   Request parameters (sanitized)

### 2. Performance Monitoring

-   Request duration histograms
-   Plugin execution time tracking
-   Resource access patterns
-   Throughput metrics

### 3. Security Monitoring

-   Failed authentication attempts
-   Unauthorized access attempts
-   Rate limiting violations
-   Suspicious parameter patterns

### 4. Maintenance Support

-   Configuration reload events
-   Plugin lifecycle tracking
-   Health check status
-   System resource utilization

## Implementation Priorities

### Phase 1: Core Infrastructure

1. Basic structured logging setup
2. Request ID generation and propagation
3. Error context preservation
4. Plugin operation tracing

### Phase 2: Advanced Features

1. Performance metrics collection
2. Security event logging
3. Log rotation and management
4. External system integration

### Phase 3: Analytics & Monitoring

1. Real-time dashboard support
2. Anomaly detection
3. Automated alerting
4. Long-term trend analysis

## Integration Points

### Plugin SDK

-   Automatic context propagation
-   Plugin-specific loggers
-   Error reporting helpers
-   Performance measurement utilities

### Transport Layer

-   Request/response logging
-   Connection event tracking
-   Protocol error logging
-   Transport-specific metrics

### Configuration System

-   Configuration change auditing
-   Validation error logging
-   Hot-reload event tracking
-   Environment detection

This logging architecture ensures comprehensive observability while maintaining performance and usability for both development and production environments.
