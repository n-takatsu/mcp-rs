# WebSocket Transport

## Overview

mcp-rs accepts MCP over WebSocket on the `/ws` endpoint of the HTTP transport. The library also has a WebSocket client (`WebSocketTransport`).

Up to v0.23.0, the library also had a standalone WebSocket server (`WebSocketServer`, and `server_mode` of `WebSocketTransport`). It had no authentication and no `Origin` check, and mcp-rs itself did not use it, so it was removed (#543). Use `/ws` on the HTTP transport instead.

## Server: `/ws` on the HTTP transport

In the configuration file:

```toml
[transport]
transport_type = "http"

[transport.http]
enable_websocket_upgrade = true
# websocket_max_connections = 1000       # more upgrades get 503
# websocket_idle_timeout_seconds = 300   # close when the client answers nothing for this long
```

In library code:

```rust
use mcp_rs::transport::http::HttpConfig;

let config = HttpConfig {
    enable_websocket_upgrade: true,   // mounts /ws on this listener
    websocket_max_connections: 1000,  // 503s new upgrades past this limit
    // TLS/HSTS/certificate pinning are the *same* fields /mcp already uses -
    // there is no separate WebSocket-specific security config surface.
    tls_enabled: true,
    tls_cert_path: Some("./certs/server.crt".to_string()),
    tls_key_path: Some("./certs/server.key".to_string()),
    enforce_https: true,
    ..HttpConfig::default()
};
```

`/ws` shares `HttpTransport`'s entire security stack with `/` and `/mcp`:
TLS termination (`tls_enabled` + `tls_cert_path`/`tls_key_path`, TLS 1.3 via
rustls), `enforce_https` (rejects plain `ws://` the same way it rejects plain
HTTP; behind a reverse proxy, it accepts `X-Forwarded-Proto: https` only from
an address in `trusted_proxies`), HSTS headers, certificate pinning,
`network_policy`, the `Origin` check, API keys and OAuth, and
`anti_replay_enabled`. There is no separate `enable_wss`/`ws_tls_*`
configuration - by design, one security posture governs the whole listener.
See `HttpConfig` in `src/transport/http.rs` for the authoritative field list,
and `HttpTransportConfig` in `src/config.rs` for the TOML-facing equivalents.

Each text message on `/ws` is one MCP JSON-RPC message, handled the same way
as a request to `/mcp`. With `anti_replay_enabled`, each message carries
`nonce` and `timestamp` at the top level of its JSON, since WebSocket messages
have no HTTP headers. See [Security](../website/docs/security.md) for the
limits (message size, idle timeout, response send timeout).

## Client: `WebSocketTransport`

```rust
use mcp_rs::transport::websocket::{WebSocketConfig, WebSocketTransport};
use mcp_rs::transport::Transport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = WebSocketConfig {
        url: "ws://127.0.0.1:8082/ws".to_string(),
        ..Default::default()
    };

    let mut transport = WebSocketTransport::new(config)?;
    transport.start().await?; // connects to `url`

    // Send/receive messages...

    transport.stop().await?;
    Ok(())
}
```

Run the example:

```bash
cargo run --example websocket_transport_foundation
```

## Testing

```bash
cargo test websocket
```
