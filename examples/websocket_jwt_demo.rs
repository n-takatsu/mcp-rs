//! WebSocket JWT Authentication Demo
//!
//! This example demonstrates how to use JWT authentication with WebSocket transport.
//!
//! Run this example:
//! ```bash
//! cargo run --example websocket_jwt_demo
//! ```

use jsonwebtoken::{encode, EncodingKey, Header};
use mcp_rs::security::AuditLogger;
use mcp_rs::transport::websocket::{JwtAlgorithm, JwtConfig, OriginValidationPolicy, WebSocketConfig, WebSocketTransport};
use mcp_rs::transport::Transport;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, Level};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    exp: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("=== WebSocket JWT Authentication Demo ===");

    // Generate a JWT token
    let secret = "my-secret-key-for-testing-only";
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        + 3600; // 1 hour

    let claims = Claims {
        sub: "user123".to_string(),
        role: "admin".to_string(),
        exp,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    info!("Generated JWT token: {}", token);
    info!("Token claims: sub={}, role={}, exp={}", claims.sub, claims.role, claims.exp);

    // Configure WebSocket with JWT authentication
    let jwt_config = JwtConfig {
        secret: secret.to_string(),
        algorithm: JwtAlgorithm::HS256,
        required_claims: vec!["sub".to_string(), "role".to_string()],
        allowed_roles: vec!["admin".to_string(), "user".to_string()],
        validate_exp: true,
        validate_nbf: false,
        validate_iat: false,
        leeway_seconds: 60,
    };

    let ws_config = WebSocketConfig {
        url: "ws://127.0.0.1:8082".to_string(),
        server_mode: true,
        require_authentication: true,
        jwt_config: Some(jwt_config),
        auth_timeout_seconds: Some(30),
        origin_validation: OriginValidationPolicy::AllowAny,
        enable_rate_limiting: true,
        max_requests_per_minute: 60,
        ..Default::default()
    };

    info!("WebSocket configuration:");
    info!("  URL: {}", ws_config.url);
    info!("  Authentication required: {}", ws_config.require_authentication);
    info!("  Auth timeout: {:?} seconds", ws_config.auth_timeout_seconds);
    info!("  JWT algorithm: {:?}", ws_config.jwt_config.as_ref().map(|c| &c.algorithm));
    info!("  Required claims: {:?}", ws_config.jwt_config.as_ref().map(|c| &c.required_claims));
    info!("  Allowed roles: {:?}", ws_config.jwt_config.as_ref().map(|c| &c.allowed_roles));

    // Create audit logger
    use mcp_rs::security::AuditConfig;
    let audit_logger = Arc::new(AuditLogger::new(AuditConfig::default()));

    // Create WebSocket transport
    let mut transport = WebSocketTransport::new(ws_config)?
        .with_audit_logger(audit_logger);

    info!("\n=== Starting WebSocket Server ===");
    info!("Server is listening on ws://127.0.0.1:8082");
    info!("\nTo connect, use a WebSocket client with:");
    info!("  Authorization: Bearer {}", token);
    info!("\nExample using wscat:");
    info!("  wscat -c ws://127.0.0.1:8082 -H \"Authorization: Bearer {}\"", token);
    info!("\n=== Authentication Features ===");
    info!("✓ JWT token validation");
    info!("✓ Algorithm verification (HS256)");
    info!("✓ Required claims check (sub, role)");
    info!("✓ Role-based access control (admin, user)");
    info!("✓ Token expiration validation");
    info!("✓ Authentication timeout (30 seconds)");
    info!("\n=== Error Scenarios ===");
    info!("1. Missing Authorization header → 401 Unauthorized");
    info!("2. Invalid token format → 401 Unauthorized");
    info!("3. Expired token → 401 Unauthorized");
    info!("4. Invalid role → 401 Unauthorized");
    info!("5. Authentication timeout → Connection closed");

    // Start the server
    transport.start().await?;

    // Keep the server running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    Ok(())
}
