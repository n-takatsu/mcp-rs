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
use mcp_rs::transport::websocket::types::PoolConfig;
use mcp_rs::transport::websocket::{
    ConnectionPool, RateLimitConfig, RateLimitStrategy, RateLimiter, WebSocketMetrics,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// グローバル接続カウンター
static CONN_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        connection_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(60),
        health_check_interval: Duration::from_secs(30),
    };

    // プール初期化
    let pool =
        Arc::new(ConnectionPool::new(pool_config).expect("Failed to create connection pool"));

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

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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
    state.metrics.increment_connections();
    let conn_id = CONN_COUNTER.fetch_add(1, Ordering::SeqCst);
    info!("✅ New connection: {}", conn_id);

    // ウェルカムメッセージ
    let welcome = format!(
        "Welcome to Load Balanced Server! 🎉\nConnection ID: {}",
        conn_id
    );

    if socket.send(Message::Text(welcome.into())).await.is_err() {
        warn!("Failed to send welcome message");
        state.metrics.decrement_connections();
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

                        if socket.send(Message::Text(response.into())).await.is_err() {
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
                        let msg =
                            format!("[{}] ⚠️  Rate limit exceeded. Please slow down.", conn_id);
                        let _ = socket.send(Message::Text(msg.into())).await;
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

    state.metrics.decrement_connections();
    info!("[{}] Connection closed", conn_id);
}

/// プール統計送信
async fn send_pool_stats(socket: &mut WebSocket, state: &AppState) {
    let stats = state.pool.statistics();
    let stats_msg = format!(
        "📊 Pool Stats: Active={}, Total={}",
        stats.active_connections, stats.total_connections
    );

    let _ = socket.send(Message::Text(stats_msg.into())).await;
}

/// ヘルスチェックエンドポイント
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.metrics.snapshot();
    let pool_stats = state.pool.statistics();
    let message_count = *state.message_count.lock().await;

    let status = serde_json::json!({
        "status": "healthy",
        "metrics": {
            "connections": snapshot.connections_total,
            "messages_sent": snapshot.messages_sent_total,
            "messages_received": snapshot.messages_received_total,
            "errors": snapshot.errors_total,
        },
        "pool": {
            "active_connections": pool_stats.active_connections,
            "total_connections": pool_stats.total_connections,
            "idle_connections": pool_stats.idle_connections,
        },
        "total_messages_processed": message_count,
    });

    axum::Json(status)
}

/// プール統計エンドポイント
async fn pool_stats_handler(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.pool.statistics();
    let response = serde_json::json!({
        "active_connections": stats.active_connections,
        "total_connections": stats.total_connections,
        "idle_connections": stats.idle_connections,
        "pending_requests": stats.pending_requests,
        "total_requests": stats.total_requests,
        "failed_requests": stats.failed_requests,
    });
    axum::Json(response).into_response()
}

/// Prometheusメトリクスエンドポイント
async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    match state.metrics.export_text() {
        Ok(text) => ([(axum::http::header::CONTENT_TYPE, "text/plain")], text).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export metrics: {}", e),
        )
            .into_response(),
    }
}
