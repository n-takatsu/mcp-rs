//! System metrics collection and management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// System performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// CPU usage percentage (0.0 to 1.0)
    pub cpu_usage: f64,
    /// Memory usage percentage (0.0 to 1.0)
    pub memory_usage: f64,
    /// Disk I/O operations per second
    pub disk_iops: f64,
    /// Network throughput in bytes/sec
    pub network_throughput: f64,
    /// Active database connections
    pub db_connections: usize,
    /// Average query response time in milliseconds
    pub avg_query_time: f64,
    /// Cache hit rate (0.0 to 1.0)
    pub cache_hit_rate: f64,
    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Request rate (requests per second)
    pub request_rate: f64,
    /// Average response time in milliseconds
    pub avg_response_time: f64,
    /// Custom metrics
    pub custom: HashMap<String, MetricValue>,
    /// Timestamp of metrics collection
    pub timestamp: i64,
}

impl SystemMetrics {
    /// Creates a new system metrics instance
    pub fn new() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0.0,
            disk_iops: 0.0,
            network_throughput: 0.0,
            db_connections: 0,
            avg_query_time: 0.0,
            cache_hit_rate: 0.0,
            error_rate: 0.0,
            request_rate: 0.0,
            avg_response_time: 0.0,
            custom: HashMap::new(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Sets CPU usage
    pub fn with_cpu_usage(mut self, usage: f64) -> Self {
        self.cpu_usage = usage.clamp(0.0, 1.0);
        self
    }

    /// Sets memory usage
    pub fn with_memory_usage(mut self, usage: f64) -> Self {
        self.memory_usage = usage.clamp(0.0, 1.0);
        self
    }

    /// Sets cache hit rate
    pub fn with_cache_hit_rate(mut self, rate: f64) -> Self {
        self.cache_hit_rate = rate.clamp(0.0, 1.0);
        self
    }

    /// Sets average query time
    pub fn with_avg_query_time(mut self, time_ms: f64) -> Self {
        self.avg_query_time = time_ms;
        self
    }

    /// Adds a custom metric
    pub fn with_custom_metric(mut self, key: String, value: MetricValue) -> Self {
        self.custom.insert(key, value);
        self
    }

    /// Calculates overall system health score (0.0 to 1.0)
    pub fn health_score(&self) -> f64 {
        let mut score = 1.0;

        // CPU usage penalty - more aggressive
        if self.cpu_usage > 0.8 {
            score -= (self.cpu_usage - 0.8) * 2.5;
        }

        // Memory usage penalty - more aggressive
        if self.memory_usage > 0.85 {
            score -= (self.memory_usage - 0.85) * 2.5;
        }

        // Error rate penalty
        score -= self.error_rate * 0.5;

        // Cache hit rate bonus
        score += (self.cache_hit_rate - 0.5).max(0.0) * 0.2;

        // Response time penalty
        if self.avg_response_time > 200.0 {
            score -= ((self.avg_response_time - 200.0) / 1000.0).min(0.5);
        }

        score.clamp(0.0, 1.0)
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Metric value types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MetricValue {
    /// Integer value
    Int(i64),
    /// Floating point value
    Float(f64),
    /// String value
    String(String),
    /// Boolean value
    Bool(bool),
}

impl MetricValue {
    /// Converts to f64 if possible
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            MetricValue::Int(v) => Some(*v as f64),
            MetricValue::Float(v) => Some(*v),
            MetricValue::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            MetricValue::String(_) => None,
        }
    }
}

/// Historical metrics data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsHistory {
    /// Collection of metrics over time
    pub metrics: Vec<SystemMetrics>,
    /// Time window in seconds
    pub window_seconds: i64,
}

impl MetricsHistory {
    /// Creates a new metrics history
    pub fn new(window_seconds: i64) -> Self {
        Self {
            metrics: Vec::new(),
            window_seconds,
        }
    }

    /// Adds a metrics snapshot
    pub fn add(&mut self, metrics: SystemMetrics) {
        self.metrics.push(metrics);
        self.cleanup_old_metrics();
    }

    /// Removes metrics outside the time window
    fn cleanup_old_metrics(&mut self) {
        let cutoff = chrono::Utc::now().timestamp() - self.window_seconds;
        self.metrics.retain(|m| m.timestamp >= cutoff);
    }

    /// Calculates average CPU usage over the window
    pub fn avg_cpu_usage(&self) -> f64 {
        if self.metrics.is_empty() {
            return 0.0;
        }
        self.metrics.iter().map(|m| m.cpu_usage).sum::<f64>() / self.metrics.len() as f64
    }

    /// Calculates average memory usage over the window
    pub fn avg_memory_usage(&self) -> f64 {
        if self.metrics.is_empty() {
            return 0.0;
        }
        self.metrics.iter().map(|m| m.memory_usage).sum::<f64>() / self.metrics.len() as f64
    }

    /// Calculates average response time over the window
    pub fn avg_response_time(&self) -> f64 {
        if self.metrics.is_empty() {
            return 0.0;
        }
        self.metrics
            .iter()
            .map(|m| m.avg_response_time)
            .sum::<f64>()
            / self.metrics.len() as f64
    }

    /// Finds peak CPU usage
    pub fn peak_cpu_usage(&self) -> f64 {
        self.metrics.iter().map(|m| m.cpu_usage).fold(0.0, f64::max)
    }

    /// Finds peak memory usage
    pub fn peak_memory_usage(&self) -> f64 {
        self.metrics
            .iter()
            .map(|m| m.memory_usage)
            .fold(0.0, f64::max)
    }

    /// Calculates trend (positive = increasing, negative = decreasing)
    pub fn cpu_trend(&self) -> f64 {
        if self.metrics.len() < 2 {
            return 0.0;
        }

        let mid = self.metrics.len() / 2;
        let first_half: f64 = self.metrics[..mid].iter().map(|m| m.cpu_usage).sum();
        let second_half: f64 = self.metrics[mid..].iter().map(|m| m.cpu_usage).sum();

        (second_half / (self.metrics.len() - mid) as f64) - (first_half / mid as f64)
    }

    /// Returns the most recent metrics
    pub fn latest(&self) -> Option<&SystemMetrics> {
        self.metrics.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_metrics_creation() {
        let metrics = SystemMetrics::new();
        assert_eq!(metrics.cpu_usage, 0.0);
        assert_eq!(metrics.memory_usage, 0.0);
    }

    #[test]
    fn test_system_metrics_builder() {
        let metrics = SystemMetrics::new()
            .with_cpu_usage(0.75)
            .with_memory_usage(0.60)
            .with_cache_hit_rate(0.90);

        assert_eq!(metrics.cpu_usage, 0.75);
        assert_eq!(metrics.memory_usage, 0.60);
        assert_eq!(metrics.cache_hit_rate, 0.90);
    }

    #[test]
    fn test_health_score() {
        let good_metrics = SystemMetrics::new()
            .with_cpu_usage(0.50)
            .with_memory_usage(0.60)
            .with_cache_hit_rate(0.85);

        let score = good_metrics.health_score();
        assert!(score > 0.8);

        let bad_metrics = SystemMetrics::new()
            .with_cpu_usage(0.95)
            .with_memory_usage(0.95)
            .with_cache_hit_rate(0.20);

        let bad_score = bad_metrics.health_score();
        assert!(bad_score < 0.5);
    }

    #[test]
    fn test_metric_value_conversion() {
        assert_eq!(MetricValue::Int(42).as_f64(), Some(42.0));
        assert_eq!(MetricValue::Float(std::f64::consts::PI).as_f64(), Some(std::f64::consts::PI));
        assert_eq!(MetricValue::Bool(true).as_f64(), Some(1.0));
        assert_eq!(MetricValue::Bool(false).as_f64(), Some(0.0));
        assert_eq!(MetricValue::String("test".to_string()).as_f64(), None);
    }

    #[test]
    fn test_metrics_history() {
        let mut history = MetricsHistory::new(300); // 5 minutes

        history.add(SystemMetrics::new().with_cpu_usage(0.5));
        history.add(SystemMetrics::new().with_cpu_usage(0.6));
        history.add(SystemMetrics::new().with_cpu_usage(0.7));

        assert_eq!(history.metrics.len(), 3);
        assert!((history.avg_cpu_usage() - 0.6).abs() < 0.01);
        assert_eq!(history.peak_cpu_usage(), 0.7);
    }
}
