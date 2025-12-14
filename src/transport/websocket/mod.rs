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
use std::sync::Arc;
use tokio::sync::RwLock;

/// WebSocketトランスポートマネージャー
pub struct WebSocketTransport {
    /// 接続プール
    pool: Arc<RwLock<ConnectionPool>>,
    /// ストリーミング設定
    stream_config: StreamConfig,
}

impl WebSocketTransport {
    /// 新しいWebSocketトランスポートを作成
    pub fn new(pool_config: PoolConfig, stream_config: StreamConfig) -> Result<Self> {
        let pool = ConnectionPool::new(pool_config)?;

        Ok(Self {
            pool: Arc::new(RwLock::new(pool)),
            stream_config,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
