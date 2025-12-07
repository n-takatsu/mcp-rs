//! WebSocket Session Management Demo
//!
//! This example demonstrates how to use session management with WebSocket transport.
//!
//! Run this example:
//! ```bash
//! cargo run --example websocket_session_demo
//! ```

use mcp_rs::security::AuditLogger;
use mcp_rs::session::SessionManager;
use mcp_rs::transport::websocket::{
    JwtAlgorithm, JwtConfig, OriginValidationPolicy, WebSocketConfig, WebSocketTransport,
};
use mcp_rs::transport::Transport;
use std::sync::Arc;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== WebSocket Session Management Demo ===");

    // Create session manager (1 hour TTL)
    let session_manager = Arc::new(SessionManager::with_ttl(1));

    // Create a test session
    let test_user = "testuser@example.com";
    let session = session_manager
        .create_session(test_user.to_string())
        .await?;

    info!("Created test session:");
    info!("  Session ID: {}", session.id);
    info!("  User: {}", test_user);
    info!("  TTL: 1 hour");

    // Configure WebSocket with session management
    let jwt_config = JwtConfig {
        secret: "my-secret-key-for-testing-only".to_string(),
        algorithm: JwtAlgorithm::HS256,
        required_claims: vec!["sub".to_string()],
        allowed_roles: vec![],
        validate_exp: true,
        validate_nbf: false,
        validate_iat: false,
        leeway_seconds: 60,
    };

    let ws_config = WebSocketConfig {
        url: "ws://127.0.0.1:8083".to_string(),
        server_mode: true,
        require_authentication: false, // Allow session-based auth without JWT
        jwt_config: Some(jwt_config),
        enable_session_management: true,
        session_ttl_seconds: 3600, // 1 hour
        origin_validation: OriginValidationPolicy::AllowAny,
        enable_rate_limiting: true,
        max_requests_per_minute: 60,
        ..Default::default()
    };

    info!("\nWebSocket configuration:");
    info!("  URL: {}", ws_config.url);
    info!(
        "  Session management: {}",
        ws_config.enable_session_management
    );
    info!("  Session TTL: {} seconds", ws_config.session_ttl_seconds);
    info!("  Auto-extend on message: enabled");

    // Create audit logger
    use mcp_rs::security::AuditConfig;
    let audit_logger = Arc::new(AuditLogger::new(AuditConfig::default()));

    // Create WebSocket transport
    let mut transport = WebSocketTransport::new(ws_config)?.with_audit_logger(audit_logger);

    info!("\n=== Starting WebSocket Server ===");
    info!("Server is listening on ws://127.0.0.1:8083");
    info!("\n=== Connection Methods ===");
    info!("\n1. Using existing session (X-Session-ID header):");
    info!(
        "   wscat -c ws://127.0.0.1:8083 -H \"X-Session-ID: {}\"",
        session.id
    );
    info!("\n2. Using existing session (Cookie):");
    info!(
        "   wscat -c ws://127.0.0.1:8083 -H \"Cookie: session_id={}\"",
        session.id
    );
    info!("\n3. Using JWT (creates new session):");
    info!("   wscat -c ws://127.0.0.1:8083 -H \"Authorization: Bearer <token>\"");

    info!("\n=== Session Features ===");
    info!("✓ Session validation on connection");
    info!("✓ Automatic session creation on JWT auth");
    info!("✓ Session TTL auto-extension on each message");
    info!("✓ Expired session detection");
    info!("✓ Session-to-connection mapping");

    info!("\n=== Session States ===");
    info!("1. Active → Connection allowed");
    info!("2. Expired → 401 Unauthorized");
    info!("3. NotFound → 401 Unauthorized (if X-Session-ID provided)");

    info!("\n=== Session Lifecycle ===");
    info!("• JWT auth → Create session → Store session_id → Return to client");
    info!("• Client reconnects → Send X-Session-ID header → Session validated → Connected");
    info!("• Every message → Session TTL extended automatically");
    info!("• No activity for TTL → Session expires → Next connection rejected");

    info!("\nTest session is ready: {}", session.id);
    info!("You can use this session ID to connect!");

    // Start the server
    transport.start().await?;

    // Keep the server running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    Ok(())
}
