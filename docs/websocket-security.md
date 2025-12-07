# WebSocket Security Features

This document describes the security features implemented in the MCP-RS WebSocket transport.

## Table of Contents

- [Overview](#overview)
- [Feature 1: Message-Level Rate Limiting](#feature-1-message-level-rate-limiting)
- [Feature 2: Session Validation](#feature-2-session-validation)
- [Feature 3: Authentication Timeout](#feature-3-authentication-timeout)
- [Configuration](#configuration)
- [Examples](#examples)
- [Best Practices](#best-practices)

## Overview

The WebSocket transport in MCP-RS includes comprehensive security features to protect against various attacks and unauthorized access:

1. **Message-Level Rate Limiting**: Prevents abuse by limiting the number of messages per IP
2. **Session Validation**: Validates and extends existing sessions automatically
3. **Authentication Timeout**: Enforces authentication within a time limit
4. **JWT Authentication**: Token-based authentication with multiple algorithms
5. **Origin Validation**: Protects against CSRF attacks
6. **Audit Logging**: Records security events for monitoring

## Feature 1: Message-Level Rate Limiting

### Purpose

Protects the server from message flooding attacks by enforcing a maximum number of messages per IP address per minute.

### How It Works

1. **Connection Metadata Tracking**: Each WebSocket connection stores its peer address and IP
2. **Per-Message Enforcement**: Before processing each incoming message, the rate limiter checks if the IP has exceeded the limit
3. **Violation Handling**: 
   - Sends JSON-RPC error response (code: -32000)
   - Logs the violation to the audit log
   - Skips processing the message
   - Keeps the connection open (non-blocking)
4. **Automatic Cleanup**: Connection metadata is cleared on disconnect

### Configuration

```rust
use mcp_rs::transport::websocket::WebSocketConfig;

let config = WebSocketConfig {
    enable_rate_limiting: true,
    max_requests_per_minute: 60, // Default: 60 messages per minute
    ..Default::default()
};
```

### Rate Limit Response

When the rate limit is exceeded, the client receives:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32000,
    "message": "Rate limit exceeded"
  },
  "id": null
}
```

### Implementation Details

- **Tracked per**: IP address (extracted from SocketAddr)
- **Enforcement**: Message-level (checked before processing each message)
- **Reset**: Counter resets after 1 minute
- **Logging**: Violations logged with category `SecurityAttack`, level `Warning`

## Feature 2: Session Validation

### Purpose

Allows clients to use existing sessions for authentication instead of re-authenticating with JWT on every connection.

### How It Works

1. **Session ID Extraction**: During WebSocket handshake, the server extracts session ID from:
   - `X-Session-ID` header
   - `session_id` cookie
2. **Session Validation**: If session ID is present:
   - Calls `SessionManager.get_session()` to retrieve session
   - Checks session state (Active, Expired, NotFound)
   - If Active → Allow connection, skip JWT validation
   - If Expired/NotFound → Return 401 Unauthorized
3. **Automatic Extension**: Session TTL is extended automatically:
   - After successful handshake (for existing sessions)
   - On each incoming message
4. **Session-to-Connection Mapping**: Server maintains mapping of SessionId → SocketAddr for tracking

### Configuration

```rust
use mcp_rs::transport::websocket::WebSocketConfig;

let config = WebSocketConfig {
    enable_session_management: true,
    session_ttl_seconds: 3600, // 1 hour
    ..Default::default()
};
```

### Connection Methods

#### Method 1: X-Session-ID Header

```bash
wscat -c ws://127.0.0.1:8082 -H "X-Session-ID: your-session-id"
```

#### Method 2: Cookie

```bash
wscat -c ws://127.0.0.1:8082 -H "Cookie: session_id=your-session-id"
```

#### Method 3: JWT (Creates New Session)

```bash
wscat -c ws://127.0.0.1:8082 -H "Authorization: Bearer your-jwt-token"
```

### Session States

| State | Description | Result |
|-------|-------------|--------|
| Active | Session is valid and not expired | Connection allowed |
| Expired | Session TTL has passed | 401 Unauthorized |
| NotFound | Session ID not found in store | 401 Unauthorized |

### Session Lifecycle

```
JWT Authentication
    ↓
Create Session → Store session_id → Return to client
    ↓
Client reconnects with X-Session-ID
    ↓
Session validated → Connection allowed
    ↓
Every message → Session TTL extended
    ↓
No activity for TTL → Session expires
    ↓
Next connection → Rejected (401)
```

## Feature 3: Authentication Timeout

### Purpose

Prevents slow authentication attacks by requiring clients to authenticate within a time limit after the WebSocket handshake.

### How It Works

1. **Timeout Enforcement**: When `require_authentication: true`, the server starts a timeout timer
2. **Handshake Wrapper**: The `accept_hdr_async()` call is wrapped with `tokio::time::timeout()`
3. **Timeout Handling**:
   - If authentication completes within timeout → Proceed normally
   - If timeout expires → Log event, set state to Disconnected, close connection
4. **Audit Logging**: Timeout events are logged with category `SecurityAttack`, level `Warning`

### Configuration

```rust
use mcp_rs::transport::websocket::WebSocketConfig;

let config = WebSocketConfig {
    require_authentication: true,
    auth_timeout_seconds: Some(30), // Default: 30 seconds
    ..Default::default()
};
```

### Timeout Behavior

- **Before timeout**: Client has time to send JWT token or session ID
- **After timeout**: Connection is immediately closed
- **Logging**: Event recorded with timeout duration and peer address

## Configuration

### Complete Example

```rust
use mcp_rs::transport::websocket::{
    JwtAlgorithm, JwtConfig, OriginValidationPolicy, WebSocketConfig
};

let jwt_config = JwtConfig {
    secret: "your-secret-key".to_string(),
    algorithm: JwtAlgorithm::HS256,
    required_claims: vec!["sub".to_string()],
    allowed_roles: vec!["admin".to_string(), "user".to_string()],
    validate_exp: true,
    validate_nbf: true,
    validate_iat: false,
    leeway_seconds: 60,
};

let config = WebSocketConfig {
    url: "ws://127.0.0.1:8082".to_string(),
    server_mode: true,
    
    // Authentication
    require_authentication: true,
    jwt_config: Some(jwt_config),
    auth_timeout_seconds: Some(30),
    
    // Session Management
    enable_session_management: true,
    session_ttl_seconds: 3600,
    
    // Rate Limiting
    enable_rate_limiting: true,
    max_requests_per_minute: 60,
    max_auth_failures: 5,
    auth_failure_block_duration_secs: 300,
    
    // Origin Validation
    origin_validation: OriginValidationPolicy::AllowList(vec![
        "https://example.com".to_string(),
    ]),
    require_origin_header: true,
    
    ..Default::default()
};
```

### Configuration Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enable_rate_limiting` | bool | true | Enable message-level rate limiting |
| `max_requests_per_minute` | u32 | 60 | Max messages per IP per minute |
| `enable_session_management` | bool | false | Enable session validation and auto-extension |
| `session_ttl_seconds` | u64 | 3600 | Session time-to-live (1 hour) |
| `require_authentication` | bool | false | Require JWT or session authentication |
| `auth_timeout_seconds` | Option<u64> | Some(30) | Authentication timeout (seconds) |
| `jwt_config` | Option<JwtConfig> | None | JWT authentication configuration |
| `origin_validation` | OriginValidationPolicy | RejectAll | Origin validation policy |
| `require_origin_header` | bool | false | Reject if Origin header missing |

## Examples

### Example 1: JWT Authentication

See [`examples/websocket_jwt_demo.rs`](../examples/websocket_jwt_demo.rs)

Demonstrates:
- JWT token generation
- Token validation during handshake
- Required claims and role-based access control
- Authentication timeout

### Example 2: Session Management

See [`examples/websocket_session_demo.rs`](../examples/websocket_session_demo.rs)

Demonstrates:
- Session creation on JWT auth
- Session validation on reconnection
- Automatic session TTL extension
- X-Session-ID header and Cookie support

### Example 3: Rate Limiting

See [`examples/websocket_rate_limit_demo.rs`](../examples/websocket_rate_limit_demo.rs)

Demonstrates:
- Message-level rate limiting
- Rate limit violation handling
- Error responses
- Audit logging

## Best Practices

### 1. Authentication

- **Always enable authentication in production**: Set `require_authentication: true`
- **Use strong secrets**: Use long, random secrets for JWT (minimum 32 bytes)
- **Validate token expiration**: Set `validate_exp: true` in JWT config
- **Use appropriate timeout**: Balance security and UX (30 seconds is reasonable)

### 2. Rate Limiting

- **Enable for all public servers**: Protects against DoS attacks
- **Tune limits based on use case**: Adjust `max_requests_per_minute` for your workload
- **Monitor violations**: Review audit logs for attack patterns
- **Consider burst traffic**: Default allows 60 messages/minute (1 per second)

### 3. Session Management

- **Enable for long-lived connections**: Reduces re-authentication overhead
- **Set appropriate TTL**: Balance security (shorter) vs UX (longer)
- **Use secure session storage**: Redis with authentication in production
- **Implement session invalidation**: Provide logout/revoke endpoints

### 4. Origin Validation

- **Never use `AllowAny` in production**: Only for development
- **Use `AllowList` for known origins**: Most secure option
- **Require Origin header**: Set `require_origin_header: true`
- **Consider regex patterns**: For subdomains or dynamic origins

### 5. Monitoring

- **Enable audit logging**: Track security events
- **Monitor rate limit violations**: May indicate attack attempts
- **Track authentication failures**: Multiple failures may indicate brute force
- **Alert on timeout events**: May indicate slow attacks or network issues

### 6. Defense in Depth

Use multiple security layers:
1. **Network level**: Firewall, reverse proxy, rate limiting
2. **Transport level**: TLS/WSS, Origin validation
3. **Application level**: JWT authentication, session management
4. **Message level**: Rate limiting, input validation
5. **Monitoring**: Audit logging, alerting

## Security Considerations

### Rate Limiting Bypass

- **IP spoofing**: Rate limiting is per IP; use reverse proxy to get real client IP
- **Distributed attacks**: May need additional DDoS protection
- **Legitimate burst traffic**: Tune limits to avoid blocking real users

### Session Security

- **Session hijacking**: Use TLS/WSS to prevent session ID interception
- **Session fixation**: Generate new session ID on authentication
- **Session storage**: Secure Redis instance, consider encryption at rest

### Authentication

- **Token leakage**: Always use HTTPS/WSS to prevent token interception
- **Token replay**: Short expiration times reduce replay window
- **Weak secrets**: Use cryptographically secure random secrets

### Timing Attacks

- **Authentication checks**: Use constant-time comparison for tokens
- **Session validation**: Avoid revealing whether session exists via timing

## Troubleshooting

### Rate Limit False Positives

**Symptom**: Legitimate clients getting rate limited

**Solutions**:
- Increase `max_requests_per_minute`
- Check if multiple clients share the same IP (NAT)
- Review audit logs for actual attack patterns

### Session Expiration Issues

**Symptom**: Sessions expiring too quickly

**Solutions**:
- Increase `session_ttl_seconds`
- Verify auto-extension is working (check logs)
- Ensure client is sending messages regularly

### Authentication Timeout

**Symptom**: Clients can't authenticate in time

**Solutions**:
- Increase `auth_timeout_seconds`
- Check network latency between client and server
- Verify JWT token generation is fast enough

### Origin Validation Failures

**Symptom**: Connections rejected with "Origin not allowed"

**Solutions**:
- Add client origin to `AllowList`
- Check if client is sending Origin header
- Consider using `AllowPattern` for dynamic origins

## Further Reading

- [WebSocket Security (OWASP)](https://owasp.org/www-community/vulnerabilities/WebSocket_security)
- [JWT Best Practices (RFC 8725)](https://datatracker.ietf.org/doc/html/rfc8725)
- [Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
- [Rate Limiting Patterns](https://cloud.google.com/architecture/rate-limiting-strategies-techniques)
