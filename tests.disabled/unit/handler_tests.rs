//! Handler Tests
//!
//! ハンドラーシステムの単体テスト

use mcp_rs::core::HandlerRegistry;

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
