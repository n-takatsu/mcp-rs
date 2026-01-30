//! Plugin Metrics Collection for Prometheus
//!
//! This module provides metrics collection for MCP plugins,
//! exposing them in Prometheus format for monitoring.

use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramVec, IntCounter, IntCounterVec,
    IntGauge, IntGaugeVec, Opts, Registry,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// PluginMetrics collects and exposes metrics for MCP plugins
pub struct PluginMetrics {
    registry: Registry,
    
    // Plugin count metrics
    plugin_count: IntGauge,
    plugin_count_by_status: IntGaugeVec,
    
    // Resource usage metrics
    plugin_cpu_usage: GaugeVec,
    plugin_memory_usage: GaugeVec,
    plugin_disk_io: GaugeVec,
    plugin_network_tx: GaugeVec,
    plugin_network_rx: GaugeVec,
    
    // Container metrics
    container_restart_count: IntCounterVec,
    container_uptime_seconds: GaugeVec,
    
    // Request metrics
    plugin_requests_total: IntCounterVec,
    plugin_request_duration: HistogramVec,
    plugin_request_errors: IntCounterVec,
    
    // Security metrics
    security_violations: IntCounterVec,
    policy_violations: IntCounterVec,
    
    // Performance metrics
    plugin_response_time: HistogramVec,
}

impl PluginMetrics {
    /// Create a new PluginMetrics instance
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // Plugin count metrics
        let plugin_count = IntGauge::new("mcp_plugin_count", "Total number of plugins")?;
        registry.register(Box::new(plugin_count.clone()))?;

        let plugin_count_by_status = IntGaugeVec::new(
            Opts::new("mcp_plugin_count_by_status", "Number of plugins by status"),
            &["status"],
        )?;
        registry.register(Box::new(plugin_count_by_status.clone()))?;

        // Resource usage metrics
        let plugin_cpu_usage = GaugeVec::new(
            Opts::new("mcp_plugin_cpu_usage_percent", "Plugin CPU usage in percent"),
            &["plugin_id"],
        )?;
        registry.register(Box::new(plugin_cpu_usage.clone()))?;

        let plugin_memory_usage = GaugeVec::new(
            Opts::new("mcp_plugin_memory_usage_bytes", "Plugin memory usage in bytes"),
            &["plugin_id"],
        )?;
        registry.register(Box::new(plugin_memory_usage.clone()))?;

        let plugin_disk_io = GaugeVec::new(
            Opts::new("mcp_plugin_disk_io_bytes", "Plugin disk I/O in bytes"),
            &["plugin_id", "direction"],
        )?;
        registry.register(Box::new(plugin_disk_io.clone()))?;

        let plugin_network_tx = GaugeVec::new(
            Opts::new("mcp_plugin_network_tx_bytes", "Plugin network transmitted bytes"),
            &["plugin_id"],
        )?;
        registry.register(Box::new(plugin_network_tx.clone()))?;

        let plugin_network_rx = GaugeVec::new(
            Opts::new("mcp_plugin_network_rx_bytes", "Plugin network received bytes"),
            &["plugin_id"],
        )?;
        registry.register(Box::new(plugin_network_rx.clone()))?;

        // Container metrics
        let container_restart_count = IntCounterVec::new(
            Opts::new("mcp_container_restart_count", "Number of container restarts"),
            &["plugin_id"],
        )?;
        registry.register(Box::new(container_restart_count.clone()))?;

        let container_uptime_seconds = GaugeVec::new(
            Opts::new("mcp_container_uptime_seconds", "Container uptime in seconds"),
            &["plugin_id"],
        )?;
        registry.register(Box::new(container_uptime_seconds.clone()))?;

        // Request metrics
        let plugin_requests_total = IntCounterVec::new(
            Opts::new("mcp_plugin_requests_total", "Total number of plugin requests"),
            &["plugin_id", "method"],
        )?;
        registry.register(Box::new(plugin_requests_total.clone()))?;

        let plugin_request_duration = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "mcp_plugin_request_duration_seconds",
                "Plugin request duration in seconds",
            ),
            &["plugin_id", "method"],
        )?;
        registry.register(Box::new(plugin_request_duration.clone()))?;

        let plugin_request_errors = IntCounterVec::new(
            Opts::new("mcp_plugin_request_errors", "Number of plugin request errors"),
            &["plugin_id", "error_type"],
        )?;
        registry.register(Box::new(plugin_request_errors.clone()))?;

        // Security metrics
        let security_violations = IntCounterVec::new(
            Opts::new("mcp_security_violations", "Number of security violations"),
            &["plugin_id", "violation_type"],
        )?;
        registry.register(Box::new(security_violations.clone()))?;

        let policy_violations = IntCounterVec::new(
            Opts::new("mcp_policy_violations", "Number of policy violations"),
            &["plugin_id", "policy_name"],
        )?;
        registry.register(Box::new(policy_violations.clone()))?;

        // Performance metrics
        let plugin_response_time = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "mcp_plugin_response_time_seconds",
                "Plugin response time in seconds",
            ),
            &["plugin_id"],
        )?;
        registry.register(Box::new(plugin_response_time.clone()))?;

        Ok(Self {
            registry,
            plugin_count,
            plugin_count_by_status,
            plugin_cpu_usage,
            plugin_memory_usage,
            plugin_disk_io,
            plugin_network_tx,
            plugin_network_rx,
            container_restart_count,
            container_uptime_seconds,
            plugin_requests_total,
            plugin_request_duration,
            plugin_request_errors,
            security_violations,
            policy_violations,
            plugin_response_time,
        })
    }

    /// Update plugin count
    pub fn set_plugin_count(&self, count: i64) {
        self.plugin_count.set(count);
    }

    /// Update plugin count by status
    pub fn set_plugin_count_by_status(&self, status: &str, count: i64) {
        self.plugin_count_by_status.with_label_values(&[status]).set(count);
    }

    /// Update plugin CPU usage
    pub fn set_plugin_cpu_usage(&self, plugin_id: &str, cpu_percent: f64) {
        self.plugin_cpu_usage.with_label_values(&[plugin_id]).set(cpu_percent);
    }

    /// Update plugin memory usage
    pub fn set_plugin_memory_usage(&self, plugin_id: &str, memory_bytes: f64) {
        self.plugin_memory_usage.with_label_values(&[plugin_id]).set(memory_bytes);
    }

    /// Update plugin disk I/O
    pub fn set_plugin_disk_io(&self, plugin_id: &str, direction: &str, bytes: f64) {
        self.plugin_disk_io.with_label_values(&[plugin_id, direction]).set(bytes);
    }

    /// Update plugin network TX
    pub fn set_plugin_network_tx(&self, plugin_id: &str, bytes: f64) {
        self.plugin_network_tx.with_label_values(&[plugin_id]).set(bytes);
    }

    /// Update plugin network RX
    pub fn set_plugin_network_rx(&self, plugin_id: &str, bytes: f64) {
        self.plugin_network_rx.with_label_values(&[plugin_id]).set(bytes);
    }

    /// Increment container restart count
    pub fn increment_container_restart(&self, plugin_id: &str) {
        self.container_restart_count.with_label_values(&[plugin_id]).inc();
    }

    /// Update container uptime
    pub fn set_container_uptime(&self, plugin_id: &str, uptime_seconds: f64) {
        self.container_uptime_seconds.with_label_values(&[plugin_id]).set(uptime_seconds);
    }

    /// Increment request count
    pub fn increment_request(&self, plugin_id: &str, method: &str) {
        self.plugin_requests_total.with_label_values(&[plugin_id, method]).inc();
    }

    /// Observe request duration
    pub fn observe_request_duration(&self, plugin_id: &str, method: &str, duration_seconds: f64) {
        self.plugin_request_duration.with_label_values(&[plugin_id, method]).observe(duration_seconds);
    }

    /// Increment request error
    pub fn increment_request_error(&self, plugin_id: &str, error_type: &str) {
        self.plugin_request_errors.with_label_values(&[plugin_id, error_type]).inc();
    }

    /// Increment security violation
    pub fn increment_security_violation(&self, plugin_id: &str, violation_type: &str) {
        self.security_violations.with_label_values(&[plugin_id, violation_type]).inc();
    }

    /// Increment policy violation
    pub fn increment_policy_violation(&self, plugin_id: &str, policy_name: &str) {
        self.policy_violations.with_label_values(&[plugin_id, policy_name]).inc();
    }

    /// Observe response time
    pub fn observe_response_time(&self, plugin_id: &str, response_time_seconds: f64) {
        self.plugin_response_time.with_label_values(&[plugin_id]).observe(response_time_seconds);
    }

    /// Get the Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Gather all metrics
    pub fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }
}

impl Default for PluginMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create PluginMetrics")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = PluginMetrics::new().unwrap();
        assert!(metrics.gather().len() > 0);
    }

    #[test]
    fn test_plugin_count_metric() {
        let metrics = PluginMetrics::new().unwrap();
        metrics.set_plugin_count(5);
        
        let gathered = metrics.gather();
        assert!(!gathered.is_empty());
    }

    #[test]
    fn test_cpu_usage_metric() {
        let metrics = PluginMetrics::new().unwrap();
        metrics.set_plugin_cpu_usage("test-plugin", 45.5);
        
        let gathered = metrics.gather();
        assert!(!gathered.is_empty());
    }
}
