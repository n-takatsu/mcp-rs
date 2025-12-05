/// WebSocket TLS audit logging integration tests
///
/// This test module verifies that the WebSocket TLS implementation
/// properly integrates with the audit logging system.
use mcp_rs::security::{AuditConfig, AuditLevel, AuditLogger};
use mcp_rs::transport::websocket::{TlsConfig, WebSocketConfig, WebSocketTransport};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test]
async fn test_websocket_with_audit_logger() {
    // Create audit logger
    let audit_config = AuditConfig {
        max_memory_entries: 1000,
        min_log_level: AuditLevel::Info,
        enable_file_output: false,
        log_file_path: None,
        json_format: true,
        rotation_enabled: false,
        rotation_size: 100 * 1024 * 1024,
    };

    let audit_logger = Arc::new(AuditLogger::new(audit_config));

    // Create WebSocket configuration (plain - for easy testing)
    let ws_config = WebSocketConfig {
        url: "ws://127.0.0.1:9001".to_string(),
        server_mode: false,
        timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 3,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        use_tls: false,
        tls_config: None,
    };

    // Create WebSocket transport with audit logger
    let _transport = WebSocketTransport::new(ws_config)
        .expect("Failed to create WebSocket transport")
        .with_audit_logger(Arc::clone(&audit_logger));

    // Verify transport was created successfully (test passes if no panic)
}

#[tokio::test]
async fn test_websocket_tls_config_with_audit_logger() {
    // Create audit logger
    let audit_config = AuditConfig {
        max_memory_entries: 1000,
        min_log_level: AuditLevel::Info,
        enable_file_output: false,
        log_file_path: None,
        json_format: true,
        rotation_enabled: false,
        rotation_size: 100 * 1024 * 1024,
    };

    let audit_logger = Arc::new(AuditLogger::new(audit_config));

    // Create TLS configuration
    let tls_config = TlsConfig {
        cert_path: Some(PathBuf::from("certs/server.crt")),
        key_path: Some(PathBuf::from("certs/server.key")),
        ca_cert_path: Some(PathBuf::from("certs/ca.crt")),
        verify_server: true,
        accept_invalid_certs: false,
    };

    // Create WebSocket configuration with TLS
    let ws_config = WebSocketConfig {
        url: "wss://127.0.0.1:9443".to_string(),
        server_mode: false,
        timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 3,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        use_tls: true,
        tls_config: Some(tls_config),
    };

    // Create WebSocket transport with audit logger
    let _transport = WebSocketTransport::new(ws_config)
        .expect("Failed to create WebSocket transport")
        .with_audit_logger(Arc::clone(&audit_logger));

    // Verify transport was created successfully (test passes if no panic)
}

#[tokio::test]
async fn test_audit_logger_builder_pattern() {
    // Create audit logger
    let audit_config = AuditConfig {
        max_memory_entries: 500,
        min_log_level: AuditLevel::Info,
        enable_file_output: false,
        log_file_path: None,
        json_format: true,
        rotation_enabled: false,
        rotation_size: 50 * 1024 * 1024,
    };

    let audit_logger = Arc::new(AuditLogger::new(audit_config));

    // Build WebSocket transport with fluent API
    let transport = WebSocketTransport::new(WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: true,
        timeout_seconds: Some(60),
        heartbeat_interval: 45,
        max_reconnect_attempts: 5,
        reconnect_delay: 10,
        max_message_size: 32 * 1024 * 1024,
        max_connections: 200,
        use_tls: false,
        tls_config: None,
    })
    .expect("Failed to create transport")
    .with_audit_logger(audit_logger);

    // Verify builder pattern works (test passes if no panic)
}

#[tokio::test]
async fn test_audit_logger_optional() {
    // Create WebSocket transport without audit logger
    let transport = WebSocketTransport::new(WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: false,
        timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 3,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        use_tls: false,
        tls_config: None,
    })
    .expect("Failed to create transport");

    // Verify transport works without audit logger (optional, test passes if no panic)
}

#[test]
fn test_audit_config_creation() {
    // Test audit configuration creation
    let config = AuditConfig {
        max_memory_entries: 1000,
        min_log_level: AuditLevel::Info,
        enable_file_output: true,
        log_file_path: Some("logs/audit.log".to_string()),
        json_format: true,
        rotation_enabled: true,
        rotation_size: 100 * 1024 * 1024,
    };

    assert_eq!(config.max_memory_entries, 1000);
    assert_eq!(config.enable_file_output, true);
    assert_eq!(config.json_format, true);
    assert_eq!(config.rotation_enabled, true);
    assert_eq!(config.rotation_size, 100 * 1024 * 1024);
}

#[test]
fn test_tls_config_with_all_certificates() {
    // Test TLS configuration with all certificate paths
    let tls_config = TlsConfig {
        cert_path: Some(PathBuf::from("certs/server.crt")),
        key_path: Some(PathBuf::from("certs/server.key")),
        ca_cert_path: Some(PathBuf::from("certs/ca.crt")),
        verify_server: true,
        accept_invalid_certs: false,
    };

    assert!(tls_config.cert_path.is_some());
    assert!(tls_config.key_path.is_some());
    assert!(tls_config.ca_cert_path.is_some());
    assert_eq!(tls_config.verify_server, true);
    assert_eq!(tls_config.accept_invalid_certs, false);
}

#[test]
fn test_websocket_config_combinations() {
    // Test 1: Plain WebSocket
    let plain_config = WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: false,
        timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 3,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        use_tls: false,
        tls_config: None,
    };

    assert_eq!(plain_config.use_tls, false);
    assert!(plain_config.tls_config.is_none());

    // Test 2: TLS WebSocket
    let tls_ws_config = WebSocketConfig {
        url: "wss://localhost:9443".to_string(),
        server_mode: false,
        timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 3,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        use_tls: true,
        tls_config: Some(TlsConfig {
            cert_path: Some(PathBuf::from("certs/server.crt")),
            key_path: Some(PathBuf::from("certs/server.key")),
            ca_cert_path: None,
            verify_server: true,
            accept_invalid_certs: false,
        }),
    };

    assert_eq!(tls_ws_config.use_tls, true);
    assert!(tls_ws_config.tls_config.is_some());

    // Test 3: Server mode
    let server_config = WebSocketConfig {
        url: "ws://0.0.0.0:8080".to_string(),
        server_mode: true,
        timeout_seconds: Some(60),
        heartbeat_interval: 45,
        max_reconnect_attempts: 0, // 0 for infinite
        reconnect_delay: 10,
        max_message_size: 32 * 1024 * 1024,
        max_connections: 500,
        use_tls: false,
        tls_config: None,
    };

    assert_eq!(server_config.server_mode, true);
    assert_eq!(server_config.max_reconnect_attempts, 0); // 0 means infinite
}
