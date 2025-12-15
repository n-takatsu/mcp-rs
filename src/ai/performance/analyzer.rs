//! Performance analysis and anomaly detection

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::bottleneck::{Bottleneck, BottleneckDetector, DefaultBottleneckDetector};
use super::metrics::{MetricsHistory, SystemMetrics};

/// Analysis severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalysisSeverity {
    /// Normal operation
    Normal,
    /// Warning level
    Warning,
    /// Error level
    Error,
    /// Critical level
    Critical,
}

/// Performance analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    /// Overall health score (0.0 to 1.0)
    pub health_score: f64,
    /// Analysis severity
    pub severity: AnalysisSeverity,
    /// Detected bottlenecks
    pub bottlenecks: Vec<Bottleneck>,
    /// Analysis summary
    pub summary: String,
    /// Detailed findings
    pub findings: Vec<String>,
    /// Analysis timestamp
    pub analyzed_at: i64,
}

impl AnalysisResult {
    /// Creates a new analysis result
    pub fn new(health_score: f64) -> Self {
        let severity = if health_score >= 0.8 {
            AnalysisSeverity::Normal
        } else if health_score >= 0.6 {
            AnalysisSeverity::Warning
        } else if health_score >= 0.4 {
            AnalysisSeverity::Error
        } else {
            AnalysisSeverity::Critical
        };

        Self {
            health_score,
            severity,
            bottlenecks: Vec::new(),
            summary: String::new(),
            findings: Vec::new(),
            analyzed_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Adds a bottleneck to the result
    pub fn add_bottleneck(&mut self, bottleneck: Bottleneck) {
        self.bottlenecks.push(bottleneck);
    }

    /// Adds a finding to the result
    pub fn add_finding(&mut self, finding: String) {
        self.findings.push(finding);
    }

    /// Sets the summary
    pub fn with_summary(mut self, summary: String) -> Self {
        self.summary = summary;
        self
    }
}

/// Performance analyzer trait
pub trait PerformanceAnalyzer: Send + Sync {
    /// Analyzes current metrics
    fn analyze(&self, metrics: &SystemMetrics) -> Result<AnalysisResult>;

    /// Analyzes metrics history
    fn analyze_history(&self, history: &MetricsHistory) -> Result<AnalysisResult>;

    /// Detects anomalies in metrics
    fn detect_anomalies(&self, metrics: &SystemMetrics) -> Vec<String>;
}

/// Default performance analyzer implementation
pub struct DefaultPerformanceAnalyzer {
    /// Bottleneck detector
    bottleneck_detector: Box<dyn BottleneckDetector>,
}

impl DefaultPerformanceAnalyzer {
    /// Creates a new default performance analyzer
    pub fn new() -> Self {
        Self {
            bottleneck_detector: Box::new(DefaultBottleneckDetector::new()),
        }
    }

    /// Generates analysis summary
    fn generate_summary(&self, result: &AnalysisResult) -> String {
        match result.severity {
            AnalysisSeverity::Normal => {
                format!(
                    "System is operating normally. Health score: {:.1}%",
                    result.health_score * 100.0
                )
            }
            AnalysisSeverity::Warning => {
                format!(
                    "System performance is degraded. {} bottleneck(s) detected.",
                    result.bottlenecks.len()
                )
            }
            AnalysisSeverity::Error => {
                format!(
                    "System is experiencing significant performance issues. {} bottleneck(s) detected.",
                    result.bottlenecks.len()
                )
            }
            AnalysisSeverity::Critical => {
                "Critical performance issues detected. Immediate attention required.".to_string()
            }
        }
    }

    /// Analyzes CPU metrics
    fn analyze_cpu(&self, metrics: &SystemMetrics, findings: &mut Vec<String>) {
        if metrics.cpu_usage > 0.90 {
            findings.push(format!(
                "CPU usage is critically high at {:.1}%",
                metrics.cpu_usage * 100.0
            ));
        } else if metrics.cpu_usage > 0.80 {
            findings.push(format!(
                "CPU usage is elevated at {:.1}%",
                metrics.cpu_usage * 100.0
            ));
        }
    }

    /// Analyzes memory metrics
    fn analyze_memory(&self, metrics: &SystemMetrics, findings: &mut Vec<String>) {
        if metrics.memory_usage > 0.90 {
            findings.push(format!(
                "Memory usage is critically high at {:.1}%",
                metrics.memory_usage * 100.0
            ));
        } else if metrics.memory_usage > 0.85 {
            findings.push(format!(
                "Memory usage is elevated at {:.1}%",
                metrics.memory_usage * 100.0
            ));
        }
    }

    /// Analyzes cache metrics
    fn analyze_cache(&self, metrics: &SystemMetrics, findings: &mut Vec<String>) {
        if metrics.cache_hit_rate < 0.50 {
            findings.push(format!(
                "Cache hit rate is low at {:.1}%",
                metrics.cache_hit_rate * 100.0
            ));
        }
    }

    /// Analyzes database metrics
    fn analyze_database(&self, metrics: &SystemMetrics, findings: &mut Vec<String>) {
        if metrics.avg_query_time > 200.0 {
            findings.push(format!(
                "Database queries are slow with average time of {:.1}ms",
                metrics.avg_query_time
            ));
        }
    }

    /// Analyzes response time metrics
    fn analyze_response_time(&self, metrics: &SystemMetrics, findings: &mut Vec<String>) {
        if metrics.avg_response_time > 500.0 {
            findings.push(format!(
                "Response time is slow at {:.1}ms",
                metrics.avg_response_time
            ));
        } else if metrics.avg_response_time > 200.0 {
            findings.push(format!(
                "Response time is elevated at {:.1}ms",
                metrics.avg_response_time
            ));
        }
    }
}

impl Default for DefaultPerformanceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceAnalyzer for DefaultPerformanceAnalyzer {
    fn analyze(&self, metrics: &SystemMetrics) -> Result<AnalysisResult> {
        let health_score = metrics.health_score();
        let mut result = AnalysisResult::new(health_score);

        // Detect bottlenecks
        let bottlenecks = self.bottleneck_detector.detect(metrics);
        let prioritized = self.bottleneck_detector.prioritize(&bottlenecks);
        for bottleneck in prioritized {
            result.add_bottleneck(bottleneck);
        }

        // Generate findings
        let mut findings = Vec::new();
        self.analyze_cpu(metrics, &mut findings);
        self.analyze_memory(metrics, &mut findings);
        self.analyze_cache(metrics, &mut findings);
        self.analyze_database(metrics, &mut findings);
        self.analyze_response_time(metrics, &mut findings);

        result.findings = findings;

        // Generate summary
        let summary = self.generate_summary(&result);
        result = result.with_summary(summary);

        Ok(result)
    }

    fn analyze_history(&self, history: &MetricsHistory) -> Result<AnalysisResult> {
        if let Some(latest) = history.latest() {
            let mut result = self.analyze(latest)?;

            // Detect bottlenecks from history
            let historical_bottlenecks = self.bottleneck_detector.detect_from_history(history);
            for bottleneck in historical_bottlenecks {
                result.add_bottleneck(bottleneck);
            }

            // Add historical findings
            let avg_cpu = history.avg_cpu_usage();
            let peak_cpu = history.peak_cpu_usage();
            if peak_cpu > 0.90 {
                result.add_finding(format!(
                    "Peak CPU usage reached {:.1}% (average: {:.1}%)",
                    peak_cpu * 100.0,
                    avg_cpu * 100.0
                ));
            }

            let avg_memory = history.avg_memory_usage();
            let peak_memory = history.peak_memory_usage();
            if peak_memory > 0.90 {
                result.add_finding(format!(
                    "Peak memory usage reached {:.1}% (average: {:.1}%)",
                    peak_memory * 100.0,
                    avg_memory * 100.0
                ));
            }

            let cpu_trend = history.cpu_trend();
            if cpu_trend > 0.1 {
                result.add_finding(format!(
                    "CPU usage is trending upward (+{:.1}%)",
                    cpu_trend * 100.0
                ));
            } else if cpu_trend < -0.1 {
                result.add_finding(format!(
                    "CPU usage is trending downward ({:.1}%)",
                    cpu_trend * 100.0
                ));
            }

            Ok(result)
        } else {
            Err(crate::error::Error::Internal(
                "No metrics available in history".to_string(),
            ))
        }
    }

    fn detect_anomalies(&self, metrics: &SystemMetrics) -> Vec<String> {
        let mut anomalies = Vec::new();

        // Check for extreme values
        if metrics.cpu_usage > 0.95 {
            anomalies.push("Extreme CPU usage detected".to_string());
        }

        if metrics.memory_usage > 0.95 {
            anomalies.push("Extreme memory usage detected".to_string());
        }

        if metrics.error_rate > 0.05 {
            anomalies.push(format!(
                "High error rate detected: {:.1}%",
                metrics.error_rate * 100.0
            ));
        }

        if metrics.avg_response_time > 1000.0 {
            anomalies.push(format!(
                "Extremely slow response time: {:.1}ms",
                metrics.avg_response_time
            ));
        }

        if metrics.cache_hit_rate < 0.20 {
            anomalies.push(format!(
                "Extremely low cache hit rate: {:.1}%",
                metrics.cache_hit_rate * 100.0
            ));
        }

        anomalies
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_severity_determination() {
        let high_score = AnalysisResult::new(0.85);
        assert_eq!(high_score.severity, AnalysisSeverity::Normal);

        let medium_score = AnalysisResult::new(0.65);
        assert_eq!(medium_score.severity, AnalysisSeverity::Warning);

        let low_score = AnalysisResult::new(0.45);
        assert_eq!(low_score.severity, AnalysisSeverity::Error);

        let critical_score = AnalysisResult::new(0.25);
        assert_eq!(critical_score.severity, AnalysisSeverity::Critical);
    }

    #[test]
    fn test_analyze_healthy_system() {
        let analyzer = DefaultPerformanceAnalyzer::new();
        let metrics = SystemMetrics::new()
            .with_cpu_usage(0.50)
            .with_memory_usage(0.60)
            .with_cache_hit_rate(0.80);

        let result = analyzer.analyze(&metrics).unwrap();
        assert!(result.health_score > 0.8);
        assert_eq!(result.severity, AnalysisSeverity::Normal);
    }

    #[test]
    fn test_analyze_degraded_system() {
        let analyzer = DefaultPerformanceAnalyzer::new();
        let metrics = SystemMetrics::new()
            .with_cpu_usage(0.90)
            .with_memory_usage(0.90);

        let result = analyzer.analyze(&metrics).unwrap();
        assert!(result.health_score < 0.8);
        assert!(!result.bottlenecks.is_empty());
    }

    #[test]
    fn test_detect_anomalies() {
        let analyzer = DefaultPerformanceAnalyzer::new();
        let metrics = SystemMetrics::new().with_cpu_usage(0.98);

        let anomalies = analyzer.detect_anomalies(&metrics);
        assert!(!anomalies.is_empty());
    }

    #[test]
    fn test_analyze_history() {
        let analyzer = DefaultPerformanceAnalyzer::new();
        let mut history = MetricsHistory::new(3600);

        // Add some metrics
        history.add(SystemMetrics::new().with_cpu_usage(0.50));
        history.add(SystemMetrics::new().with_cpu_usage(0.70));
        history.add(SystemMetrics::new().with_cpu_usage(0.90));

        let result = analyzer.analyze_history(&history).unwrap();
        assert!(!result.findings.is_empty());
    }
}
