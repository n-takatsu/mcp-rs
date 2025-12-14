//! WebSocket Rate Limiting Demo
//!
//! This example demonstrates message-level rate limiting for WebSocket connections.
//!
//! Run this example:
//! ```bash
//! cargo run --example websocket_rate_limit_demo
//! ```

use mcp_rs::security::AuditLogger;
use mcp_rs::transport::websocket::{OriginValidationPolicy, WebSocketConfig, WebSocketTransport};
use mcp_rs::transport::Transport;
use std::sync::Arc;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== WebSocket Rate Limiting Demo ===");

    // Configure WebSocket with strict rate limiting
    let max_requests = 10; // Very low for demo purposes
    let ws_config = WebSocketConfig {
        url: "ws://127.0.0.1:8084".to_string(),
        server_mode: true,
        require_authentication: false, // No auth required for demo
        enable_rate_limiting: true,
        max_requests_per_minute: max_requests,
        origin_validation: OriginValidationPolicy::AllowAny,
        ..Default::default()
    };

    info!("WebSocket configuration:");
    info!("  URL: {}", ws_config.url);
    info!("  Rate limiting: {}", ws_config.enable_rate_limiting);
    info!("  Max requests per minute: {}", max_requests);
    info!("  Enforcement level: Message-level (per incoming message)");

    // Create audit logger
    use mcp_rs::security::AuditConfig;
    let audit_logger = Arc::new(AuditLogger::new(AuditConfig::default()));

    // Create WebSocket transport
    let mut transport = WebSocketTransport::new(ws_config)?.with_audit_logger(audit_logger);

    info!("\n=== Starting WebSocket Server ===");
    info!("Server is listening on ws://127.0.0.1:8084");
    info!("\n=== Rate Limiting Features ===");
    info!("✓ Per-IP rate limiting");
    info!("✓ Message-level enforcement");
    info!("✓ Connection metadata tracking");
    info!("✓ Rate limit violation logging");
    info!("✓ JSON-RPC error responses");

    info!("\n=== Testing Rate Limits ===");
    info!("Connect with: wscat -c ws://127.0.0.1:8084");
    info!("\nSend messages rapidly to trigger rate limit:");
    info!("  1st-10th message → Processed normally");
    info!("  11th+ message → Rate limit error");

    info!("\n=== Rate Limit Response ===");
    info!("When rate limit is exceeded, you'll receive:");
    info!(
        r#"{{
  "jsonrpc": "2.0",
  "error": {{
    "code": -32000,
    "message": "Rate limit exceeded"
  }},
  "id": null
}}"#
    );

    info!("\n=== Rate Limit Tracking ===");
    info!("• Each message increments counter per IP");
    info!("• Counter resets after 1 minute");
    info!("• Violations logged to audit log");
    info!("• Connection remains open (non-blocking)");

    info!("\n=== Audit Log Events ===");
    info!("Rate limit violations are logged with:");
    info!("  • Category: SecurityAttack");
    info!("  • Level: Warning");
    info!("  • Metadata: peer_ip, peer_addr, max_requests_per_minute");

    info!("\n=== Implementation Details ===");
    info!("• ConnectionMetadata tracks peer_addr and peer_ip");
    info!("• RateLimiter.check_rate_limit() called before processing");
    info!("• Violations → Error response + Skip message processing");
    info!("• Cleanup on disconnect");

    info!(
        "\nServer is ready! Try sending {} messages rapidly.",
        max_requests + 5
    );

    // Start the server
    transport.start().await?;

    // Keep the server running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    Ok(())
}
