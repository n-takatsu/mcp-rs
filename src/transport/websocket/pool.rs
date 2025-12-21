//! WebSocket Connection Pool

use super::connection::WebSocketConnection;
use super::types::*;
pub use super::types::{HealthStatus, PoolConfig, PoolStatistics};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

/// 接続プール
#[derive(Debug)]
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
    /// 自動スケーリング有効化
    auto_scaling_enabled: Arc<Mutex<bool>>,
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
            auto_scaling_enabled: Arc::new(Mutex::new(true)),
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

    /// 自動スケーリングを有効化/無効化
    pub async fn set_auto_scaling(&self, enabled: bool) {
        let mut auto_scaling = self.auto_scaling_enabled.lock().await;
        *auto_scaling = enabled;
        info!(
            "Auto-scaling {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// 自動スケーリングタスクを開始
    pub fn start_auto_scaling(&self) -> tokio::task::JoinHandle<()> {
        let idle_connections = Arc::clone(&self.idle_connections);
        let statistics = Arc::clone(&self.statistics);
        let auto_scaling_enabled = Arc::clone(&self.auto_scaling_enabled);
        let config = self.config.clone();
        let base_url = self.base_url.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10)); // 10秒ごとにチェック

            loop {
                interval.tick().await;

                let enabled = *auto_scaling_enabled.lock().await;
                if !enabled {
                    continue;
                }

                let stats = statistics.lock().await.clone();
                let idle_count = idle_connections.lock().await.len();

                // スケールアップ: アクティブ接続が多く、アイドルが少ない場合
                let total = stats.active_connections + idle_count;
                let utilization = if total > 0 {
                    stats.active_connections as f64 / total as f64
                } else {
                    0.0
                };

                if utilization > 0.8 && total < config.max_connections {
                    // 80%以上の使用率でスケールアップ
                    let scale_up_count = (config.max_connections.min(total + 5) - total).min(5);
                    debug!(
                        "Scaling up pool by {} connections (utilization: {:.1}%)",
                        scale_up_count,
                        utilization * 100.0
                    );

                    for _ in 0..scale_up_count {
                        if let Ok(conn) = WebSocketConnection::connect(&base_url).await {
                            idle_connections.lock().await.push_back(conn);
                        }
                    }
                }

                // スケールダウン: アイドル接続が多すぎる場合
                if idle_count > config.min_connections && utilization < 0.3 {
                    let scale_down_count = (idle_count - config.min_connections).min(5);
                    debug!(
                        "Scaling down pool by {} connections (utilization: {:.1}%)",
                        scale_down_count,
                        utilization * 100.0
                    );

                    let mut idle = idle_connections.lock().await;
                    for _ in 0..scale_down_count {
                        if let Some(conn) = idle.pop_back() {
                            let _ = conn.close().await;
                        }
                    }
                }
            }
        })
    }

    /// プールメトリクスを取得（詳細版）
    pub async fn get_metrics(&self) -> PoolMetrics {
        let stats = self.statistics.lock().await.clone();
        let idle_count = self.idle_connections.lock().await.len();

        PoolMetrics {
            total_connections: stats.active_connections + idle_count,
            active_connections: stats.active_connections,
            idle_connections: idle_count,
            pending_requests: stats.pending_requests,
            total_requests: stats.total_requests,
            failed_requests: stats.failed_requests,
            avg_wait_time_ms: stats.avg_wait_time_ms,
            utilization_rate: if stats.active_connections + idle_count > 0 {
                stats.active_connections as f64 / (stats.active_connections + idle_count) as f64
            } else {
                0.0
            },
        }
    }

    /// アクティブな接続数を取得
    pub async fn active_count(&self) -> usize {
        self.statistics.lock().await.active_connections
    }

    /// アイドルな接続数を取得
    pub async fn idle_count(&self) -> usize {
        self.idle_connections.lock().await.len()
    }
}

/// プールメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolMetrics {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub pending_requests: usize,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub avg_wait_time_ms: f64,
    pub utilization_rate: f64,
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
