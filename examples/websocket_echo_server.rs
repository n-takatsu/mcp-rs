//! WebSocket Echo Server Example
//!
//! 基本的なWebSocketエコーサーバーのサンプル
//!
//! ## 実行方法
//! ```bash
//! cargo run --example websocket_echo_server
//! ```
//!
//! ## 接続方法
//! ブラウザのコンソールまたは `wscat` などのツールで接続:
//! ```javascript
//! const ws = new WebSocket('ws://localhost:8080');
//! ws.onmessage = (event) => console.log('Received:', event.data);
//! ws.send('Hello, Server!');
//! ```

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
    RateLimitConfig, RateLimitStrategy, RateLimiter, WebSocketMetrics,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// アプリケーション状態
#[derive(Clone)]
struct AppState {
    metrics: Arc<WebSocketMetrics>,
    rate_limiter: Arc<RateLimiter>,
    active_connections: Arc<Mutex<usize>>,
}

#[tokio::main]
async fn main() {
    // ロギング初期化
    tracing_subscriber::fmt::init();

    // メトリクス初期化
    let metrics = Arc::new(WebSocketMetrics::new().expect("Failed to create metrics"));

    // レート制限初期化（100 req/sec）
    let rate_config = RateLimitConfig {
        strategy: RateLimitStrategy::TokenBucket,
        max_requests_per_second: 100,
        max_burst: 200,
        window_size_ms: 1000,
    };
    let rate_limiter = Arc::new(RateLimiter::new(rate_config));

    // 状態初期化
    let state = AppState {
        metrics,
        rate_limiter,
        active_connections: Arc::new(Mutex::new(0)),
    };

    // ルーター構築
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    // サーバー起動
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    info!("🚀 WebSocket Echo Server listening on {}", addr);
    info!("📊 Health: http://localhost:8080/health");
    info!("📈 Metrics: http://localhost:8080/metrics");
    info!("🔌 WebSocket: ws://localhost:8080/ws");

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
    // 接続カウント増加
    {
        let mut count = state.active_connections.lock().await;
        *count += 1;
        state.metrics.increment_connections();
        info!("✅ New connection (total: {})", *count);
    }

    // ウェルカムメッセージ
    if socket
        .send(Message::Text(
            "Welcome to WebSocket Echo Server! 🎉".into(),
        ))
        .await
        .is_err()
    {
        warn!("Failed to send welcome message");
        return;
    }

    // メッセージループ
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                info!("📨 Received: {}", text);

                // レート制限チェック
                match state.rate_limiter.check_global_rate_limit().await {
                    Ok(allowed) if allowed => {
                        state.metrics.increment_messages_received();

                        // エコーバック
                        let echo = format!("Echo: {}", text);
                        if socket.send(Message::Text(echo.into())).await.is_err() {
                            error!("Failed to send echo");
                            break;
                        }

                        state.metrics.increment_messages_sent();
                    }
                    Ok(_) => {
                        // レート制限超過
                        let msg = "⚠️  Rate limit exceeded. Please slow down.";
                        let _ = socket.send(Message::Text(msg.into())).await;
                        warn!("Rate limit exceeded");
                    }
                    Err(e) => {
                        error!("Rate limit check error: {}", e);
                        state.metrics.increment_errors();
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                info!("📦 Received binary data: {} bytes", data.len());
                // バイナリもエコーバック
                if socket.send(Message::Binary(data)).await.is_err() {
                    error!("Failed to send binary echo");
                    break;
                }
            }
            Ok(Message::Ping(data)) => {
                // Pongで応答
                if socket.send(Message::Pong(data)).await.is_err() {
                    error!("Failed to send pong");
                    break;
                }
            }
            Ok(Message::Pong(_)) => {
                // Pong受信（何もしない）
            }
            Ok(Message::Close(_)) => {
                info!("👋 Client requested close");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                state.metrics.increment_errors();
                break;
            }
        }
    }

    // 接続カウント減少
    {
        let mut count = state.active_connections.lock().await;
        *count -= 1;
        state.metrics.decrement_connections();
        info!("❌ Connection closed (remaining: {})", *count);
    }
}

/// ヘルスチェックエンドポイント
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let count = state.active_connections.lock().await;
    let snapshot = state.metrics.snapshot();

    let status = serde_json::json!({
        "status": "healthy",
        "active_connections": *count,
        "total_messages_sent": snapshot.messages_sent_total,
        "total_messages_received": snapshot.messages_received_total,
        "total_errors": snapshot.errors_total,
    });

    axum::Json(status)
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
