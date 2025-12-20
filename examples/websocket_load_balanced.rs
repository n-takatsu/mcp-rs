//! WebSocket Load Balanced Server Example
//!
//! 負荷分散・フェイルオーバー機能を持つWebSocketサーバーのサンプル
//!
//! ## 実行方法
//! ```bash
//! cargo run --example websocket_load_balanced
//! ```
//!
//! ## 機能
//! - コネクションプール（自動スケーリング）
//! - 負荷分散（RoundRobin/LeastConnections/Random）
//! - フェイルオーバー（自動リトライ）
//! - レート制限（TokenBucket）
//! - メトリクス収集

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use mcp_rs::transport::websocket::{
    ConnectionPool, FailoverConfig, LoadBalanceStrategy, PoolConfig, RateLimitConfig,
    RateLimitStrategy, RateLimiter, WebSocketMetrics,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// アプリケーション状態
#[derive(Clone)]
struct AppState {
    pool: Arc<ConnectionPool>,
    metrics: Arc<WebSocketMetrics>,
    rate_limiter: Arc<RateLimiter>,
    message_count: Arc<Mutex<u64>>,
}

#[tokio::main]
async fn main() {
    // ロギング初期化
    tracing_subscriber::fmt::init();

    // メトリクス初期化
    let metrics = Arc::new(WebSocketMetrics::new().expect("Failed to create metrics"));

    // コネクションプール設定
    let pool_config = PoolConfig {
        min_connections: 2,
        max_connections: 10,
        load_balance_strategy: LoadBalanceStrategy::LeastConnections,
        enable_auto_scaling: true,
        scale_up_threshold: 0.8,
        scale_down_threshold: 0.2,
        health_check_interval_ms: 5000,
    };

    // フェイルオーバー設定
    let failover_config = FailoverConfig {
        max_retries: 3,
        retry_delay_ms: 100,
        enable_circuit_breaker: true,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 30000,
    };

    // プール初期化
    let pool = Arc::new(
        ConnectionPool::new_with_failover(pool_config, failover_config)
            .expect("Failed to create connection pool"),
    );

    // レート制限設定（1000 req/sec）
    let rate_config = RateLimitConfig {
        strategy: RateLimitStrategy::TokenBucket,
        max_requests_per_second: 1000,
        max_burst: 2000,
        window_size_ms: 1000,
    };
    let rate_limiter = Arc::new(RateLimiter::new(rate_config));

    // 状態初期化
    let state = AppState {
        pool,
        metrics,
        rate_limiter,
        message_count: Arc::new(Mutex::new(0)),
    };

    // ルーター構築
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .route("/pool/stats", get(pool_stats_handler))
        .with_state(state);

    // サーバー起動
    let addr = SocketAddr::from(([127, 0, 0, 1], 8082));
    info!("⚖️  WebSocket Load Balanced Server listening on {}", addr);
    info!("📊 Health: http://localhost:8082/health");
    info!("📈 Metrics: http://localhost:8082/metrics");
    info!("🔌 WebSocket: ws://localhost:8082/ws");
    info!("📊 Pool Stats: http://localhost:8082/pool/stats");

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

/// WebSocketアップグレードハンドラー
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// WebSocketメッセージハンドラー
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    // 接続追加
    match state.pool.add_connection().await {
        Ok(conn_id) => {
            state.metrics.increment_connections();
            info!("✅ New connection added to pool: {}", conn_id);

            // ウェルカムメッセージ
            let welcome = format!(
                "Welcome to Load Balanced Server! 🎉\nConnection ID: {}\nStrategy: {:?}",
                conn_id,
                state.pool.get_stats().await.unwrap().load_balance_strategy
            );

            if socket.send(Message::Text(welcome)).await.is_err() {
                warn!("Failed to send welcome message");
                let _ = state.pool.remove_connection(&conn_id).await;
                return;
            }

            // メッセージループ
            while let Some(msg) = socket.recv().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        info!("📨 [{}] Received: {}", conn_id, text);

                        // レート制限チェック
                        match state.rate_limiter.check_global_rate_limit().await {
                            Ok(allowed) if allowed => {
                                state.metrics.increment_messages_received();

                                // メッセージカウント更新
                                let mut count = state.message_count.lock().await;
                                *count += 1;

                                // レスポンス作成
                                let response = format!(
                                    "[Connection {}] Message #{} processed: {}",
                                    conn_id, *count, text
                                );

                                if socket.send(Message::Text(response)).await.is_err() {
                                    error!("[{}] Failed to send response", conn_id);
                                    break;
                                }

                                state.metrics.increment_messages_sent();

                                // プール統計を定期的に送信
                                if *count % 10 == 0 {
                                    send_pool_stats(&mut socket, &state).await;
                                }
                            }
                            Ok(_) => {
                                // レート制限超過
                                let msg = format!(
                                    "[{}] ⚠️  Rate limit exceeded. Please slow down.",
                                    conn_id
                                );
                                let _ = socket.send(Message::Text(msg)).await;
                                warn!("[{}] Rate limit exceeded", conn_id);
                            }
                            Err(e) => {
                                error!("[{}] Rate limit check error: {}", conn_id, e);
                                state.metrics.increment_errors();
                            }
                        }
                    }
                    Ok(Message::Binary(data)) => {
                        info!("[{}] Received binary: {} bytes", conn_id, data.len());
                        if socket.send(Message::Binary(data)).await.is_err() {
                            error!("[{}] Failed to send binary", conn_id);
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("[{}] Client requested close", conn_id);
                        break;
                    }
                    Ok(_) => {
                        // Ping/Pong（何もしない）
                    }
                    Err(e) => {
                        error!("[{}] WebSocket error: {}", conn_id, e);
                        state.metrics.increment_errors();
                        break;
                    }
                }
            }

            // 接続削除
            if let Err(e) = state.pool.remove_connection(&conn_id).await {
                error!("[{}] Failed to remove connection: {}", conn_id, e);
            }

            state.metrics.decrement_connections();
            info!("[{}] Connection closed", conn_id);
        }
        Err(e) => {
            error!("Failed to add connection to pool: {}", e);
            state.metrics.increment_errors();
            let _ = socket
                .send(Message::Text(format!("Error: {}", e)))
                .await;
        }
    }
}

/// プール統計送信
async fn send_pool_stats(socket: &mut WebSocket, state: &AppState) {
    if let Ok(stats) = state.pool.get_stats().await {
        let stats_msg = format!(
            "📊 Pool Stats: Active={}/{}, Load={:.1}%, Strategy={:?}",
            stats.active_connections,
            stats.max_connections,
            stats.load_percentage * 100.0,
            stats.load_balance_strategy
        );

        let _ = socket.send(Message::Text(stats_msg)).await;
    }
}

/// ヘルスチェックエンドポイント
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.metrics.snapshot();
    let pool_stats = state.pool.get_stats().await;
    let message_count = *state.message_count.lock().await;

    let status = serde_json::json!({
        "status": "healthy",
        "metrics": {
            "connections": snapshot.connections_total,
            "messages_sent": snapshot.messages_sent_total,
            "messages_received": snapshot.messages_received_total,
            "errors": snapshot.errors_total,
        },
        "pool": pool_stats.map(|s| serde_json::json!({
            "active_connections": s.active_connections,
            "max_connections": s.max_connections,
            "load_percentage": s.load_percentage,
            "strategy": format!("{:?}", s.load_balance_strategy),
        })),
        "total_messages_processed": message_count,
    });

    axum::Json(status)
}

/// プール統計エンドポイント
async fn pool_stats_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.pool.get_stats().await {
        Ok(stats) => {
            let response = serde_json::json!({
                "active_connections": stats.active_connections,
                "max_connections": stats.max_connections,
                "min_connections": stats.min_connections,
                "load_percentage": stats.load_percentage,
                "load_balance_strategy": format!("{:?}", stats.load_balance_strategy),
                "auto_scaling_enabled": stats.auto_scaling_enabled,
            });
            axum::Json(response).into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get pool stats: {}", e),
        )
            .into_response(),
    }
}

/// Prometheusメトリクスエンドポイント
async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.metrics.export_text() {
        Ok(text) => (
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            text,
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export metrics: {}", e),
        )
            .into_response(),
    }
}
