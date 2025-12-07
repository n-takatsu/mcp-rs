//! 動的トランスポート切り替えの統合テスト

use mcp_rs::transport::{DynamicTransportManager, TransportConfig};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_stdio_to_http_switch() {
    // STDIO transportで開始
    let config = TransportConfig::default();
    let manager = DynamicTransportManager::new(config).unwrap();

    // HTTPへ切り替え
    let addr = "127.0.0.1:13000".parse().unwrap();
    let result = manager.switch_to_http(addr).await;
    assert!(result.is_ok(), "HTTP切り替えに失敗: {:?}", result.err());
    
    // サーバー起動を待機
    sleep(Duration::from_millis(500)).await;
}

#[tokio::test]
async fn test_http_to_stdio_switch() {
    // STDIOで開始
    let config = TransportConfig::default();
    let manager = DynamicTransportManager::new(config).unwrap();
    
    // HTTPへ切り替え
    let addr = "127.0.0.1:13001".parse().unwrap();
    manager.switch_to_http(addr).await.unwrap();
    
    // サーバー起動を待機
    sleep(Duration::from_millis(500)).await;

    // STDIOへ切り替え
    let result = manager.switch_to_stdio().await;
    assert!(result.is_ok(), "STDIO切り替えに失敗: {:?}", result.err());
    
    // graceful shutdownを待機
    sleep(Duration::from_millis(600)).await;
}
