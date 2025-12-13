//! 監視システムの統合テスト

use mcp_rs::monitoring::{
    alerts::{AlertLevel, AlertManager, AlertRule},
    collector::{CollectorConfig, MetricsCollector},
    dashboard::DashboardManager,
    detector::AnomalyDetector,
    metrics::{MetricStats, MetricType, SystemMetrics},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_metrics_collection() {
    let config = CollectorConfig {
        interval: Duration::from_secs(1),
        history_size: 100,
        enable_system_metrics: true,
    };

    let collector = MetricsCollector::new(config);

    // メトリクス収集を開始
    collector.start().await;

    // 少し待機
    tokio::time::sleep(Duration::from_millis(100)).await;

    let current = collector.get_latest().await;
    assert!(current.is_some());
}

#[tokio::test]
async fn test_alert_system() {
    let manager = AlertManager::new();

    // デフォルトルールを追加
    manager.add_default_rules().await;

    // アラートをチェック
    let metrics = SystemMetrics::new();
    let alerts = manager.check_metrics(&metrics).await;
    // 新しいメトリクスなのでアラートなし、または少数のアラート
    assert!(alerts.len() < 10);
}

#[tokio::test]
async fn test_anomaly_detection() {
    let detector = AnomalyDetector::new();

    // Z-score検知
    let values = vec![10.0, 12.0, 11.0, 13.0, 12.0];
    let stats = MetricStats::from_values(values);

    let result = detector.detect_zscore(12.0, &stats);
    assert!(!result.is_anomaly);

    let result = detector.detect_zscore(50.0, &stats);
    assert!(result.is_anomaly);
}

#[tokio::test]
async fn test_dashboard_integration() {
    let config = CollectorConfig {
        interval: Duration::from_secs(1),
        history_size: 100,
        enable_system_metrics: true,
    };

    let collector = Arc::new(RwLock::new(MetricsCollector::new(config)));
    let dashboard = DashboardManager::new(collector.clone());

    // メトリクス収集を開始
    collector.read().await.start().await;

    // 少し待機
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ダッシュボードデータ取得
    let response = dashboard.get_dashboard().await;
    assert!(response.current.cpu_usage >= 0.0);
}

#[tokio::test]
async fn test_custom_metrics() {
    let config = CollectorConfig {
        interval: Duration::from_secs(1),
        history_size: 100,
        enable_system_metrics: true,
    };

    let collector = MetricsCollector::new(config);

    // リクエストメトリクスを記録
    collector.start().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    collector.record_request(150.0, false).await;
    collector.record_request(200.0, false).await;
    collector.record_request(100.0, true).await;

    let current = collector.get_latest().await;
    assert!(current.is_some());
}

#[tokio::test]
async fn test_alert_triggering() {
    let manager = AlertManager::new();

    // 高CPU使用率ルール
    let rule = AlertRule {
        name: "high_cpu".to_string(),
        metric_type: MetricType::CpuUsage,
        threshold: 95.0,
        greater_than: true,
        level: AlertLevel::Critical,
        message_template: "CPU usage exceeded 95%".to_string(),
        enabled: true,
    };
    manager.add_rule(rule).await;

    // アラートトリガー
    let mut metrics = SystemMetrics::new();
    metrics.cpu_usage = 98.0;
    let alerts = manager.check_metrics(&metrics).await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].level, AlertLevel::Critical);
}

#[tokio::test]
async fn test_metrics_history() {
    let config = CollectorConfig {
        interval: Duration::from_millis(50),
        history_size: 10,
        enable_system_metrics: true,
    };

    let collector = MetricsCollector::new(config);

    // メトリクス収集を開始
    collector.start().await;

    // 少し待機して複数回収集
    tokio::time::sleep(Duration::from_millis(300)).await;

    let history = collector.get_history(10).await;
    assert!(!history.is_empty());
}

#[tokio::test]
async fn test_iqr_anomaly_detection() {
    let detector = AnomalyDetector::new();
    let values = vec![10.0, 12.0, 11.0, 13.0, 100.0]; // 100.0は異常値

    let results = detector.detect_iqr(&values);
    assert_eq!(results.len(), 5);
}

#[tokio::test]
async fn test_moving_average_detection() {
    let detector = AnomalyDetector::new();
    let values = vec![10.0, 11.0, 12.0, 11.0, 50.0]; // 50.0は異常値

    let result = detector.detect_moving_average(&values, 4);
    assert!(result.is_anomaly);
}

#[tokio::test]
async fn test_alert_history() {
    let manager = AlertManager::new();
    manager.add_default_rules().await;

    // アラート発火
    let mut metrics = SystemMetrics::new();
    metrics.cpu_usage = 98.0;
    manager.check_metrics(&metrics).await;

    let history = manager.get_alert_history(10).await;
    assert!(!history.is_empty());
}

#[tokio::test]
async fn test_metric_stats() {
    let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    let stats = MetricStats::from_values(values);

    assert_eq!(stats.mean, 30.0);
    assert!(stats.std_dev > 0.0);
}
