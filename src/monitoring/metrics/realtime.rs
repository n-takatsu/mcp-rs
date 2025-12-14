//! Realtime Metrics Module
//!
//! リアルタイムメトリクス監視システム

use super::{MetricPoint, MetricType, MetricsCollector};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

/// リアルタイムメトリクスストア
#[derive(Clone)]
pub struct RealtimeMetrics {
    /// メトリクスバッファ（時系列データ）
    buffer: Arc<RwLock<VecDeque<MetricPoint>>>,
    /// バッファサイズ上限
    max_buffer_size: usize,
}

impl RealtimeMetrics {
    /// 新しいリアルタイムメトリクスストアを作成
    pub fn new(max_buffer_size: usize) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(max_buffer_size))),
            max_buffer_size,
        }
    }

    /// メトリクスポイントを追加
    pub async fn add_metric(&self, metric: MetricPoint) {
        let mut buffer = self.buffer.write().await;

        // バッファサイズ上限に達した場合、古いデータを削除
        if buffer.len() >= self.max_buffer_size {
            buffer.pop_front();
        }

        buffer.push_back(metric);
    }

    /// 最新のメトリクスを取得
    pub async fn get_latest(&self, metric_type: &MetricType) -> Option<MetricPoint> {
        let buffer = self.buffer.read().await;
        buffer
            .iter()
            .rev()
            .find(|m| &m.metric_type == metric_type)
            .cloned()
    }

    /// 時間範囲内のメトリクスを取得
    pub async fn get_range(
        &self,
        metric_type: &MetricType,
        start: SystemTime,
        end: SystemTime,
    ) -> Vec<MetricPoint> {
        let buffer = self.buffer.read().await;
        buffer
            .iter()
            .filter(|m| &m.metric_type == metric_type && m.timestamp >= start && m.timestamp <= end)
            .cloned()
            .collect()
    }

    /// 全メトリクスを取得
    pub async fn get_all(&self) -> Vec<MetricPoint> {
        let buffer = self.buffer.read().await;
        buffer.iter().cloned().collect()
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self, metric_type: &MetricType) -> Option<MetricStatistics> {
        let buffer = self.buffer.read().await;
        let values: Vec<f64> = buffer
            .iter()
            .filter(|m| &m.metric_type == metric_type)
            .map(|m| m.value)
            .collect();

        if values.is_empty() {
            return None;
        }

        let count = values.len();
        let sum: f64 = values.iter().sum();
        let mean = sum / count as f64;

        let mut sorted_values = values.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let min = *sorted_values.first()?;
        let max = *sorted_values.last()?;
        let median = if count % 2 == 0 {
            (sorted_values[count / 2 - 1] + sorted_values[count / 2]) / 2.0
        } else {
            sorted_values[count / 2]
        };

        // 標準偏差
        let variance: f64 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();

        Some(MetricStatistics {
            count,
            mean,
            median,
            min,
            max,
            std_dev,
        })
    }

    /// バッファをクリア
    pub async fn clear(&self) {
        let mut buffer = self.buffer.write().await;
        buffer.clear();
    }
}

/// メトリクス統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricStatistics {
    /// データポイント数
    pub count: usize,
    /// 平均値
    pub mean: f64,
    /// 中央値
    pub median: f64,
    /// 最小値
    pub min: f64,
    /// 最大値
    pub max: f64,
    /// 標準偏差
    pub std_dev: f64,
}

/// リアルタイムモニター
pub struct RealtimeMonitor {
    /// メトリクスストア
    metrics: RealtimeMetrics,
    /// コレクター
    collectors: Vec<Box<dyn MetricsCollector>>,
    /// モニタリング間隔
    interval: Duration,
}

impl RealtimeMonitor {
    /// 新しいリアルタイムモニターを作成
    pub fn new(interval: Duration, max_buffer_size: usize) -> Self {
        Self {
            metrics: RealtimeMetrics::new(max_buffer_size),
            collectors: Vec::new(),
            interval,
        }
    }

    /// コレクターを追加
    pub fn add_collector(&mut self, collector: Box<dyn MetricsCollector>) {
        self.collectors.push(collector);
    }

    /// メトリクスストアを取得
    pub fn metrics(&self) -> &RealtimeMetrics {
        &self.metrics
    }

    /// モニタリングを開始
    pub async fn start(self) -> Result<()> {
        let metrics = self.metrics.clone();
        let collectors = Arc::new(self.collectors);
        let interval = self.interval;

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            loop {
                interval_timer.tick().await;

                for collector in collectors.iter() {
                    match collector.collect().await {
                        Ok(points) => {
                            for point in points {
                                metrics.add_metric(point).await;
                            }
                        }
                        Err(e) => {
                            eprintln!("Collector {} error: {}", collector.name(), e);
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_realtime_metrics_add_and_get() {
        let metrics = RealtimeMetrics::new(10);

        let metric = MetricPoint::new(MetricType::Cpu, 50.0);
        metrics.add_metric(metric.clone()).await;

        let latest = metrics.get_latest(&MetricType::Cpu).await;
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().value, 50.0);
    }

    #[tokio::test]
    async fn test_buffer_size_limit() {
        let metrics = RealtimeMetrics::new(3);

        for i in 0..5 {
            metrics
                .add_metric(MetricPoint::new(MetricType::Cpu, i as f64))
                .await;
        }

        let all = metrics.get_all().await;
        assert_eq!(all.len(), 3);
        // 最古の2つが削除され、2.0, 3.0, 4.0が残る
        assert_eq!(all[0].value, 2.0);
    }

    #[tokio::test]
    async fn test_get_range() {
        let metrics = RealtimeMetrics::new(10);
        let now = SystemTime::now();

        for i in 0..5 {
            let timestamp = now + Duration::from_secs(i);
            metrics
                .add_metric(
                    MetricPoint::new(MetricType::Memory, i as f64).with_timestamp(timestamp),
                )
                .await;
        }

        let start = now + Duration::from_secs(1);
        let end = now + Duration::from_secs(3);
        let range = metrics.get_range(&MetricType::Memory, start, end).await;

        assert_eq!(range.len(), 3); // 1, 2, 3
    }

    #[tokio::test]
    async fn test_statistics() {
        let metrics = RealtimeMetrics::new(10);

        for value in [10.0, 20.0, 30.0, 40.0, 50.0] {
            metrics
                .add_metric(MetricPoint::new(MetricType::Cpu, value))
                .await;
        }

        let stats = metrics.get_statistics(&MetricType::Cpu).await.unwrap();

        assert_eq!(stats.count, 5);
        assert_eq!(stats.mean, 30.0);
        assert_eq!(stats.median, 30.0);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 50.0);
    }

    #[tokio::test]
    async fn test_clear() {
        let metrics = RealtimeMetrics::new(10);

        metrics
            .add_metric(MetricPoint::new(MetricType::Cpu, 50.0))
            .await;

        assert_eq!(metrics.get_all().await.len(), 1);

        metrics.clear().await;
        assert_eq!(metrics.get_all().await.len(), 0);
    }

    #[tokio::test]
    async fn test_realtime_monitor_creation() {
        let monitor = RealtimeMonitor::new(Duration::from_secs(5), 100);
        assert!(monitor.collectors.is_empty());
    }
}
