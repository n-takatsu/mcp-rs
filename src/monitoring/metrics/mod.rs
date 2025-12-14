//! Metrics Collection Module
//!
//! リアルタイムメトリクス収集システム

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

pub mod realtime;

/// メトリクスの種類
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetricType {
    /// CPU使用率
    Cpu,
    /// メモリ使用率
    Memory,
    /// ネットワークトラフィック
    Network,
    /// データベースクエリ
    DatabaseQuery,
    /// カスタムメトリクス
    Custom(String),
}

/// メトリクスデータポイント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    /// メトリクスタイプ
    pub metric_type: MetricType,
    /// 値
    pub value: f64,
    /// タイムスタンプ
    pub timestamp: SystemTime,
    /// タグ（メタデータ）
    pub tags: std::collections::HashMap<String, String>,
}

impl MetricPoint {
    /// 新しいメトリクスポイントを作成
    pub fn new(metric_type: MetricType, value: f64) -> Self {
        Self {
            metric_type,
            value,
            timestamp: SystemTime::now(),
            tags: std::collections::HashMap::new(),
        }
    }

    /// タグを追加
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// カスタムタイムスタンプを設定
    pub fn with_timestamp(mut self, timestamp: SystemTime) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// メトリクス収集器トレイト
#[async_trait::async_trait]
pub trait MetricsCollector: Send + Sync {
    /// メトリクスを収集
    async fn collect(&self) -> crate::error::Result<Vec<MetricPoint>>;

    /// メトリクス収集間隔
    fn collection_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    /// コレクター名
    fn name(&self) -> &str;
}

/// システムメトリクス収集器
pub struct SystemMetricsCollector {
    name: String,
}

impl SystemMetricsCollector {
    /// 新しいシステムメトリクス収集器を作成
    pub fn new() -> Self {
        Self {
            name: "system".to_string(),
        }
    }
}

impl Default for SystemMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl MetricsCollector for SystemMetricsCollector {
    async fn collect(&self) -> crate::error::Result<Vec<MetricPoint>> {
        let mut metrics = Vec::new();

        // CPU使用率（モック実装）
        let cpu_usage = Self::get_cpu_usage();
        metrics.push(
            MetricPoint::new(MetricType::Cpu, cpu_usage)
                .with_tag("host", "localhost")
                .with_tag("core", "all"),
        );

        // メモリ使用率（モック実装）
        let memory_usage = Self::get_memory_usage();
        metrics.push(
            MetricPoint::new(MetricType::Memory, memory_usage)
                .with_tag("host", "localhost")
                .with_tag("type", "used"),
        );

        Ok(metrics)
    }

    fn collection_interval(&self) -> Duration {
        Duration::from_secs(10) // 10秒ごとに収集
    }

    fn name(&self) -> &str {
        &self.name
    }
}

impl SystemMetricsCollector {
    /// CPU使用率を取得（モック実装）
    fn get_cpu_usage() -> f64 {
        // 実際の実装では sysinfo crate などを使用
        rand::random::<f64>() * 100.0
    }

    /// メモリ使用率を取得（モック実装）
    fn get_memory_usage() -> f64 {
        // 実際の実装では sysinfo crate などを使用
        rand::random::<f64>() * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_point_creation() {
        let metric = MetricPoint::new(MetricType::Cpu, 50.0);
        assert_eq!(metric.value, 50.0);
        assert!(matches!(metric.metric_type, MetricType::Cpu));
    }

    #[test]
    fn test_metric_point_with_tags() {
        let metric = MetricPoint::new(MetricType::Memory, 75.0)
            .with_tag("host", "server1")
            .with_tag("region", "us-west");

        assert_eq!(metric.tags.get("host"), Some(&"server1".to_string()));
        assert_eq!(metric.tags.get("region"), Some(&"us-west".to_string()));
    }

    #[tokio::test]
    async fn test_system_metrics_collector() {
        let collector = SystemMetricsCollector::new();
        let metrics = collector.collect().await.unwrap();

        assert!(!metrics.is_empty());
        assert_eq!(collector.name(), "system");
    }

    #[test]
    fn test_collection_interval() {
        let collector = SystemMetricsCollector::new();
        assert_eq!(collector.collection_interval(), Duration::from_secs(10));
    }

    #[test]
    fn test_metric_type_custom() {
        let metric = MetricPoint::new(MetricType::Custom("latency".to_string()), 123.45);
        assert!(matches!(
            metric.metric_type,
            MetricType::Custom(ref s) if s == "latency"
        ));
    }
}
