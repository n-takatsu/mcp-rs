//! メトリクス型定義

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// メトリクスの種類
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// CPU使用率（%）
    CpuUsage,
    /// メモリ使用率（%）
    MemoryUsage,
    /// リクエスト数（総数）
    RequestCount,
    /// リクエストレート（req/s）
    RequestRate,
    /// レスポンスタイム（ms）
    ResponseTime,
    /// エラー率（%）
    ErrorRate,
    /// アクティブ接続数
    ActiveConnections,
    /// スループット（bytes/s）
    Throughput,
    /// カスタムメトリクス
    Custom(String),
}

impl MetricType {
    /// メトリクス名を取得
    pub fn name(&self) -> &str {
        match self {
            Self::CpuUsage => "cpu_usage",
            Self::MemoryUsage => "memory_usage",
            Self::RequestCount => "request_count",
            Self::RequestRate => "request_rate",
            Self::ResponseTime => "response_time",
            Self::ErrorRate => "error_rate",
            Self::ActiveConnections => "active_connections",
            Self::Throughput => "throughput",
            Self::Custom(name) => name,
        }
    }

    /// 単位を取得
    pub fn unit(&self) -> &str {
        match self {
            Self::CpuUsage | Self::MemoryUsage | Self::ErrorRate => "%",
            Self::ResponseTime => "ms",
            Self::RequestCount | Self::ActiveConnections => "count",
            Self::RequestRate => "req/s",
            Self::Throughput => "bytes/s",
            Self::Custom(_) => "",
        }
    }
}

/// メトリクス値
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    /// メトリクスの種類
    pub metric_type: MetricType,
    /// 値
    pub value: f64,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// タグ（ラベル）
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

impl MetricValue {
    /// 新しいメトリクス値を作成
    pub fn new(metric_type: MetricType, value: f64) -> Self {
        Self {
            metric_type,
            value,
            timestamp: Utc::now(),
            tags: HashMap::new(),
        }
    }

    /// タグを追加
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// タグを複数追加
    pub fn with_tags(mut self, tags: HashMap<String, String>) -> Self {
        self.tags.extend(tags);
        self
    }
}

/// システムメトリクスのスナップショット
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// CPU使用率（%）
    pub cpu_usage: f64,
    /// メモリ使用率（%）
    pub memory_usage: f64,
    /// 使用メモリ量（MB）
    pub memory_used_mb: f64,
    /// 総メモリ量（MB）
    pub memory_total_mb: f64,
    /// リクエスト数
    pub request_count: u64,
    /// エラー数
    pub error_count: u64,
    /// アクティブ接続数
    pub active_connections: u32,
    /// 平均レスポンスタイム（ms）
    pub avg_response_time: f64,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
}

impl SystemMetrics {
    /// 新しいシステムメトリクスを作成
    pub fn new() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            memory_used_mb: 0.0,
            memory_total_mb: 0.0,
            request_count: 0,
            error_count: 0,
            active_connections: 0,
            avg_response_time: 0.0,
            timestamp: Utc::now(),
        }
    }

    /// エラー率を計算（%）
    pub fn error_rate(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            (self.error_count as f64 / self.request_count as f64) * 100.0
        }
    }

    /// 健全性スコアを計算（0-100）
    pub fn health_score(&self) -> f64 {
        let cpu_score = (100.0 - self.cpu_usage).max(0.0);
        let memory_score = (100.0 - self.memory_usage).max(0.0);
        let error_score = (100.0 - self.error_rate()).max(0.0);

        (cpu_score + memory_score + error_score) / 3.0
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// メトリクス統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricStats {
    /// 最小値
    pub min: f64,
    /// 最大値
    pub max: f64,
    /// 平均値
    pub mean: f64,
    /// 標準偏差
    pub std_dev: f64,
    /// パーセンタイル（P50, P95, P99）
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    /// サンプル数
    pub count: usize,
}

impl MetricStats {
    /// 値のリストから統計を計算
    pub fn from_values(mut values: Vec<f64>) -> Self {
        if values.is_empty() {
            return Self::default();
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let count = values.len();
        let min = values[0];
        let max = values[count - 1];
        let mean = values.iter().sum::<f64>() / count as f64;

        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();

        let p50 = percentile(&values, 0.5);
        let p95 = percentile(&values, 0.95);
        let p99 = percentile(&values, 0.99);

        Self {
            min,
            max,
            mean,
            std_dev,
            p50,
            p95,
            p99,
            count,
        }
    }
}

impl Default for MetricStats {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            std_dev: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            count: 0,
        }
    }
}

/// パーセンタイルを計算
fn percentile(sorted_values: &[f64], p: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }

    let index = (p * (sorted_values.len() - 1) as f64).round() as usize;
    sorted_values[index.min(sorted_values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_type_name() {
        assert_eq!(MetricType::CpuUsage.name(), "cpu_usage");
        assert_eq!(MetricType::Custom("test".to_string()).name(), "test");
    }

    #[test]
    fn test_metric_value_creation() {
        let metric = MetricValue::new(MetricType::CpuUsage, 50.0).with_tag("host", "server1");

        assert_eq!(metric.value, 50.0);
        assert_eq!(metric.tags.get("host"), Some(&"server1".to_string()));
    }

    #[test]
    fn test_system_metrics_error_rate() {
        let mut metrics = SystemMetrics::new();
        metrics.request_count = 100;
        metrics.error_count = 5;

        assert_eq!(metrics.error_rate(), 5.0);
    }

    #[test]
    fn test_system_metrics_health_score() {
        let mut metrics = SystemMetrics::new();
        metrics.cpu_usage = 50.0;
        metrics.memory_usage = 60.0;
        metrics.request_count = 100;
        metrics.error_count = 0;

        let score = metrics.health_score();
        assert!((60.0..=70.0).contains(&score));
    }

    #[test]
    fn test_metric_stats() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = MetricStats::from_values(values);

        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 5.0);
        assert_eq!(stats.mean, 3.0);
        assert_eq!(stats.count, 5);
    }
}
