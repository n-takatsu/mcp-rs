//! WebSocket Integration Tests
//!
//! WebSocket機能の統合テスト

use mcp_rs::transport::websocket::types::PoolConfig;
use mcp_rs::transport::websocket::ConnectionPool;
use std::time::Duration;

#[tokio::test]
async fn test_connection_pool_basic() {
    let config = PoolConfig {
        max_connections: 10,
        min_connections: 2,
        connection_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(60),
        health_check_interval: Duration::from_secs(30),
    };

    let result = ConnectionPool::new(config);
    assert!(result.is_ok(), "Connection pool creation should succeed");

    let pool = result.unwrap();
    let stats = pool.statistics();
    assert_eq!(
        stats.active_connections, 0,
        "Pool should start with zero active connections"
    );
}

#[tokio::test]
async fn test_pool_auto_scaling() {
    let config = PoolConfig {
        max_connections: 100,
        min_connections: 5,
        connection_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(60),
        health_check_interval: Duration::from_secs(30),
    };

    let pool = ConnectionPool::new(config).unwrap();

    // 自動スケーリングを有効化
    pool.set_auto_scaling(true).await;

    // 統計を取得
    let stats = pool.statistics();

    // 統計の基本的な検証
    assert!(stats.total_connections <= 100);
    assert_eq!(stats.pending_requests, 0);
}
