//! WebSocket Transport Demo
//!
//! Demonstrates WebSocket transport with client/server modes
//!
//! Run server:
//! ```bash
//! cargo run --example websocket_transport_demo -- server
//! ```
//!
//! Run client (in another terminal):
//! ```bash
//! cargo run --example websocket_transport_demo -- client
//! ```

use mcp_rs::transport::websocket::{WebSocketConfig, WebSocketTransport};
use mcp_rs::transport::Transport;
use mcp_rs::types::{JsonRpcRequest, JsonRpcResponse, RequestId};
use serde_json::json;
use std::env;
use tokio::time::{sleep, Duration};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let args: Vec<String> = env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("server");

    match mode {
        "server" => run_server().await?,
        "client" => run_client().await?,
        _ => {
            eprintln!("Usage: {} [server|client]", args[0]);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    info!("🚀 Starting WebSocket server...");

    let config = WebSocketConfig {
        url: "ws://127.0.0.1:8082".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        use_tls: false,
        tls_config: None,
        origin_validation: mcp_rs::transport::websocket::OriginValidationPolicy::AllowAny,
        require_origin_header: false,
        jwt_config: None,
        require_authentication: false,
        auth_timeout_seconds: Some(30),
        heartbeat_interval: 10,
        max_reconnect_attempts: 5,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        ..Default::default()
    };

    let mut transport = WebSocketTransport::new(config)?;
    transport.start().await?;

    info!("✅ Server started, waiting for client connection...");

    // Wait for connection
    sleep(Duration::from_secs(2)).await;

    if transport.is_connected() {
        info!("🔗 Client connected!");

        // Send a welcome message
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({
                "message": "Welcome to MCP-RS WebSocket Server!",
                "timestamp": chrono::Utc::now().to_rfc3339()
            })),
            error: None,
            id: RequestId::Number(1),
        };

        transport.send_message(response).await?;
        info!("📤 Sent welcome message");

        // Handle incoming messages
        for i in 0..10 {
            if let Some(request) = transport.receive_message().await? {
                info!("📨 Received: {:?}", request);

                // Echo response
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(json!({
                        "echo": request,
                        "count": i
                    })),
                    error: None,
                    id: request.id,
                };

                transport.send_message(response).await?;
            }
            sleep(Duration::from_millis(500)).await;
        }

        // Connection stats
        let stats = transport.connection_stats();
        info!(
            "📊 Stats - Sent: {}, Received: {}, Bytes sent: {}, Bytes received: {}",
            stats.messages_sent, stats.messages_received, stats.bytes_sent, stats.bytes_received
        );
    } else {
        info!("❌ No client connected");
    }

    transport.stop().await?;
    info!("🛑 Server stopped");

    Ok(())
}

async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    info!("🚀 Starting WebSocket client...");

    let config = WebSocketConfig {
        url: "ws://127.0.0.1:8082".to_string(),
        server_mode: false,
        timeout_seconds: Some(30),
        use_tls: false,
        tls_config: None,
        origin_validation: mcp_rs::transport::websocket::OriginValidationPolicy::AllowAny,
        require_origin_header: false,
        jwt_config: None,
        require_authentication: false,
        auth_timeout_seconds: Some(30),
        heartbeat_interval: 10,
        max_reconnect_attempts: 5,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        ..Default::default()
    };

    let mut transport = WebSocketTransport::new(config)?;
    transport.start().await?;

    info!("✅ Connected to server");

    // Receive welcome message
    if let Some(response) = transport.receive_message().await? {
        info!("📨 Received welcome: {:?}", response);
    }

    // Send test messages
    for i in 0..5 {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test_method".to_string(),
            params: Some(json!({
                "message": format!("Hello from client #{}", i),
                "timestamp": chrono::Utc::now().to_rfc3339()
            })),
            id: RequestId::Number(i + 100),
        };

        // Send request as a pseudo-response (for demo purposes)
        transport
            .send_message(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({
                    "method": request.method,
                    "params": request.params
                })),
                error: None,
                id: RequestId::Number(i + 100),
            })
            .await?;
        info!("📤 Sent message #{}", i);

        sleep(Duration::from_secs(1)).await;

        // Receive echo
        if let Some(response) = transport.receive_message().await? {
            info!("📨 Received echo: {:?}", response);
        }
    }

    // Connection stats
    let stats = transport.connection_stats();
    info!(
        "📊 Stats - Sent: {}, Received: {}, Bytes sent: {}, Bytes received: {}",
        stats.messages_sent, stats.messages_received, stats.bytes_sent, stats.bytes_received
    );

    transport.stop().await?;
    info!("🛑 Client stopped");

    Ok(())
}
