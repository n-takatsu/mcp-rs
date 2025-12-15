//! Bottleneck detection and analysis

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::metrics::{MetricsHistory, SystemMetrics};

/// Bottleneck categories
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BottleneckCategory {
    /// CPU bottleneck
    Cpu,
    /// Memory bottleneck
    Memory,
    /// Database bottleneck
    Database,
    /// Network bottleneck
    Network,
    /// Cache inefficiency
    Cache,
    /// Disk I/O bottleneck
    DiskIo,
    /// Application logic bottleneck
    Application,
}

impl BottleneckCategory {
    /// Returns a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            BottleneckCategory::Cpu => "CPU Processing",
            BottleneckCategory::Memory => "Memory Usage",
            BottleneckCategory::Database => "Database Operations",
            BottleneckCategory::Network => "Network I/O",
            BottleneckCategory::Cache => "Caching Strategy",
            BottleneckCategory::DiskIo => "Disk I/O",
            BottleneckCategory::Application => "Application Logic",
        }
    }
}

/// Severity levels for bottlenecks
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

impl Severity {
    /// Returns severity score (0-100)
    pub fn score(&self) -> u8 {
        match self {
            Severity::Low => 25,
            Severity::Medium => 50,
            Severity::High => 75,
            Severity::Critical => 100,
        }
    }
}

/// Detected performance bottleneck
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bottleneck {
    /// Bottleneck category
    pub category: BottleneckCategory,
    /// Severity level
    pub severity: Severity,
    /// Human-readable description
    pub description: String,
    /// Related metrics
    pub metrics: HashMap<String, f64>,
    /// Detection timestamp
    pub detected_at: i64,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
}

impl Bottleneck {
    /// Creates a new bottleneck
    pub fn new(category: BottleneckCategory, severity: Severity, description: String) -> Self {
        Self {
            category,
            severity,
            description,
            metrics: HashMap::new(),
            detected_at: chrono::Utc::now().timestamp(),
            confidence: 1.0,
        }
    }

    /// Adds a metric to the bottleneck
    pub fn with_metric(mut self, key: String, value: f64) -> Self {
        self.metrics.insert(key, value);
        self
    }

    /// Sets confidence score
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Returns priority score for sorting
    pub fn priority_score(&self) -> u8 {
        (self.severity.score() as f64 * self.confidence) as u8
    }
}

/// Bottleneck detector trait
pub trait BottleneckDetector: Send + Sync {
    /// Detects bottlenecks from metrics
    fn detect(&self, metrics: &SystemMetrics) -> Vec<Bottleneck>;

    /// Detects bottlenecks from historical metrics
    fn detect_from_history(&self, history: &MetricsHistory) -> Vec<Bottleneck>;

    /// Prioritizes bottlenecks by severity and impact
    fn prioritize(&self, bottlenecks: &[Bottleneck]) -> Vec<Bottleneck>;
}

/// Default bottleneck detector implementation
#[derive(Debug, Clone)]
pub struct DefaultBottleneckDetector {
    /// CPU usage threshold for detection
    cpu_threshold: f64,
    /// Memory usage threshold
    memory_threshold: f64,
    /// Cache hit rate threshold
    cache_threshold: f64,
    /// Response time threshold (ms)
    response_time_threshold: f64,
}

impl DefaultBottleneckDetector {
    /// Creates a new default bottleneck detector
    pub fn new() -> Self {
        Self {
            cpu_threshold: 0.80,
            memory_threshold: 0.85,
            cache_threshold: 0.50,
            response_time_threshold: 200.0,
        }
    }

    /// Sets CPU threshold
    pub fn with_cpu_threshold(mut self, threshold: f64) -> Self {
        self.cpu_threshold = threshold;
        self
    }

    /// Sets memory threshold
    pub fn with_memory_threshold(mut self, threshold: f64) -> Self {
        self.memory_threshold = threshold;
        self
    }

    /// Checks for CPU bottleneck
    fn check_cpu(&self, metrics: &SystemMetrics) -> Option<Bottleneck> {
        if metrics.cpu_usage > self.cpu_threshold {
            let severity = if metrics.cpu_usage > 0.95 {
                Severity::Critical
            } else if metrics.cpu_usage > 0.90 {
                Severity::High
            } else {
                Severity::Medium
            };

            Some(
                Bottleneck::new(
                    BottleneckCategory::Cpu,
                    severity,
                    format!("High CPU usage detected: {:.1}%", metrics.cpu_usage * 100.0),
                )
                .with_metric("cpu_usage".to_string(), metrics.cpu_usage)
                .with_confidence(0.95),
            )
        } else {
            None
        }
    }

    /// Checks for memory bottleneck
    fn check_memory(&self, metrics: &SystemMetrics) -> Option<Bottleneck> {
        if metrics.memory_usage > self.memory_threshold {
            let severity = if metrics.memory_usage > 0.95 {
                Severity::Critical
            } else if metrics.memory_usage > 0.90 {
                Severity::High
            } else {
                Severity::Medium
            };

            Some(
                Bottleneck::new(
                    BottleneckCategory::Memory,
                    severity,
                    format!(
                        "High memory usage detected: {:.1}%",
                        metrics.memory_usage * 100.0
                    ),
                )
                .with_metric("memory_usage".to_string(), metrics.memory_usage)
                .with_confidence(0.95),
            )
        } else {
            None
        }
    }

    /// Checks for cache bottleneck
    fn check_cache(&self, metrics: &SystemMetrics) -> Option<Bottleneck> {
        if metrics.cache_hit_rate < self.cache_threshold {
            let severity = if metrics.cache_hit_rate < 0.30 {
                Severity::High
            } else if metrics.cache_hit_rate < 0.40 {
                Severity::Medium
            } else {
                Severity::Low
            };

            Some(
                Bottleneck::new(
                    BottleneckCategory::Cache,
                    severity,
                    format!(
                        "Low cache hit rate detected: {:.1}%",
                        metrics.cache_hit_rate * 100.0
                    ),
                )
                .with_metric("cache_hit_rate".to_string(), metrics.cache_hit_rate)
                .with_confidence(0.85),
            )
        } else {
            None
        }
    }

    /// Checks for database bottleneck
    fn check_database(&self, metrics: &SystemMetrics) -> Option<Bottleneck> {
        if metrics.avg_query_time > 100.0 {
            let severity = if metrics.avg_query_time > 500.0 {
                Severity::High
            } else if metrics.avg_query_time > 250.0 {
                Severity::Medium
            } else {
                Severity::Low
            };

            Some(
                Bottleneck::new(
                    BottleneckCategory::Database,
                    severity,
                    format!(
                        "Slow database queries detected: {:.1}ms average",
                        metrics.avg_query_time
                    ),
                )
                .with_metric("avg_query_time".to_string(), metrics.avg_query_time)
                .with_confidence(0.90),
            )
        } else {
            None
        }
    }

    /// Checks for response time bottleneck
    fn check_response_time(&self, metrics: &SystemMetrics) -> Option<Bottleneck> {
        if metrics.avg_response_time > self.response_time_threshold {
            let severity = if metrics.avg_response_time > 1000.0 {
                Severity::High
            } else if metrics.avg_response_time > 500.0 {
                Severity::Medium
            } else {
                Severity::Low
            };

            Some(
                Bottleneck::new(
                    BottleneckCategory::Application,
                    severity,
                    format!(
                        "Slow response time detected: {:.1}ms average",
                        metrics.avg_response_time
                    ),
                )
                .with_metric("avg_response_time".to_string(), metrics.avg_response_time)
                .with_confidence(0.88),
            )
        } else {
            None
        }
    }
}

impl Default for DefaultBottleneckDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl BottleneckDetector for DefaultBottleneckDetector {
    fn detect(&self, metrics: &SystemMetrics) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        if let Some(b) = self.check_cpu(metrics) {
            bottlenecks.push(b);
        }

        if let Some(b) = self.check_memory(metrics) {
            bottlenecks.push(b);
        }

        if let Some(b) = self.check_cache(metrics) {
            bottlenecks.push(b);
        }

        if let Some(b) = self.check_database(metrics) {
            bottlenecks.push(b);
        }

        if let Some(b) = self.check_response_time(metrics) {
            bottlenecks.push(b);
        }

        bottlenecks
    }

    fn detect_from_history(&self, history: &MetricsHistory) -> Vec<Bottleneck> {
        if let Some(latest) = history.latest() {
            let mut bottlenecks = self.detect(latest);

            // Check for trending issues
            let cpu_trend = history.cpu_trend();
            if cpu_trend > 0.1 {
                bottlenecks.push(
                    Bottleneck::new(
                        BottleneckCategory::Cpu,
                        Severity::Medium,
                        "CPU usage is trending upward".to_string(),
                    )
                    .with_metric("cpu_trend".to_string(), cpu_trend)
                    .with_confidence(0.75),
                );
            }

            bottlenecks
        } else {
            Vec::new()
        }
    }

    fn prioritize(&self, bottlenecks: &[Bottleneck]) -> Vec<Bottleneck> {
        let mut sorted = bottlenecks.to_vec();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.priority_score()));
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bottleneck_creation() {
        let bottleneck = Bottleneck::new(
            BottleneckCategory::Cpu,
            Severity::High,
            "Test bottleneck".to_string(),
        );

        assert_eq!(bottleneck.category, BottleneckCategory::Cpu);
        assert_eq!(bottleneck.severity, Severity::High);
    }

    #[test]
    fn test_bottleneck_priority_score() {
        let high_bottleneck = Bottleneck::new(
            BottleneckCategory::Cpu,
            Severity::High,
            "High severity".to_string(),
        )
        .with_confidence(0.95);

        let low_bottleneck = Bottleneck::new(
            BottleneckCategory::Cache,
            Severity::Low,
            "Low severity".to_string(),
        )
        .with_confidence(0.80);

        assert!(high_bottleneck.priority_score() > low_bottleneck.priority_score());
    }

    #[test]
    fn test_detect_cpu_bottleneck() {
        let detector = DefaultBottleneckDetector::new();
        let metrics = SystemMetrics::new().with_cpu_usage(0.95);

        let bottlenecks = detector.detect(&metrics);
        assert!(!bottlenecks.is_empty());
        assert!(bottlenecks
            .iter()
            .any(|b| b.category == BottleneckCategory::Cpu));
    }

    #[test]
    fn test_detect_memory_bottleneck() {
        let detector = DefaultBottleneckDetector::new();
        let metrics = SystemMetrics::new().with_memory_usage(0.90);

        let bottlenecks = detector.detect(&metrics);
        assert!(bottlenecks
            .iter()
            .any(|b| b.category == BottleneckCategory::Memory));
    }

    #[test]
    fn test_detect_cache_bottleneck() {
        let detector = DefaultBottleneckDetector::new();
        let metrics = SystemMetrics::new().with_cache_hit_rate(0.30);

        let bottlenecks = detector.detect(&metrics);
        assert!(bottlenecks
            .iter()
            .any(|b| b.category == BottleneckCategory::Cache));
    }

    #[test]
    fn test_prioritize_bottlenecks() {
        let detector = DefaultBottleneckDetector::new();
        let bottlenecks = vec![
            Bottleneck::new(BottleneckCategory::Cpu, Severity::Low, "Low".to_string()),
            Bottleneck::new(
                BottleneckCategory::Memory,
                Severity::Critical,
                "Critical".to_string(),
            ),
            Bottleneck::new(
                BottleneckCategory::Cache,
                Severity::Medium,
                "Medium".to_string(),
            ),
        ];

        let sorted = detector.prioritize(&bottlenecks);
        assert_eq!(sorted[0].severity, Severity::Critical);
    }
}
