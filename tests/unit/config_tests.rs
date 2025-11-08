//! Configuration Tests
//!
//! 設定システムの単体テスト

use mcp_rs::config::{McpConfig, TransportConfig};

#[tokio::test]
async fn test_config_creation() {
    let config = McpConfig::default();
    assert!(config.server.bind_addr.is_some());
    assert_eq!(config.server.bind_addr.unwrap(), "127.0.0.1:8080");
}

#[tokio::test]
async fn test_transport_config() {
    let transport = TransportConfig {
        transport_type: Some("stdio".to_string()),
        bind_addr: Some("127.0.0.1:8080".to_string()),
        timeout: Some(30),
    };
    
    assert_eq!(transport.transport_type, Some("stdio".to_string()));
    assert_eq!(transport.bind_addr, Some("127.0.0.1:8080".to_string()));
    assert_eq!(transport.timeout, Some(30));
}