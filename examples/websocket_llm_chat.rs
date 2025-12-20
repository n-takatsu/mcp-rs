//! WebSocket LLM Chat Server Example
//!
//! LLMストリーミング応答を持つチャットサーバーのサンプル
//!
//! ## 実行方法
//! ```bash
//! cargo run --example websocket_llm_chat
//! ```
//!
//! ## 接続方法
//! ```javascript
//! const ws = new WebSocket('ws://localhost:8081/chat');
//! ws.onmessage = (event) => {
//!     const data = JSON.parse(event.data);
//!     console.log('Chunk:', data.content);
//! };
//! ws.send(JSON.stringify({ message: 'Hello, AI!' }));
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
    CompressionAlgorithm, CompressionConfig, CompressionManager, LlmStreamConfig, LlmStreamer,
    WebSocketMetrics,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

/// チャットリクエスト
#[derive(Debug, Deserialize)]
struct ChatRequest {
    message: String,
    #[serde(default)]
    stream: bool,
}

/// チャットレスポンス
#[derive(Debug, Serialize)]
struct ChatResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// アプリケーション状態
#[derive(Clone)]
struct AppState {
    metrics: Arc<WebSocketMetrics>,
    llm_streamer: Arc<LlmStreamer>,
    compression: Arc<CompressionManager>,
}

#[tokio::main]
async fn main() {
    // ロギング初期化
    tracing_subscriber::fmt::init();

    // メトリクス初期化
    let metrics = Arc::new(WebSocketMetrics::new().expect("Failed to create metrics"));

    // LLMストリーマー初期化
    let llm_config = LlmStreamConfig {
        chunk_size: 20,
        delay_ms: 50,
        ..Default::default()
    };
    let llm_streamer = Arc::new(LlmStreamer::new(llm_config));

    // 圧縮マネージャー初期化（Brotli使用）
    let compression_config = CompressionConfig {
        algorithm: CompressionAlgorithm::Brotli,
        level: 4,
        ..Default::default()
    };
    let compression = Arc::new(
        CompressionManager::new(compression_config).expect("Failed to create compression manager"),
    );

    // 状態初期化
    let state = AppState {
        metrics,
        llm_streamer,
        compression,
    };

    // ルーター構築
    let app = Router::new()
        .route("/chat", get(websocket_handler))
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    // サーバー起動
    let addr = SocketAddr::from(([127, 0, 0, 1], 8081));
    info!("🤖 WebSocket LLM Chat Server listening on {}", addr);
    info!("📊 Health: http://localhost:8081/health");
    info!("📈 Metrics: http://localhost:8081/metrics");
    info!("💬 Chat: ws://localhost:8081/chat");

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
    state.metrics.increment_connections();
    info!("✅ New chat connection");

    // ウェルカムメッセージ
    let welcome = ChatResponse {
        content: Some("🤖 Welcome to LLM Chat! Send a message to start.".to_string()),
        done: Some(false),
        error: None,
    };
    let _ = send_json(&mut socket, &welcome).await;

    // メッセージループ
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                info!("📨 Received: {}", text);
                state.metrics.increment_messages_received();

                // JSONパース
                match serde_json::from_str::<ChatRequest>(&text) {
                    Ok(request) => {
                        if request.stream {
                            // ストリーミングレスポンス
                            handle_streaming_chat(&mut socket, &state, &request.message).await;
                        } else {
                            // 通常レスポンス
                            handle_chat(&mut socket, &state, &request.message).await;
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse request: {}", e);
                        let error_response = ChatResponse {
                            content: None,
                            done: Some(true),
                            error: Some(format!("Invalid JSON: {}", e)),
                        };
                        let _ = send_json(&mut socket, &error_response).await;
                        state.metrics.increment_errors();
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("👋 Client requested close");
                break;
            }
            Ok(_) => {
                // Ping/Pong/Binaryは無視
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                state.metrics.increment_errors();
                break;
            }
        }
    }

    state.metrics.decrement_connections();
    info!("❌ Chat connection closed");
}

/// 通常チャット処理
async fn handle_chat(socket: &mut WebSocket, state: &AppState, message: &str) {
    // シミュレートされたLLM応答
    sleep(Duration::from_millis(500)).await;

    let response_text = format!(
        "You said: '{}'. This is a simulated response from an LLM model.",
        message
    );

    let response = ChatResponse {
        content: Some(response_text),
        done: Some(true),
        error: None,
    };

    if send_json(socket, &response).await.is_ok() {
        state.metrics.increment_messages_sent();
    } else {
        state.metrics.increment_errors();
    }
}

/// ストリーミングチャット処理
async fn handle_streaming_chat(socket: &mut WebSocket, state: &AppState, message: &str) {
    // シミュレートされたLLM応答
    let full_response = format!(
        "You asked: '{}'. Here's a streaming response: \
         This demonstrates how the LLM Streamer works. \
         It breaks down long responses into smaller chunks \
         and sends them progressively to provide a better user experience. \
         The streaming approach allows users to see the response as it's being generated.",
        message
    );

    info!("🔄 Starting streaming response ({} chars)", full_response.len());

    // ストリーミング開始
    match state.llm_streamer.start_stream(&full_response).await {
        Ok(stream_id) => {
            let mut chunk_count = 0;

            // チャンクを順次送信
            loop {
                match state.llm_streamer.next_chunk(&stream_id).await {
                    Ok(Some(chunk)) => {
                        chunk_count += 1;

                        let response = ChatResponse {
                            content: Some(chunk),
                            done: Some(false),
                            error: None,
                        };

                        if send_json(socket, &response).await.is_err() {
                            error!("Failed to send chunk {}", chunk_count);
                            state.metrics.increment_errors();
                            break;
                        }

                        state.metrics.increment_messages_sent();
                    }
                    Ok(None) => {
                        // ストリーミング完了
                        info!("✅ Streaming complete ({} chunks)", chunk_count);

                        let done_response = ChatResponse {
                            content: None,
                            done: Some(true),
                            error: None,
                        };

                        let _ = send_json(socket, &done_response).await;
                        state.metrics.increment_messages_sent();
                        break;
                    }
                    Err(e) => {
                        error!("Streaming error: {}", e);
                        state.metrics.increment_errors();
                        break;
                    }
                }
            }

            // ストリーム終了
            let _ = state.llm_streamer.end_stream(&stream_id).await;
        }
        Err(e) => {
            error!("Failed to start stream: {}", e);
            let error_response = ChatResponse {
                content: None,
                done: Some(true),
                error: Some(format!("Streaming error: {}", e)),
            };
            let _ = send_json(socket, &error_response).await;
            state.metrics.increment_errors();
        }
    }
}

/// JSON応答送信（圧縮付き）
async fn send_json<T: Serialize>(socket: &mut WebSocket, data: &T) -> Result<(), ()> {
    let json = serde_json::to_string(data).map_err(|e| {
        error!("Failed to serialize JSON: {}", e);
    })?;

    socket
        .send(Message::Text(json))
        .await
        .map_err(|e| {
            error!("Failed to send message: {}", e);
        })
}

/// ヘルスチェックエンドポイント
async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot = state.metrics.snapshot();

    let status = serde_json::json!({
        "status": "healthy",
        "connections": snapshot.connections_total,
        "messages_sent": snapshot.messages_sent_total,
        "messages_received": snapshot.messages_received_total,
        "errors": snapshot.errors_total,
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
