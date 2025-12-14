//! WebSocket Connection Pool

use super::connection::WebSocketConnection;
use super::types::*;
pub use super::types::{HealthStatus, PoolConfig, PoolStatistics};
use crate::error::{Error, Result};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{interval, Duration};

/// 接続プール
pub struct ConnectionPool {
    /// プール設定
    config: PoolConfig,
    /// アイドル接続
    idle_connections: Arc<Mutex<VecDeque<WebSocketConnection>>>,
    /// 接続セマフォ
    semaphore: Arc<Semaphore>,
    /// 統計情報
    statistics: Arc<Mutex<PoolStatistics>>,
    /// ベースURL
    base_url: String,
}

impl ConnectionPool {
    /// 新しい接続プールを作成
    pub fn new(config: PoolConfig) -> Result<Self> {
        let statistics = PoolStatistics {
            total_connections: 0,
            active_connections: 0,
            idle_connections: 0,
            pending_requests: 0,
            total_requests: 0,
            failed_requests: 0,
            avg_wait_time_ms: 0.0,
        };

        Ok(Self {
            config: config.clone(),
            idle_connections: Arc::new(Mutex::new(VecDeque::new())),
            semaphore: Arc::new(Semaphore::new(config.max_connections)),
            statistics: Arc::new(Mutex::new(statistics)),
            base_url: "ws://localhost:8080".to_string(), // デフォルト
        })
    }

    /// ベースURLを設定
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// 接続を取得
    pub async fn acquire(&self) -> Result<WebSocketConnection> {
        let start = std::time::Instant::now();

        // セマフォを取得（最大接続数制限）
        let permit = tokio::time::timeout(self.config.connection_timeout, self.semaphore.acquire())
            .await
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::ConnectionError("Failed to acquire semaphore".to_string()))?;

        // アイドル接続から取得を試みる
        let mut idle = self.idle_connections.lock().await;
        let connection = if let Some(conn) = idle.pop_front() {
            // ヘルスチェック
            match conn.health_check().await {
                Ok(HealthStatus::Healthy) => Some(conn),
                _ => None,
            }
        } else {
            None
        };

        drop(idle);

        // アイドル接続がなければ新規作成
        let connection = if let Some(conn) = connection {
            conn
        } else {
            WebSocketConnection::connect(&self.base_url).await?
        };

        // 統計更新
        let mut stats = self.statistics.lock().await;
        stats.total_requests += 1;
        stats.active_connections += 1;
        let wait_time = start.elapsed().as_millis() as f64;
        stats.avg_wait_time_ms = (stats.avg_wait_time_ms * (stats.total_requests - 1) as f64
            + wait_time)
            / stats.total_requests as f64;

        permit.forget(); // セマフォを保持

        Ok(connection)
    }

    /// 接続を返却
    pub async fn release(&mut self, connection: WebSocketConnection) -> Result<()> {
        // ヘルスチェック
        let health = connection.health_check().await?;

        let mut idle = self.idle_connections.lock().await;
        let mut stats = self.statistics.lock().await;

        if health == HealthStatus::Healthy && idle.len() < self.config.max_connections {
            // アイドルプールに返却
            idle.push_back(connection);
            stats.idle_connections = idle.len();
        } else {
            // 接続をクローズ
            let _ = connection.close().await;
        }

        stats.active_connections = stats.active_connections.saturating_sub(1);
        self.semaphore.add_permits(1);

        Ok(())
    }

    /// 統計情報を取得
    pub fn statistics(&self) -> PoolStatistics {
        // Note: 同期的に統計を取得するため、try_lockを使用
        if let Ok(stats) = self.statistics.try_lock() {
            stats.clone()
        } else {
            PoolStatistics {
                total_connections: 0,
                active_connections: 0,
                idle_connections: 0,
                pending_requests: 0,
                total_requests: 0,
                failed_requests: 0,
                avg_wait_time_ms: 0.0,
            }
        }
    }

    /// ヘルスチェックタスクを開始
    pub fn start_health_check(&self) -> tokio::task::JoinHandle<()> {
        let idle_connections = Arc::clone(&self.idle_connections);
        let check_interval = self.config.health_check_interval;

        tokio::spawn(async move {
            let mut interval = interval(check_interval);

            loop {
                interval.tick().await;

                let mut idle = idle_connections.lock().await;
                let mut healthy_connections = VecDeque::new();

                while let Some(conn) = idle.pop_front() {
                    if let Ok(HealthStatus::Healthy) = conn.health_check().await {
                        healthy_connections.push_back(conn);
                    } else {
                        // 不健全な接続はクローズ
                        let _ = conn.close().await;
                    }
                }

                *idle = healthy_connections;
            }
        })
    }

    /// アイドル接続をクリーンアップ
    pub async fn cleanup_idle(&mut self, max_idle_time: Duration) -> usize {
        let mut idle = self.idle_connections.lock().await;
        let mut removed = 0;
        let mut healthy_connections = VecDeque::new();

        while let Some(conn) = idle.pop_front() {
            let metrics = conn.metrics().await;
            let idle_duration = chrono::Utc::now()
                .signed_duration_since(metrics.last_active)
                .num_seconds();

            if idle_duration < max_idle_time.as_secs() as i64 {
                healthy_connections.push_back(conn);
            } else {
                let _ = conn.close().await;
                removed += 1;
            }
        }

        *idle = healthy_connections;
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_creation() {
        let config = PoolConfig {
            max_connections: 10,
            min_connections: 2,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
        };

        let pool = ConnectionPool::new(config);
        assert!(pool.is_ok());
    }

    #[test]
    fn test_pool_statistics() {
        let config = PoolConfig::default();
        let pool = ConnectionPool::new(config).unwrap();
        let stats = pool.statistics();

        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.active_connections, 0);
    }
}
