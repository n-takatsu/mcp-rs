//! WebSocket Transport統合テスト

use mcp_rs::transport::websocket::types::*;
use mcp_rs::transport::websocket::*;
use std::time::Duration;

#[tokio::test]
async fn test_websocket_connection_creation() {
    let result = WebSocketConnection::builder()
        .url("ws://localhost:8080")
        .timeout(Duration::from_secs(5))
        .build();

    // 実際のサーバーがないため、接続失敗は正常
    assert!(result.await.is_err());
}

#[tokio::test]
async fn test_pool_creation_and_configuration() {
    let config = PoolConfig {
        max_connections: 10,
        min_connections: 2,
        connection_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(300),
        health_check_interval: Duration::from_secs(30),
    };

    let pool = ConnectionPool::new(config.clone());
    assert!(pool.is_ok());

    let pool = pool.unwrap();
    let stats = pool.statistics();
    assert_eq!(stats.total_connections, 0);
    assert_eq!(stats.active_connections, 0);
}

#[tokio::test]
async fn test_pool_statistics_initialization() {
    let config = PoolConfig::default();
    let pool = ConnectionPool::new(config).unwrap();
    let stats = pool.statistics();

    assert_eq!(stats.total_requests, 0);
    assert_eq!(stats.failed_requests, 0);
    assert_eq!(stats.pending_requests, 0);
    assert_eq!(stats.avg_wait_time_ms, 0.0);
}

#[tokio::test]
async fn test_stream_config_defaults() {
    let config = StreamConfig::default();
    assert_eq!(config.chunk_size, 8192);
    assert_eq!(config.max_buffer_size, 1024 * 1024);
    assert!(config.compression_enabled);
}

#[tokio::test]
async fn test_connection_state_transitions() {
    let states = vec![
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
        ConnectionState::Closed,
    ];

    for state in states {
        assert!(matches!(
            state,
            ConnectionState::Disconnected
                | ConnectionState::Connecting
                | ConnectionState::Connected
                | ConnectionState::Reconnecting
                | ConnectionState::Closed
        ));
    }
}

#[tokio::test]
async fn test_websocket_messages() {
    let messages = vec![
        WebSocketMessage::Text("test".to_string()),
        WebSocketMessage::Binary(vec![1, 2, 3]),
        WebSocketMessage::Ping(vec![]),
        WebSocketMessage::Pong(vec![]),
        WebSocketMessage::Close(None),
    ];

    for msg in messages {
        match msg {
            WebSocketMessage::Text(s) => assert_eq!(s, "test"),
            WebSocketMessage::Binary(d) => assert_eq!(d, vec![1, 2, 3]),
            WebSocketMessage::Ping(_) => {}
            WebSocketMessage::Pong(_) => {}
            WebSocketMessage::Close(_) => {}
        }
    }
}

#[tokio::test]
async fn test_close_frame_creation() {
    let frame = CloseFrame {
        code: 1000,
        reason: "Normal closure".to_string(),
    };

    assert_eq!(frame.code, 1000);
    assert_eq!(frame.reason, "Normal closure");
}

#[tokio::test]
async fn test_connection_metrics_initialization() {
    let metrics = ConnectionMetrics {
        connection_id: uuid::Uuid::new_v4().to_string(),
        connected_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        messages_sent: 0,
        messages_received: 0,
        bytes_sent: 0,
        bytes_received: 0,
        error_count: 0,
        avg_response_time_ms: 0.0,
    };

    assert_eq!(metrics.messages_sent, 0);
    assert_eq!(metrics.messages_received, 0);
    assert_eq!(metrics.error_count, 0);
}

#[tokio::test]
async fn test_stream_progress_percentage() {
    let progress = StreamProgress {
        total_bytes: 1000,
        transferred_bytes: 250,
        transfer_rate: 100.0,
        estimated_time_remaining: Some(75.0),
    };

    assert_eq!(progress.percentage(), 0.25);
}

#[tokio::test]
async fn test_stream_progress_percentage_zero_total() {
    let progress = StreamProgress {
        total_bytes: 0,
        transferred_bytes: 100,
        transfer_rate: 100.0,
        estimated_time_remaining: None,
    };

    assert_eq!(progress.percentage(), 0.0);
}

#[tokio::test]
async fn test_health_status_variants() {
    let statuses = [
        HealthStatus::Healthy,
        HealthStatus::Warning,
        HealthStatus::Unhealthy,
        HealthStatus::Unknown,
    ];

    assert_eq!(statuses.len(), 4);
    assert!(matches!(statuses[0], HealthStatus::Healthy));
    assert!(matches!(statuses[1], HealthStatus::Warning));
}

#[tokio::test]
async fn test_pool_config_validation() {
    let config = PoolConfig {
        max_connections: 10,
        min_connections: 2,
        connection_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(300),
        health_check_interval: Duration::from_secs(30),
    };

    assert!(config.max_connections >= config.min_connections);
    assert!(config.connection_timeout.as_secs() > 0);
    assert!(config.idle_timeout >= config.connection_timeout);
}

#[tokio::test]
async fn test_websocket_transport_creation() {
    let pool_config = PoolConfig::default();
    let stream_config = StreamConfig::default();

    let result = WebSocketTransport::new(pool_config, stream_config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_streaming_transport_progress_tracking() {
    let progress = StreamProgress {
        total_bytes: 10_000_000,
        transferred_bytes: 2_500_000,
        transfer_rate: 1_000_000.0,
        estimated_time_remaining: Some(8.0),
    };

    assert_eq!(progress.percentage(), 0.25);
    assert_eq!(progress.transfer_rate, 1_000_000.0);
    assert_eq!(progress.estimated_time_remaining, Some(8.0));
}

#[tokio::test]
async fn test_pool_statistics_tracking() {
    let stats = PoolStatistics {
        total_connections: 10,
        active_connections: 5,
        idle_connections: 5,
        pending_requests: 2,
        total_requests: 1000,
        failed_requests: 10,
        avg_wait_time_ms: 15.5,
    };

    assert_eq!(stats.total_connections, 10);
    assert_eq!(stats.active_connections, 5);
    assert_eq!(stats.idle_connections, 5);
    assert_eq!(stats.pending_requests, 2);
    assert_eq!(stats.total_requests, 1000);
    assert_eq!(stats.failed_requests, 10);
    assert_eq!(stats.avg_wait_time_ms, 15.5);
}

#[tokio::test]
async fn test_connection_health_check_timing() {
    let metrics = ConnectionMetrics {
        connection_id: uuid::Uuid::new_v4().to_string(),
        connected_at: chrono::Utc::now() - chrono::Duration::minutes(10),
        last_active: chrono::Utc::now() - chrono::Duration::minutes(4),
        messages_sent: 100,
        messages_received: 100,
        bytes_sent: 10000,
        bytes_received: 10000,
        error_count: 0,
        avg_response_time_ms: 50.0,
    };

    let idle_duration = chrono::Utc::now()
        .signed_duration_since(metrics.last_active)
        .num_minutes();

    // 4分アイドル状態
    assert!(idle_duration >= 4);
}

#[tokio::test]
async fn test_websocket_message_serialization() {
    let text_msg = WebSocketMessage::Text("hello".to_string());
    let binary_msg = WebSocketMessage::Binary(vec![1, 2, 3, 4]);

    let text_json = serde_json::to_string(&text_msg);
    let binary_json = serde_json::to_string(&binary_msg);

    assert!(text_json.is_ok());
    assert!(binary_json.is_ok());
}

#[tokio::test]
async fn test_stream_config_with_compression() {
    let config = StreamConfig {
        chunk_size: 16384,
        max_buffer_size: 2 * 1024 * 1024,
        compression_enabled: true,
    };

    assert_eq!(config.chunk_size, 16384);
    assert_eq!(config.max_buffer_size, 2 * 1024 * 1024);
    assert!(config.compression_enabled);
}
