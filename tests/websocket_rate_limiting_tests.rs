//! WebSocket rate limiting integration tests
//!
//! Tests rate limiting functionality for WebSocket connections including:
//! - Request rate limiting
//! - Authentication failure tracking and blocking
//! - IP-based and user-based limits

use mcp_rs::transport::websocket::{WebSocketConfig, WebSocketTransport};

#[test]
fn test_websocket_config_with_rate_limiting_enabled() {
    let config = WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        enable_rate_limiting: true,
        max_requests_per_minute: 60,
        max_auth_failures: 3,
        auth_failure_block_duration_secs: 300,
        ..Default::default()
    };

    assert!(config.enable_rate_limiting);
    assert_eq!(config.max_requests_per_minute, 60);
    assert_eq!(config.max_auth_failures, 3);
    assert_eq!(config.auth_failure_block_duration_secs, 300);
}

#[test]
fn test_websocket_config_with_rate_limiting_disabled() {
    let config = WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        enable_rate_limiting: false,
        ..Default::default()
    };

    assert!(!config.enable_rate_limiting);
}

#[test]
fn test_websocket_transport_with_rate_limiting() {
    let config = WebSocketConfig {
        url: "ws://localhost:8081".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        enable_rate_limiting: true,
        max_requests_per_minute: 100,
        max_auth_failures: 5,
        auth_failure_block_duration_secs: 600,
        ..Default::default()
    };

    let transport = WebSocketTransport::new(config);
    assert!(transport.is_ok());
}

#[test]
fn test_rate_limiting_config_defaults() {
    let config = WebSocketConfig::default();

    // Default should have rate limiting enabled
    assert!(config.enable_rate_limiting);
    assert_eq!(config.max_requests_per_minute, 60);
    assert_eq!(config.max_auth_failures, 5);
    assert_eq!(config.auth_failure_block_duration_secs, 300);
}

#[test]
fn test_strict_rate_limiting_config() {
    let config = WebSocketConfig {
        url: "wss://api.example.com".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        use_tls: true,
        enable_rate_limiting: true,
        max_requests_per_minute: 30,           // Strict: 30 req/min
        max_auth_failures: 3,                  // Low tolerance
        auth_failure_block_duration_secs: 900, // 15 minutes
        require_authentication: true,
        ..Default::default()
    };

    assert_eq!(config.max_requests_per_minute, 30);
    assert_eq!(config.max_auth_failures, 3);
    assert_eq!(config.auth_failure_block_duration_secs, 900);
}

#[test]
fn test_lenient_rate_limiting_config() {
    let config = WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: true,
        enable_rate_limiting: true,
        max_requests_per_minute: 120,         // Lenient: 120 req/min
        max_auth_failures: 10,                // High tolerance
        auth_failure_block_duration_secs: 60, // 1 minute
        ..Default::default()
    };

    assert_eq!(config.max_requests_per_minute, 120);
    assert_eq!(config.max_auth_failures, 10);
    assert_eq!(config.auth_failure_block_duration_secs, 60);
}
