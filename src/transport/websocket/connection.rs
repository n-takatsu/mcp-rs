//! WebSocket Connection Management

use super::types::*;
use crate::error::{Error, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

/// WebSocket接続
#[derive(Debug)]
pub struct WebSocketConnection {
    /// 接続ID
    id: String,
    /// WebSocketストリーム
    #[allow(dead_code)]
    stream: Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    /// 接続状態
    state: Arc<RwLock<ConnectionState>>,
    /// メトリクス
    metrics: Arc<RwLock<ConnectionMetrics>>,
}

impl WebSocketConnection {
    /// 新しい接続を作成
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws_stream, _) = connect_async(url)
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        let connection_id = uuid::Uuid::new_v4().to_string();

        let metrics = ConnectionMetrics {
            connection_id: connection_id.clone(),
            connected_at: Utc::now(),
            last_active: Utc::now(),
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            error_count: 0,
            avg_response_time_ms: 0.0,
        };

        Ok(Self {
            id: connection_id,
            stream: Arc::new(Mutex::new(ws_stream)),
            state: Arc::new(RwLock::new(ConnectionState::Connected)),
            metrics: Arc::new(RwLock::new(metrics)),
        })
    }

    /// メッセージを送信
    pub async fn send(&self, message: WebSocketMessage) -> Result<()> {
        use futures_util::SinkExt;

        let tungstenite_msg = match message {
            WebSocketMessage::Text(text) => Message::Text(text.into()),
            WebSocketMessage::Binary(data) => Message::Binary(data.into()),
            WebSocketMessage::Ping(data) => Message::Ping(data.into()),
            WebSocketMessage::Pong(data) => Message::Pong(data.into()),
            WebSocketMessage::Close(frame) => {
                if let Some(frame) = frame {
                    Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                        code:
                            tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(
                                frame.code,
                            ),
                        reason: frame.reason.into(),
                    }))
                } else {
                    Message::Close(None)
                }
            }
        };

        let mut stream = self.stream.lock().await;
        stream
            .send(tungstenite_msg)
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        // メトリクス更新
        let mut metrics = self.metrics.write().await;
        metrics.messages_sent += 1;
        metrics.last_active = Utc::now();

        Ok(())
    }

    /// メッセージを受信
    pub async fn receive(&self) -> Result<Option<WebSocketMessage>> {
        use futures_util::StreamExt;

        let mut stream = self.stream.lock().await;

        match stream.next().await {
            Some(Ok(msg)) => {
                let ws_msg = match msg {
                    Message::Text(text) => Some(WebSocketMessage::Text(text.to_string())),
                    Message::Binary(data) => Some(WebSocketMessage::Binary(data.to_vec())),
                    Message::Ping(data) => Some(WebSocketMessage::Ping(data.to_vec())),
                    Message::Pong(data) => Some(WebSocketMessage::Pong(data.to_vec())),
                    Message::Close(frame) => {
                        let close_frame = frame.map(|f| CloseFrame {
                            code: f.code.into(),
                            reason: f.reason.to_string(),
                        });
                        Some(WebSocketMessage::Close(close_frame))
                    }
                    Message::Frame(_) => None,
                };

                // メトリクス更新
                if ws_msg.is_some() {
                    let mut metrics = self.metrics.write().await;
                    metrics.messages_received += 1;
                    metrics.last_active = Utc::now();
                }

                Ok(ws_msg)
            }
            Some(Err(e)) => {
                let mut metrics = self.metrics.write().await;
                metrics.error_count += 1;
                Err(Error::ConnectionError(e.to_string()))
            }
            None => Ok(None),
        }
    }

    /// 接続をクローズ
    pub async fn close(&self) -> Result<()> {
        use futures_util::SinkExt;

        let mut stream = self.stream.lock().await;
        stream
            .close(None)
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        let mut state = self.state.write().await;
        *state = ConnectionState::Closed;

        Ok(())
    }

    /// 接続ID取得
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 接続状態取得
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// メトリクス取得
    pub async fn metrics(&self) -> ConnectionMetrics {
        self.metrics.read().await.clone()
    }

    /// ヘルスチェック
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let state = self.state().await;
        let metrics = self.metrics().await;

        if state != ConnectionState::Connected {
            return Ok(HealthStatus::Unhealthy);
        }

        // 最終アクティブから5分以上経過していたら警告
        let idle_duration = Utc::now()
            .signed_duration_since(metrics.last_active)
            .num_seconds();

        if idle_duration > 300 {
            Ok(HealthStatus::Warning)
        } else {
            Ok(HealthStatus::Healthy)
        }
    }

    /// ビルダーを取得
    pub fn builder() -> WebSocketConnectionBuilder {
        WebSocketConnectionBuilder::new("")
    }
}

/// WebSocket接続ビルダー
pub struct WebSocketConnectionBuilder {
    url: String,
    timeout: Option<std::time::Duration>,
}

impl WebSocketConnectionBuilder {
    /// 新しいビルダーを作成
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            timeout: None,
        }
    }

    /// URLを設定
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// タイムアウトを設定
    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 接続を構築
    pub async fn build(self) -> Result<WebSocketConnection> {
        if let Some(timeout) = self.timeout {
            tokio::time::timeout(timeout, WebSocketConnection::connect(&self.url))
                .await
                .map_err(|_| Error::Timeout)?
        } else {
            WebSocketConnection::connect(&self.url).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_builder() {
        let builder = WebSocketConnectionBuilder::new("ws://localhost:8080")
            .timeout(std::time::Duration::from_secs(5));

        assert_eq!(builder.url, "ws://localhost:8080");
        assert!(builder.timeout.is_some());
    }
}
