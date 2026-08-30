//! WebSocket Transport Module
//!
//! WebSocketベースのリアルタイム双方向通信を提供

pub mod balancer;
pub mod compression;
pub mod connection;
pub mod failover;
pub mod jsonrpc;
pub mod llm_bridge;
pub mod metrics;
pub mod pool;
pub mod rate_limit;
pub mod server;
pub mod stream;
pub mod transfer;
pub mod types;

pub use balancer::{
    BalancerConfig, BalancerManager, BalancerStats, BalancingStrategy, Endpoint, EndpointStats,
    LoadBalancer,
};
pub use compression::{CompressionConfig, CompressionManager, CompressionStats};
pub use connection::{WebSocketConnection, WebSocketConnectionBuilder};
pub use failover::{
    Failover, FailoverConfig, FailoverEvent, FailoverManager, FailoverStatus, SessionState,
};
pub use jsonrpc::{error_codes, JsonRpcMessage, JsonRpcNotification};
pub use llm_bridge::{
    AnthropicBridge, LlmBridge, LlmBridgeFactory, LlmConfig, LlmProvider, OpenAiBridge, StreamChunk,
};
pub use metrics::{MetricsSnapshot, WebSocketMetrics};
pub use pool::{ConnectionPool, PoolMetrics};
pub use rate_limit::{LimiterStats, RateLimitConfig, RateLimitStrategy, RateLimiter};
pub use server::{
    ConnectionId, EchoHandler, MessageHandler, ServerConfig, ServerStatistics, WebSocketServer,
};
pub use stream::StreamingTransport;
pub use transfer::{
    CompressionType, FileChunk, FileTransferProtocol, TransferManager, TransferOptions,
    TransferProgress, TransferState,
};
pub use types::{PoolStatistics, WebSocketConfig, WebSocketConfigBuilder};

use crate::error::{Error, Result};
use crate::transport::{ConnectionStats, Transport, TransportInfo};
use crate::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// WebSocketトランスポートマネージャー
pub struct WebSocketTransport {
    /// 設定
    config: WebSocketConfig,
    /// 接続プール
    pool: Arc<RwLock<ConnectionPool>>,
    /// サーバーインスタンス（サーバーモード時）
    server: Arc<Mutex<Option<WebSocketServer>>>,
    /// アクティブな接続
    active_connection: Arc<Mutex<Option<WebSocketConnection>>>,
    /// 起動状態
    running: Arc<Mutex<bool>>,
}

impl std::fmt::Debug for WebSocketTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketTransport")
            .field("config", &self.config)
            .field("pool", &self.pool)
            .field("running", &self.running)
            .finish()
    }
}

impl WebSocketTransport {
    /// 新しいWebSocketトランスポートを作成
    pub fn new(config: WebSocketConfig) -> Result<Self> {
        let pool = ConnectionPool::new(config.pool_config.clone())?;

        Ok(Self {
            config,
            pool: Arc::new(RwLock::new(pool)),
            server: Arc::new(Mutex::new(None)),
            active_connection: Arc::new(Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
        })
    }

    /// ビルダーを作成
    pub fn builder() -> WebSocketConfigBuilder {
        WebSocketConfigBuilder::new()
    }

    /// 設定から作成
    pub fn from_config(config: WebSocketConfig) -> Result<Self> {
        Self::new(config)
    }

    /// 設定を取得
    pub fn config(&self) -> &WebSocketConfig {
        &self.config
    }

    /// サーバーモードかどうか
    pub fn is_server_mode(&self) -> bool {
        self.config.server_mode
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
            self.config.stream_config.clone(),
        ))
    }
    /// JSON-RPC通知を送信
    pub async fn send_notification(&self, notification: JsonRpcNotification) -> Result<()> {
        let active = self.active_connection.lock().await;
        if let Some(conn) = active.as_ref() {
            let jsonrpc_msg = JsonRpcMessage::Notification(notification);
            let ws_msg = jsonrpc_msg.to_websocket()?;
            conn.send(ws_msg).await?;
            Ok(())
        } else {
            Err(Error::ConnectionError("Not connected".to_string()))
        }
    }

    /// JSON-RPCメッセージを直接送信（汎用）
    pub async fn send_jsonrpc(&self, message: JsonRpcMessage) -> Result<()> {
        let active = self.active_connection.lock().await;
        if let Some(conn) = active.as_ref() {
            let ws_msg = message.to_websocket()?;
            conn.send(ws_msg).await?;
            Ok(())
        } else {
            Err(Error::ConnectionError("Not connected".to_string()))
        }
    }

    /// JSON-RPCメッセージを受信（汎用）
    pub async fn receive_jsonrpc(&self) -> Result<Option<JsonRpcMessage>> {
        let active = self.active_connection.lock().await;
        if let Some(conn) = active.as_ref() {
            match conn.receive().await? {
                Some(ws_msg) => Ok(Some(JsonRpcMessage::from_websocket(ws_msg)?)),
                None => Ok(None),
            }
        } else {
            Err(Error::ConnectionError("Not connected".to_string()))
        }
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

        if self.config.server_mode {
            // サーバーモード：WebSocketサーバーを起動
            // URLからSocketAddrをパース (ws://host:port -> host:port)
            let addr_str = self
                .config
                .url
                .trim_start_matches("ws://")
                .trim_start_matches("wss://");
            let bind_addr: SocketAddr = addr_str
                .parse()
                .map_err(|e| Error::ConnectionError(format!("Invalid bind address: {}", e)))?;

            let server_config = ServerConfig {
                bind_addr,
                max_connections: self.config.max_connections,
                max_message_size: self.config.max_message_size,
                ping_interval: std::time::Duration::from_secs(self.config.heartbeat_interval),
                timeout: std::time::Duration::from_secs(self.config.timeout_seconds.unwrap_or(30)),
                network_policy: crate::security::NetworkPolicy::default(),
                anti_replay_enabled: self.config.anti_replay_enabled,
            };

            let mut server = WebSocketServer::new(server_config);
            server.start().await?;

            let mut srv = self.server.lock().await;
            *srv = Some(server);
        } else {
            // クライアントモード：接続を確立
            let connection = WebSocketConnection::connect(&self.config.url).await?;
            let mut active = self.active_connection.lock().await;
            *active = Some(connection);
        }

        *running = true;
        Ok(())
    }

    async fn stop(&mut self) -> std::result::Result<(), Self::Error> {
        let mut running = self.running.lock().await;
        if !*running {
            return Ok(());
        }

        if self.config.server_mode {
            // サーバーモード：サーバーを停止
            let mut server = self.server.lock().await;
            if let Some(mut srv) = server.take() {
                srv.stop().await?;
            }
        } else {
            // クライアントモード：アクティブな接続をクローズ
            let mut active = self.active_connection.lock().await;
            if let Some(conn) = active.take() {
                conn.close().await?;
            }
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
            // JsonRpcMessage経由で変換
            let jsonrpc_msg = JsonRpcMessage::Response(message);
            let ws_msg = jsonrpc_msg.to_websocket()?;
            conn.send(ws_msg).await?;
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
                Some(ws_msg) => match JsonRpcMessage::from_websocket(ws_msg) {
                    Ok(JsonRpcMessage::Request(request)) => Ok(Some(request)),
                    Ok(JsonRpcMessage::Notification(_)) => {
                        // 通知はリクエストとして扱わない
                        Ok(None)
                    }
                    Ok(JsonRpcMessage::Response(_)) => {
                        // レスポンスは無視（サーバー側）
                        Ok(None)
                    }
                    Err(e) => Err(e),
                },
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
                url: self.config.url.clone(),
            },
            description: if self.config.server_mode {
                format!(
                    "WebSocket Server with connection pooling (max: {})",
                    self.config.max_connections
                )
            } else {
                "WebSocket Client with connection pooling".to_string()
            },
            capabilities: crate::transport::TransportCapabilities {
                bidirectional: true,
                multiplexing: true,
                compression: self.config.stream_config.compression_enabled,
                max_message_size: Some(self.config.max_message_size),
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
        let config = WebSocketConfig::default();
        let transport = WebSocketTransport::new(config);
        assert!(transport.is_ok());
    }

    #[tokio::test]
    async fn test_transport_builder() {
        let config = WebSocketConfigBuilder::new()
            .url("ws://localhost:8080")
            .server_mode(true)
            .max_connections(100)
            .timeout(30)
            .build();

        let transport = WebSocketTransport::from_config(config).unwrap();
        assert!(transport.is_server_mode());
    }

    #[tokio::test]
    async fn test_transport_trait_implementation() {
        let config = WebSocketConfigBuilder::new()
            .url("ws://localhost:8080")
            .build();

        let transport = WebSocketTransport::new(config).unwrap();

        // Transport traitメソッドのテスト
        let info = transport.transport_info();
        assert!(info.description.contains("WebSocket"));
        assert!(info.capabilities.bidirectional);
        assert!(info.capabilities.multiplexing);

        let stats = transport.connection_stats();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
    }
}
