//! コンテナ監視とログ管理
//!
//! コンテナのヘルスチェック、リソース使用率監視、ログ収集を提供します。

use bollard::container::StatsOptions;
use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use futures_util::stream::StreamExt;
use chrono::{DateTime, Utc};
use crate::docker_runtime::{DockerError, Result};

/// コンテナのヘルス状態
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    /// 健全
    Healthy,
    /// 異常
    Unhealthy,
    /// 起動中
    Starting,
    /// 不明
    Unknown,
}

/// コンテナのメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetrics {
    /// コンテナID
    pub container_id: String,
    
    /// コンテナ名
    pub container_name: String,
    
    /// CPU使用率（0.0-100.0）
    pub cpu_usage_percent: f64,
    
    /// メモリ使用量（バイト）
    pub memory_usage_bytes: u64,
    
    /// メモリ制限（バイト）
    pub memory_limit_bytes: u64,
    
    /// メモリ使用率（0.0-100.0）
    pub memory_usage_percent: f64,
    
    /// ネットワーク受信バイト
    pub network_rx_bytes: u64,
    
    /// ネットワーク送信バイト
    pub network_tx_bytes: u64,
    
    /// ブロックI/O読み取りバイト
    pub block_io_read_bytes: u64,
    
    /// ブロックI/O書き込みバイト
    pub block_io_write_bytes: u64,
    
    /// 収集時刻
    pub timestamp: DateTime<Utc>,
}

/// ヘルスチェック設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// チェック間隔（秒）
    pub interval_seconds: u64,
    
    /// タイムアウト（秒）
    pub timeout_seconds: u64,
    
    /// 再試行回数
    pub retries: u32,
    
    /// ヘルスチェックコマンド
    pub command: Vec<String>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            timeout_seconds: 10,
            retries: 3,
            command: vec!["CMD-SHELL".to_string(), "exit 0".to_string()],
        }
    }
}

/// 監視マネージャー
pub struct MonitoringManager {
    docker: Arc<RwLock<Docker>>,
    /// メトリクス履歴（コンテナID -> メトリクスのリスト）
    metrics_history: Arc<RwLock<HashMap<String, Vec<ContainerMetrics>>>>,
    /// ヘルス状態（コンテナID -> ステータス）
    health_status: Arc<RwLock<HashMap<String, HealthStatus>>>,
}

impl MonitoringManager {
    /// 新しいMonitoringManagerを作成
    pub fn new(docker: Arc<RwLock<Docker>>) -> Self {
        Self {
            docker,
            metrics_history: Arc::new(RwLock::new(HashMap::new())),
            health_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// コンテナのメトリクスを収集
    pub async fn collect_metrics(&self, container_id: &str) -> Result<ContainerMetrics> {
        let docker = self.docker.read().await;

        let options = Some(StatsOptions {
            stream: false,
            one_shot: true,
        });

        let mut stream = docker.stats(container_id, options);
        
        if let Some(result) = stream.next().await {
            let stats = result.map_err(|e| DockerError::ApiError(format!(
                "Failed to get container stats: {}",
                e
            )))?;

            // CPU使用率計算
            let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64 
                - stats.precpu_stats.cpu_usage.total_usage as f64;
            let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
                - stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
            let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;
            
            let cpu_usage_percent = if system_delta > 0.0 {
                (cpu_delta / system_delta) * num_cpus * 100.0
            } else {
                0.0
            };

            // メモリ使用率計算
            let memory_usage = stats.memory_stats.usage.unwrap_or(0);
            let memory_limit = stats.memory_stats.limit.unwrap_or(1);
            let memory_usage_percent = (memory_usage as f64 / memory_limit as f64) * 100.0;

            // ネットワークI/O
            let mut network_rx_bytes = 0;
            let mut network_tx_bytes = 0;
            if let Some(networks) = stats.networks {
                for (_, net_stats) in networks {
                    network_rx_bytes += net_stats.rx_bytes;
                    network_tx_bytes += net_stats.tx_bytes;
                }
            }

            // ブロックI/O
            let mut block_io_read = 0;
            let mut block_io_write = 0;
            if let Some(blkio_stats) = stats.blkio_stats.io_service_bytes_recursive {
                for entry in blkio_stats {
                    match entry.op.as_str() {
                        "read" | "Read" => block_io_read += entry.value,
                        "write" | "Write" => block_io_write += entry.value,
                        _ => {}
                    }
                }
            }

            let metrics = ContainerMetrics {
                container_id: container_id.to_string(),
                container_name: stats.name,
                cpu_usage_percent,
                memory_usage_bytes: memory_usage,
                memory_limit_bytes: memory_limit,
                memory_usage_percent,
                network_rx_bytes,
                network_tx_bytes,
                block_io_read_bytes: block_io_read,
                block_io_write_bytes: block_io_write,
                timestamp: Utc::now(),
            };

            // メトリクス履歴に追加
            let mut history = self.metrics_history.write().await;
            history
                .entry(container_id.to_string())
                .or_insert_with(Vec::new)
                .push(metrics.clone());

            // 履歴を最新100件に制限
            if let Some(list) = history.get_mut(container_id) {
                if list.len() > 100 {
                    list.drain(0..list.len() - 100);
                }
            }

            Ok(metrics)
        } else {
            Err(DockerError::ApiError("No stats available".to_string()))
        }
    }

    /// コンテナのヘルスチェック
    pub async fn check_health(&self, container_id: &str) -> Result<HealthStatus> {
        let docker = self.docker.read().await;

        let inspect = docker.inspect_container(container_id, None)
            .await
            .map_err(|e| DockerError::ContainerNotFound(format!(
                "Container {} not found: {}",
                container_id, e
            )))?;

        let status = if let Some(state) = inspect.state {
            if let Some(health) = state.health {
                match health.status.as_deref() {
                    Some("healthy") => HealthStatus::Healthy,
                    Some("unhealthy") => HealthStatus::Unhealthy,
                    Some("starting") => HealthStatus::Starting,
                    _ => HealthStatus::Unknown,
                }
            } else if state.running.unwrap_or(false) {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy
            }
        } else {
            HealthStatus::Unknown
        };

        // ステータスを保存
        let mut health_map = self.health_status.write().await;
        health_map.insert(container_id.to_string(), status.clone());

        Ok(status)
    }

    /// メトリクス履歴を取得
    pub async fn get_metrics_history(&self, container_id: &str) -> Vec<ContainerMetrics> {
        let history = self.metrics_history.read().await;
        history.get(container_id)
            .cloned()
            .unwrap_or_default()
    }

    /// すべてのコンテナのヘルス状態を取得
    pub async fn get_all_health_status(&self) -> HashMap<String, HealthStatus> {
        self.health_status.read().await.clone()
    }

    /// リソース使用率が閾値を超えているかチェック
    pub async fn check_resource_limits(
        &self,
        container_id: &str,
        cpu_threshold: f64,
        memory_threshold: f64,
    ) -> Result<bool> {
        let metrics = self.collect_metrics(container_id).await?;

        if metrics.cpu_usage_percent > cpu_threshold {
            tracing::warn!(
                "Container {} CPU usage ({:.2}%) exceeds threshold ({:.2}%)",
                container_id,
                metrics.cpu_usage_percent,
                cpu_threshold
            );
            return Ok(true);
        }

        if metrics.memory_usage_percent > memory_threshold {
            tracing::warn!(
                "Container {} memory usage ({:.2}%) exceeds threshold ({:.2}%)",
                container_id,
                metrics.memory_usage_percent,
                memory_threshold
            );
            return Ok(true);
        }

        Ok(false)
    }

    /// 監視ループを開始
    pub async fn start_monitoring(
        &self,
        container_ids: Vec<String>,
        interval_seconds: u64,
    ) {
        let docker = Arc::clone(&self.docker);
        let metrics_history = Arc::clone(&self.metrics_history);
        let health_status = Arc::clone(&self.health_status);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(interval_seconds)
            );

            loop {
                interval.tick().await;

                for container_id in &container_ids {
                    // メトリクス収集
                    let manager = Self {
                        docker: Arc::clone(&docker),
                        metrics_history: Arc::clone(&metrics_history),
                        health_status: Arc::clone(&health_status),
                    };

                    if let Err(e) = manager.collect_metrics(container_id).await {
                        tracing::error!("Failed to collect metrics for {}: {}", container_id, e);
                    }

                    // ヘルスチェック
                    if let Err(e) = manager.check_health(container_id).await {
                        tracing::error!("Failed to check health for {}: {}", container_id, e);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::Docker;

    async fn setup() -> MonitoringManager {
        let docker = Docker::connect_with_socket_defaults().unwrap();
        MonitoringManager::new(Arc::new(RwLock::new(docker)))
    }

    #[tokio::test]
    #[ignore] // Docker環境と実行中のコンテナが必要
    async fn test_collect_metrics() {
        let manager = setup().await;
        // 実行中のコンテナIDを取得する必要があります
        // let metrics = manager.collect_metrics("container_id").await;
        // assert!(metrics.is_ok());
    }

    #[test]
    fn test_health_status_enum() {
        let status = HealthStatus::Healthy;
        assert_eq!(status, HealthStatus::Healthy);
    }
}
