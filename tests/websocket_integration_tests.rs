//! WebSocket Integration Tests
//!
//! WebSocket機能の統合テスト

use mcp_rs::transport::websocket::{
    CompressionConfig, CompressionManager, CompressionType, ConnectionPool, PoolConfig,
    RateLimitConfig, RateLimitStrategy, RateLimiter, WebSocketMetrics,
};
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
async fn test_metrics_collection() {
    let metrics = WebSocketMetrics::new();
    assert!(metrics.is_ok(), "Metrics creation should succeed");

    let metrics = metrics.unwrap();

    // 接続を追跡
    metrics.increment_connections();
    metrics.increment_connections();
    assert_eq!(metrics.snapshot().connections_total, 2);

    // メッセージを追跡
    metrics.increment_messages_sent_by(10);
    metrics.increment_messages_received_by(15);

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.messages_sent_total, 10);
    assert_eq!(snapshot.messages_received_total, 15);

    // メトリクスエクスポート
    let export = metrics.export_text();
    assert!(export.is_ok(), "Metrics export should succeed");
}

#[tokio::test]
async fn test_rate_limiting() {
    let config = RateLimitConfig {
        strategy: RateLimitStrategy::TokenBucket,
        max_requests_per_second: 10,
        max_burst: 10,
        window_size_ms: 1000,
    };

    let limiter = RateLimiter::new(config);

    // 最初の10リクエストは許可される
    for _ in 0..10 {
        let allowed = limiter.check_rate_limit("test_key").await;
        assert!(allowed.is_ok());
        assert!(allowed.unwrap(), "Request should be allowed");
    }

    // 11番目のリクエストは拒否される
    let allowed = limiter.check_rate_limit("test_key").await;
    assert!(allowed.is_ok());
    assert!(!allowed.unwrap(), "Request should be denied");
}

#[test]
fn test_compression_gzip() {
    let config = CompressionConfig {
        compression_type: CompressionType::Gzip,
        level: 6,
        min_size: 100,
    };

    let manager = CompressionManager::new(config);

    let data = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                   Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                   This text should be long enough to compress effectively.";

    let compressed = manager.compress(data);
    assert!(compressed.is_ok(), "Compression should succeed");

    let compressed = compressed.unwrap();
    assert!(
        compressed.len() < data.len(),
        "Compressed data should be smaller"
    );

    let decompressed = manager.decompress(&compressed);
    assert!(decompressed.is_ok(), "Decompression should succeed");
    assert_eq!(
        decompressed.unwrap(),
        data.to_vec(),
        "Decompressed data should match original"
    );
}

#[test]
fn test_compression_brotli() {
    let config = CompressionConfig {
        compression_type: CompressionType::Gzip,
        level: 6,
        min_size: 100,
    };

    let manager = CompressionManager::new(config);

    let data = vec![b'A'; 1000]; // 1KB of 'A' characters

    let compressed = manager.compress_brotli(&data);
    assert!(compressed.is_ok(), "Brotli compression should succeed");

    let compressed = compressed.unwrap();
    assert!(
        compressed.len() < data.len(),
        "Compressed data should be smaller than original"
    );

    let decompressed = manager.decompress_brotli(&compressed);
    assert!(decompressed.is_ok(), "Brotli decompression should succeed");
    assert_eq!(
        decompressed.unwrap(),
        data,
        "Decompressed data should match original"
    );
}

#[tokio::test]
async fn test_rate_limit_strategies() {
    // TokenBucket
    let token_bucket_config = RateLimitConfig {
        strategy: RateLimitStrategy::TokenBucket,
        max_requests_per_second: 5,
        max_burst: 5,
        window_size_ms: 1000,
    };
    let limiter = RateLimiter::new(token_bucket_config);
    for _ in 0..5 {
        assert!(limiter.check_rate_limit("tb_test").await.unwrap());
    }
    assert!(!limiter.check_rate_limit("tb_test").await.unwrap());

    // LeakyBucket
    let leaky_bucket_config = RateLimitConfig {
        strategy: RateLimitStrategy::LeakyBucket,
        max_requests_per_second: 5,
        max_burst: 5,
        window_size_ms: 1000,
    };
    let limiter = RateLimiter::new(leaky_bucket_config);
    for _ in 0..5 {
        assert!(limiter.check_rate_limit("lb_test").await.unwrap());
    }
    assert!(!limiter.check_rate_limit("lb_test").await.unwrap());

    // SlidingWindow
    let sliding_window_config = RateLimitConfig {
        strategy: RateLimitStrategy::SlidingWindow,
        max_requests_per_second: 5,
        max_burst: 5,
        window_size_ms: 1000,
    };
    let limiter = RateLimiter::new(sliding_window_config);
    for _ in 0..5 {
        assert!(limiter.check_rate_limit("sw_test").await.unwrap());
    }
    assert!(!limiter.check_rate_limit("sw_test").await.unwrap());
}

#[test]
fn test_compression_stats() {
    use mcp_rs::transport::websocket::CompressionStats;

    let mut stats = CompressionStats::new();

    stats.record_compression(10000, 3000);
    assert_eq!(stats.original_bytes, 10000);
    assert_eq!(stats.compressed_bytes, 3000);
    assert_eq!(stats.bytes_saved(), 7000);
    assert!((stats.compression_ratio - 70.0).abs() < 0.1);

    stats.record_compression(5000, 2000);
    assert_eq!(stats.original_bytes, 15000);
    assert_eq!(stats.compressed_bytes, 5000);
    assert_eq!(stats.compression_count, 2);
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
