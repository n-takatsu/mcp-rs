//! Runtime Tests
//!
//! ランタイムシステムの単体テスト

use mcp_rs::core::{Runtime, RuntimeConfig};

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

    let io_error = io::Error::other("test error");
    let error = Error::Io(io_error);
    assert!(matches!(error, Error::Io(_)));
}
