//! リアルタイムダッシュボードAPI

use crate::monitoring::collector::MetricsCollector;
use crate::monitoring::metrics::{MetricType, SystemMetrics};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// ダッシュボードレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardResponse {
    /// 現在のシステムメトリクス
    pub current: SystemMetrics,
    /// 統計情報
    pub stats: MetricsSummary,
    /// アクティブなアラート数
    pub active_alerts: usize,
}

/// メトリクス要約
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSummary {
    /// CPU使用率の統計
    pub cpu_stats: MetricSummaryItem,
    /// メモリ使用率の統計
    pub memory_stats: MetricSummaryItem,
    /// リクエスト数の統計
    pub request_count_stats: MetricSummaryItem,
}

/// メトリクス要約項目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSummaryItem {
    /// 平均値
    pub avg: f64,
    /// 最小値
    pub min: f64,
    /// 最大値
    pub max: f64,
    /// 95パーセンタイル
    pub p95: f64,
}

impl From<Vec<f64>> for MetricSummaryItem {
    fn from(values: Vec<f64>) -> Self {
        if values.is_empty() {
            return Self {
                avg: 0.0,
                min: 0.0,
                max: 0.0,
                p95: 0.0,
            };
        }

        let mut sorted = values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let sum: f64 = values.iter().sum();
        let avg = sum / values.len() as f64;
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        let p95_idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1);
        let p95 = sorted[p95_idx];

        Self { avg, min, max, p95 }
    }
}

/// ダッシュボードマネージャー
pub struct DashboardManager {
    /// メトリクス収集器
    collector: Arc<RwLock<MetricsCollector>>,
}

impl DashboardManager {
    /// 新しいマネージャーを作成
    pub fn new(collector: Arc<RwLock<MetricsCollector>>) -> Self {
        Self { collector }
    }

    /// 現在のダッシュボードデータを取得
    pub async fn get_dashboard(&self) -> DashboardResponse {
        let current = self
            .collector
            .read()
            .await
            .get_latest()
            .await
            .unwrap_or_else(SystemMetrics::new);

        let history = self.collector.read().await.get_history(3600).await;

        // CPU統計
        let cpu_values: Vec<f64> = history.iter().map(|m| m.cpu_usage).collect();

        // メモリ統計
        let memory_values: Vec<f64> = history.iter().map(|m| m.memory_usage).collect();

        // リクエスト数統計
        let request_values: Vec<f64> = history.iter().map(|m| m.request_count as f64).collect();

        let stats = MetricsSummary {
            cpu_stats: MetricSummaryItem::from(cpu_values),
            memory_stats: MetricSummaryItem::from(memory_values),
            request_count_stats: MetricSummaryItem::from(request_values),
        };

        DashboardResponse {
            current,
            stats,
            active_alerts: 0, // AlertManagerと統合時に実装
        }
    }

    /// 指定期間のメトリクス履歴を取得
    pub async fn get_metrics_history(&self, limit: usize) -> Vec<SystemMetrics> {
        self.collector.read().await.get_history(limit).await
    }

    /// 特定メトリクスの時系列データを取得
    pub async fn get_metric_timeseries(&self, metric_type: MetricType, limit: usize) -> Vec<f64> {
        let history = self.collector.read().await.get_history(limit).await;

        history
            .iter()
            .map(|m| match metric_type {
                MetricType::CpuUsage => m.cpu_usage,
                MetricType::MemoryUsage => m.memory_usage,
                MetricType::RequestCount => m.request_count as f64,
                MetricType::RequestRate => 0.0, // 未実装
                MetricType::ResponseTime => m.avg_response_time,
                MetricType::ErrorRate => m.error_rate(),
                MetricType::ActiveConnections => m.active_connections as f64,
                MetricType::Throughput => 0.0, // 未実装
                MetricType::Custom(_) => 0.0,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::collector::CollectorConfig;

    #[tokio::test]
    async fn test_dashboard_manager() {
        let config = CollectorConfig {
            interval: std::time::Duration::from_secs(1),
            history_size: 100,
            enable_system_metrics: true,
        };

        let collector = Arc::new(RwLock::new(MetricsCollector::new(config)));
        let dashboard = DashboardManager::new(collector.clone());

        // メトリクスを収集
        collector.read().await.start().await;

        // ダッシュボードデータ取得
        let response = dashboard.get_dashboard().await;
        assert!(response.current.cpu_usage >= 0.0);
    }

    #[tokio::test]
    async fn test_metrics_history() {
        let config = CollectorConfig {
            interval: std::time::Duration::from_millis(50),
            history_size: 100,
            enable_system_metrics: true,
        };

        let collector = Arc::new(RwLock::new(MetricsCollector::new(config)));
        let dashboard = DashboardManager::new(collector.clone());

        // メトリクスを収集開始
        collector.read().await.start().await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let history = dashboard.get_metrics_history(10).await;
        assert!(!history.is_empty());
    }

    #[tokio::test]
    async fn test_metric_timeseries() {
        let config = CollectorConfig {
            interval: std::time::Duration::from_secs(1),
            history_size: 100,
            enable_system_metrics: true,
        };

        let collector = Arc::new(RwLock::new(MetricsCollector::new(config)));
        let dashboard = DashboardManager::new(collector.clone());

        // メトリクスを収集開始
        collector.read().await.start().await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let timeseries = dashboard
            .get_metric_timeseries(MetricType::CpuUsage, 10)
            .await;
        assert!(!timeseries.is_empty());
    }
}
