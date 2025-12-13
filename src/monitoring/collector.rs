//! メトリクス収集システム

use crate::monitoring::metrics::{MetricType, MetricValue, SystemMetrics};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;

/// メトリクス収集設定
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// 収集間隔
    pub interval: Duration,
    /// 保持する履歴データ数
    pub history_size: usize,
    /// システムメトリクス収集を有効化
    pub enable_system_metrics: bool,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            history_size: 3600, // 1時間分（1秒間隔）
            enable_system_metrics: true,
        }
    }
}

/// メトリクス収集器
pub struct MetricsCollector {
    config: CollectorConfig,
    metrics_history: Arc<RwLock<VecDeque<SystemMetrics>>>,
    custom_metrics: Arc<RwLock<Vec<MetricValue>>>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new(CollectorConfig::default())
    }
}

impl MetricsCollector {
    /// 新しいコレクターを作成
    pub fn new(config: CollectorConfig) -> Self {
        let history_size = config.history_size;
        Self {
            config,
            metrics_history: Arc::new(RwLock::new(VecDeque::with_capacity(history_size))),
            custom_metrics: Arc::new(RwLock::new(Vec::new())),
        }
    }



    /// メトリクス収集を開始
    pub async fn start(&self) {
        if !self.config.enable_system_metrics {
            return;
        }

        let history = self.metrics_history.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut ticker = interval(config.interval);

            loop {
                ticker.tick().await;

                let metrics = Self::collect_system_metrics().await;

                let mut history = history.write().await;
                if history.len() >= config.history_size {
                    history.pop_front();
                }
                history.push_back(metrics);
            }
        });
    }

    /// システムメトリクスを収集
    async fn collect_system_metrics() -> SystemMetrics {
        // 実際の実装では sysinfo クレートなどを使用
        // ここでは簡易的なデモ実装
        SystemMetrics::new()
    }

    /// 最新のメトリクスを取得
    pub async fn get_latest(&self) -> Option<SystemMetrics> {
        let history = self.metrics_history.read().await;
        history.back().cloned()
    }

    /// 履歴データを取得
    pub async fn get_history(&self, limit: usize) -> Vec<SystemMetrics> {
        let history = self.metrics_history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// 指定期間の履歴を取得
    pub async fn get_history_range(&self, duration: Duration) -> Vec<SystemMetrics> {
        let samples = (duration.as_secs() / self.config.interval.as_secs()) as usize;
        self.get_history(samples).await
    }

    /// カスタムメトリクスを記録
    pub async fn record_metric(&self, metric: MetricValue) {
        let mut metrics = self.custom_metrics.write().await;
        metrics.push(metric);

        // 古いデータを削除（メモリ制限）
        let history_size = self.config.history_size;
        if metrics.len() > history_size {
            let drain_count = metrics.len() - history_size;
            metrics.drain(0..drain_count);
        }
    }

    /// カスタムメトリクスを取得
    pub async fn get_custom_metrics(
        &self,
        metric_type: &MetricType,
        limit: usize,
    ) -> Vec<MetricValue> {
        let metrics = self.custom_metrics.read().await;
        metrics
            .iter()
            .filter(|m| &m.metric_type == metric_type)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// リクエストメトリクスを記録
    pub async fn record_request(&self, response_time_ms: f64, is_error: bool) {
        if let Some(mut latest) = self.get_latest().await {
            latest.request_count += 1;
            if is_error {
                latest.error_count += 1;
            }

            // 移動平均でレスポンスタイムを更新
            let total_time = latest.avg_response_time * (latest.request_count - 1) as f64;
            latest.avg_response_time =
                (total_time + response_time_ms) / latest.request_count as f64;

            let mut history = self.metrics_history.write().await;
            if let Some(back) = history.back_mut() {
                *back = latest;
            }
        }
    }

    /// アクティブ接続数を更新
    pub async fn update_active_connections(&self, delta: i32) {
        let mut history = self.metrics_history.write().await;
        if let Some(back) = history.back_mut() {
            back.active_connections = (back.active_connections as i32 + delta).max(0) as u32;
        }
    }

    /// 統計情報をクリア
    pub async fn clear(&self) {
        let mut history = self.metrics_history.write().await;
        history.clear();

        let mut custom = self.custom_metrics.write().await;
        custom.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_collector_creation() {
        let config = CollectorConfig::default();
        let collector = MetricsCollector::new(config);

        let latest = collector.get_latest().await;
        assert!(latest.is_none());
    }

    #[tokio::test]
    async fn test_record_metric() {
        let collector = MetricsCollector::default();
        let metric = MetricValue::new(MetricType::CpuUsage, 50.0);

        collector.record_metric(metric).await;

        let metrics = collector
            .get_custom_metrics(&MetricType::CpuUsage, 10)
            .await;
        assert_eq!(metrics.len(), 1);
    }

    #[tokio::test]
    async fn test_record_request() {
        let collector = MetricsCollector::default();

        // 初期メトリクスを追加
        let mut history = collector.metrics_history.write().await;
        history.push_back(SystemMetrics::new());
        drop(history);

        collector.record_request(100.0, false).await;

        let latest = collector.get_latest().await.unwrap();
        assert_eq!(latest.request_count, 1);
        assert_eq!(latest.error_count, 0);
    }
}
