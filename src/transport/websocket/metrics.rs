//! WebSocket Metrics Module
//!
//! Prometheusメトリクスの収集とエクスポート

use prometheus::{
    register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram, Registry,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;

/// WebSocketメトリクス収集
#[derive(Debug, Clone)]
pub struct WebSocketMetrics {
    /// 現在のWebSocket接続数（Gauge）
    connections_total: Gauge,
    /// 送信メッセージ総数（Counter）
    messages_sent_total: Counter,
    /// 受信メッセージ総数（Counter）
    messages_received_total: Counter,
    /// レイテンシ（Histogram）
    latency_seconds: Histogram,
    /// エラー総数（Counter）
    errors_total: Counter,
    /// Prometheusレジストリ
    registry: Arc<Registry>,
}

impl WebSocketMetrics {
    /// 新しいメトリクスインスタンスを作成
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Arc::new(Registry::new());

        // Gauge: 現在のWebSocket接続数
        let connections_total = Gauge::new(
            "websocket_connections_total",
            "Total number of active WebSocket connections",
        )?;
        registry.register(Box::new(connections_total.clone()))?;

        // Counter: 送信メッセージ総数
        let messages_sent_total = Counter::new(
            "websocket_messages_sent_total",
            "Total number of messages sent via WebSocket",
        )?;
        registry.register(Box::new(messages_sent_total.clone()))?;

        // Counter: 受信メッセージ総数
        let messages_received_total = Counter::new(
            "websocket_messages_received_total",
            "Total number of messages received via WebSocket",
        )?;
        registry.register(Box::new(messages_received_total.clone()))?;

        // Histogram: レイテンシ（秒単位）
        // バケット: 1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s, 10s
        let latency_seconds = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "websocket_latency_seconds",
                "WebSocket message latency in seconds",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]),
        )?;
        registry.register(Box::new(latency_seconds.clone()))?;

        // Counter: エラー総数
        let errors_total =
            Counter::new("websocket_errors_total", "Total number of WebSocket errors")?;
        registry.register(Box::new(errors_total.clone()))?;

        Ok(Self {
            connections_total,
            messages_sent_total,
            messages_received_total,
            latency_seconds,
            errors_total,
            registry,
        })
    }

    /// 接続数を増加
    pub fn increment_connections(&self) {
        self.connections_total.inc();
    }

    /// 接続数を減少
    pub fn decrement_connections(&self) {
        self.connections_total.dec();
    }

    /// 現在の接続数を設定
    pub fn set_connections(&self, count: usize) {
        self.connections_total.set(count as f64);
    }

    /// 送信メッセージをカウント
    pub fn increment_messages_sent(&self) {
        self.messages_sent_total.inc();
    }

    /// 送信メッセージを複数カウント
    pub fn increment_messages_sent_by(&self, count: u64) {
        self.messages_sent_total.inc_by(count as f64);
    }

    /// 受信メッセージをカウント
    pub fn increment_messages_received(&self) {
        self.messages_received_total.inc();
    }

    /// 受信メッセージを複数カウント
    pub fn increment_messages_received_by(&self, count: u64) {
        self.messages_received_total.inc_by(count as f64);
    }

    /// レイテンシを記録（秒単位）
    pub fn observe_latency(&self, seconds: f64) {
        self.latency_seconds.observe(seconds);
    }

    /// レイテンシを記録（Instant から計算）
    pub fn observe_latency_since(&self, start: Instant) {
        let elapsed = start.elapsed();
        self.latency_seconds.observe(elapsed.as_secs_f64());
    }

    /// エラーをカウント
    pub fn increment_errors(&self) {
        self.errors_total.inc();
    }

    /// エラーを複数カウント
    pub fn increment_errors_by(&self, count: u64) {
        self.errors_total.inc_by(count as f64);
    }

    /// Prometheusレジストリを取得
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// メトリクスをテキスト形式でエクスポート
    pub fn export_text(&self) -> Result<String, prometheus::Error> {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer).unwrap_or_default())
    }

    /// メトリクススナップショットを取得
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            connections_total: self.connections_total.get() as u64,
            messages_sent_total: self.messages_sent_total.get() as u64,
            messages_received_total: self.messages_received_total.get() as u64,
            errors_total: self.errors_total.get() as u64,
        }
    }
}

impl Default for WebSocketMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create WebSocketMetrics")
    }
}

/// メトリクススナップショット
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// 現在の接続数
    pub connections_total: u64,
    /// 送信メッセージ総数
    pub messages_sent_total: u64,
    /// 受信メッセージ総数
    pub messages_received_total: u64,
    /// エラー総数
    pub errors_total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = WebSocketMetrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_connections_tracking() {
        let metrics = WebSocketMetrics::new().unwrap();

        metrics.increment_connections();
        assert_eq!(metrics.connections_total.get() as u64, 1);

        metrics.increment_connections();
        metrics.increment_connections();
        assert_eq!(metrics.connections_total.get() as u64, 3);

        metrics.decrement_connections();
        assert_eq!(metrics.connections_total.get() as u64, 2);

        metrics.set_connections(10);
        assert_eq!(metrics.connections_total.get() as u64, 10);
    }

    #[test]
    fn test_messages_tracking() {
        let metrics = WebSocketMetrics::new().unwrap();

        metrics.increment_messages_sent();
        assert_eq!(metrics.messages_sent_total.get() as u64, 1);

        metrics.increment_messages_sent_by(5);
        assert_eq!(metrics.messages_sent_total.get() as u64, 6);

        metrics.increment_messages_received();
        assert_eq!(metrics.messages_received_total.get() as u64, 1);

        metrics.increment_messages_received_by(10);
        assert_eq!(metrics.messages_received_total.get() as u64, 11);
    }

    #[test]
    fn test_errors_tracking() {
        let metrics = WebSocketMetrics::new().unwrap();

        metrics.increment_errors();
        assert_eq!(metrics.errors_total.get() as u64, 1);

        metrics.increment_errors_by(3);
        assert_eq!(metrics.errors_total.get() as u64, 4);
    }

    #[test]
    fn test_latency_observation() {
        let metrics = WebSocketMetrics::new().unwrap();

        metrics.observe_latency(0.001); // 1ms
        metrics.observe_latency(0.05); // 50ms
        metrics.observe_latency(1.0); // 1s

        let start = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        metrics.observe_latency_since(start);

        // ヒストグラムに記録されていることを確認（詳細な値はチェックしない）
        assert!(metrics.latency_seconds.get_sample_count() > 0);
    }

    #[test]
    fn test_snapshot() {
        let metrics = WebSocketMetrics::new().unwrap();

        metrics.set_connections(5);
        metrics.increment_messages_sent_by(10);
        metrics.increment_messages_received_by(20);
        metrics.increment_errors_by(2);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.connections_total, 5);
        assert_eq!(snapshot.messages_sent_total, 10);
        assert_eq!(snapshot.messages_received_total, 20);
        assert_eq!(snapshot.errors_total, 2);
    }

    #[test]
    fn test_export_text() {
        let metrics = WebSocketMetrics::new().unwrap();

        metrics.increment_connections();
        metrics.increment_messages_sent();
        metrics.increment_messages_received();

        let export = metrics.export_text();
        assert!(export.is_ok());

        let text = export.unwrap();
        assert!(text.contains("websocket_connections_total"));
        assert!(text.contains("websocket_messages_sent_total"));
        assert!(text.contains("websocket_messages_received_total"));
    }
}
