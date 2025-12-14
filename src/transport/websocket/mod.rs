//! WebSocket Transport Module
//!
//! WebSocketベースのリアルタイム双方向通信を提供

pub mod connection;
pub mod pool;
pub mod stream;
pub mod types;

pub use connection::{WebSocketConnection, WebSocketConnectionBuilder};
pub use pool::ConnectionPool;
pub use stream::StreamingTransport;
pub use types::*;

use crate::error::{Error, Result};
use crate::transport::{ConnectionStats, Transport, TransportInfo};
use crate::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// WebSocketトランスポートマネージャー
#[derive(Debug)]
pub struct WebSocketTransport {
    /// 接続プール
    pool: Arc<RwLock<ConnectionPool>>,
    /// ストリーミング設定
    stream_config: StreamConfig,
    /// アクティブな接続
    active_connection: Arc<Mutex<Option<WebSocketConnection>>>,
    /// 接続URL
    url: String,
    /// 起動状態
    running: Arc<Mutex<bool>>,
}

impl WebSocketTransport {
    /// 新しいWebSocketトランスポートを作成
    pub fn new(pool_config: PoolConfig, stream_config: StreamConfig) -> Result<Self> {
        let pool = ConnectionPool::new(pool_config)?;

        Ok(Self {
            pool: Arc::new(RwLock::new(pool)),
            stream_config,
            active_connection: Arc::new(Mutex::new(None)),
            url: "ws://localhost:8080".to_string(),
            running: Arc::new(Mutex::new(false)),
        })
    }

    /// URLを設定
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// 接続を取得
    pub async fn get_connection(&self) -> Result<WebSocketConnection> {
        let pool = self.pool.read().await;
        pool.acquire().await
    }

    /// 接続をプールに返却
    pub async fn return_connection(&self, conn: WebSocketConnection) -> Result<()> {
        let mut pool = self.pool.write().await;
        pool.release(conn).await
    }

    /// プール統計を取得
    pub async fn get_statistics(&self) -> PoolStatistics {
        let pool = self.pool.read().await;
        pool.statistics()
    }

    /// ストリーミングトランスポートを作成
    pub async fn create_streaming_transport(&self) -> Result<StreamingTransport> {
        let connection = self.get_connection().await?;
        Ok(StreamingTransport::new(
            connection,
            self.stream_config.clone(),
        ))
    }
}

/// Transport trait実装
#[async_trait]
impl Transport for WebSocketTransport {
    type Error = Error;

    async fn start(&mut self) -> std::result::Result<(), Self::Error> {
        let mut running = self.running.lock().await;
        if *running {
            return Ok(());
        }

        // 接続を確立
        let connection = WebSocketConnection::connect(&self.url).await?;
        let mut active = self.active_connection.lock().await;
        *active = Some(connection);
        *running = true;

        Ok(())
    }

    async fn stop(&mut self) -> std::result::Result<(), Self::Error> {
        let mut running = self.running.lock().await;
        if !*running {
            return Ok(());
        }

        // アクティブな接続をクローズ
        let mut active = self.active_connection.lock().await;
        if let Some(conn) = active.take() {
            conn.close().await?;
        }
        *running = false;

        Ok(())
    }

    async fn send_message(
        &mut self,
        message: JsonRpcResponse,
    ) -> std::result::Result<(), Self::Error> {
        let active = self.active_connection.lock().await;
        if let Some(conn) = active.as_ref() {
            let json = serde_json::to_string(&message)?;
            conn.send(WebSocketMessage::Text(json)).await?;
            Ok(())
        } else {
            Err(Error::ConnectionError("Not connected".to_string()))
        }
    }

    async fn receive_message(
        &mut self,
    ) -> std::result::Result<Option<JsonRpcRequest>, Self::Error> {
        let active = self.active_connection.lock().await;
        if let Some(conn) = active.as_ref() {
            match conn.receive().await? {
                Some(WebSocketMessage::Text(text)) => {
                    let request: JsonRpcRequest = serde_json::from_str(&text)?;
                    Ok(Some(request))
                }
                Some(WebSocketMessage::Close(_)) => Ok(None),
                Some(_) => Ok(None), // バイナリやPing/Pongは無視
                None => Ok(None),
            }
        } else {
            Err(Error::ConnectionError("Not connected".to_string()))
        }
    }

    fn is_connected(&self) -> bool {
        // Note: Mutexをロックできないため、簡易実装
        // 実際の接続状態確認は非同期で行う
        true
    }

    fn transport_info(&self) -> TransportInfo {
        TransportInfo {
            transport_type: crate::transport::TransportType::WebSocket {
                url: self.url.clone(),
            },
            description: "WebSocket Transport with connection pooling".to_string(),
            capabilities: crate::transport::TransportCapabilities {
                bidirectional: true,
                multiplexing: true,
                compression: false,
                max_message_size: None,
                framing_methods: vec![crate::transport::FramingMethod::WebSocketFrame],
            },
        }
    }

    fn connection_stats(&self) -> ConnectionStats {
        // Note: 非同期統計取得が必要なため、簡易実装
        ConnectionStats {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            uptime: std::time::Duration::from_secs(0),
            last_activity: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Transport;

    #[tokio::test]
    async fn test_websocket_transport_creation() {
        let pool_config = PoolConfig {
            max_connections: 10,
            min_connections: 2,
            connection_timeout: std::time::Duration::from_secs(5),
            idle_timeout: std::time::Duration::from_secs(300),
            health_check_interval: std::time::Duration::from_secs(30),
        };

        let stream_config = StreamConfig {
            chunk_size: 8192,
            max_buffer_size: 1024 * 1024,
            compression_enabled: true,
        };

        let transport = WebSocketTransport::new(pool_config, stream_config);
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_transport_trait_implementation() {
        let pool_config = PoolConfig {
            max_connections: 10,
            min_connections: 2,
            connection_timeout: std::time::Duration::from_secs(5),
            idle_timeout: std::time::Duration::from_secs(300),
            health_check_interval: std::time::Duration::from_secs(30),
        };

        let stream_config = StreamConfig {
            chunk_size: 8192,
            max_buffer_size: 1024 * 1024,
            compression_enabled: true,
        };

        let transport = WebSocketTransport::new(pool_config, stream_config)
            .unwrap()
            .with_url("ws://localhost:8080");

        // Transport traitメソッドのテスト
        let info = transport.transport_info();
        assert_eq!(info.description, "WebSocket Transport with connection pooling");
        assert!(info.capabilities.bidirectional);
        assert!(info.capabilities.multiplexing);

        let stats = transport.connection_stats();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
    }
}
