use mcp_rs::config::McpConfig;
use mcp_rs::core::{HandlerRegistry, Runtime, RuntimeConfig};

#[tokio::test]
async fn test_config_creation() {
    let config = McpConfig::default();
    assert!(config.server.bind_addr.is_some());
    assert_eq!(config.server.bind_addr.unwrap(), "127.0.0.1:8080");
}

#[tokio::test]
async fn test_handler_registry() {
    let mut registry = HandlerRegistry::new();

    // Test initial state
    assert!(!registry.is_initialized());
    assert_eq!(registry.handler_count(), 0);

    // Test initialization
    let init_result = registry.initialize().await;
    assert!(init_result.is_ok());
    assert!(registry.is_initialized());
}

#[tokio::test]
async fn test_runtime_basic() {
    let config = RuntimeConfig::default();
    let runtime = Runtime::new(config);

    // Test initialization
    let init_result = runtime.initialize().await;
    assert!(init_result.is_ok());
}

#[tokio::test]
async fn test_error_enum() {
    use mcp_rs::error::Error;
    use std::io;

    let io_error = io::Error::new(io::ErrorKind::Other, "test error");
    let error = Error::Io(io_error);
    assert!(matches!(error, Error::Io(_)));
}

#[tokio::test]
async fn test_transport_config() {
    use mcp_rs::config::TransportConfig;

    let transport = TransportConfig {
        transport_type: Some("stdio".to_string()),
        stdio: None,
        http: None,
    };

    assert_eq!(transport.transport_type.unwrap(), "stdio");
}
