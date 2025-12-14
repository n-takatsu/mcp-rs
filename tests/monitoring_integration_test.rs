//! 監視システムの統合テスト

use mcp_rs::analytics::{AnomalyDetectionAlgorithm, AnomalyDetector};
use mcp_rs::monitoring::{
    alerts::{AlertCondition, AlertLevel, AlertManager, AlertRule, Comparison},
    dashboard::{DashboardConfig, DashboardManager, DashboardWidget, WidgetType},
    MetricPoint, MetricType, RealtimeMetrics, RealtimeMonitor, SystemMetricsCollector,
};
use std::time::Duration;

#[tokio::test]
async fn test_metrics_collection() {
    let mut monitor = RealtimeMonitor::new(Duration::from_millis(100), 100);

    monitor.add_collector(Box::new(SystemMetricsCollector::new()));

    // メトリクスへの参照を取得
    let metrics = monitor.metrics().clone();

    // メトリクス収集を開始
    let _ = monitor.start().await;

    // 少し待機してメトリクスが収集されるのを待つ
    tokio::time::sleep(Duration::from_millis(300)).await;

    let all_metrics = metrics.get_all().await;
    // メトリクスが収集されているはず
    assert!(!all_metrics.is_empty());
}

#[tokio::test]
async fn test_alert_system() {
    let manager = AlertManager::new();

    // ルールを追加
    manager
        .add_rule(AlertRule::new(
            "test-rule",
            "Test Rule",
            AlertCondition::Threshold {
                metric_type: MetricType::Cpu,
                threshold: 80.0,
                comparison: Comparison::GreaterThan,
            },
            AlertLevel::Warning,
        ))
        .await;

    // アラートをチェック
    let test_metrics = vec![MetricPoint::new(MetricType::Cpu, 90.0)];
    let alerts = manager.evaluate_metrics(&test_metrics).await;
    assert!(!alerts.is_empty());
}

#[tokio::test]
async fn test_anomaly_detection() {
    let mut detector =
        AnomalyDetector::new(100, AnomalyDetectionAlgorithm::ZScore { threshold: 3.0 });

    // 正常データを追加
    for i in 0..10 {
        detector.add_point(48.0 + i as f64);
    }

    let result = detector.detect(52.0);
    assert!(!result.is_anomaly);

    let result = detector.detect(150.0);
    assert!(result.is_anomaly);
}

#[tokio::test]
async fn test_dashboard_integration() {
    let metrics = RealtimeMetrics::new(100);

    // メトリクス追加
    metrics
        .add_metric(MetricPoint::new(MetricType::Cpu, 50.0))
        .await;
    metrics
        .add_metric(MetricPoint::new(MetricType::Memory, 60.0))
        .await;

    let mut config = DashboardConfig::new("Test Dashboard");
    config.add_widget(DashboardWidget::new(
        "cpu-widget",
        "CPU",
        WidgetType::Gauge,
        MetricType::Cpu,
    ));

    let dashboard = DashboardManager::new(metrics, config);

    // ダッシュボードデータ取得
    let data = dashboard.get_widget_data("cpu-widget").await;
    assert!(data.is_some());
    assert_eq!(data.unwrap().current_value, 50.0);
}

#[tokio::test]
async fn test_custom_metrics() {
    let metrics = RealtimeMetrics::new(100);

    // カスタムメトリクスを追加
    metrics
        .add_metric(MetricPoint::new(
            MetricType::Custom("request_count".to_string()),
            150.0,
        ))
        .await;
    metrics
        .add_metric(MetricPoint::new(
            MetricType::Custom("request_count".to_string()),
            200.0,
        ))
        .await;

    let all = metrics.get_all().await;
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_alert_triggering() {
    let manager = AlertManager::new();

    // 高CPU使用率ルール
    manager
        .add_rule(AlertRule::new(
            "high-cpu",
            "High CPU",
            AlertCondition::Threshold {
                metric_type: MetricType::Cpu,
                threshold: 95.0,
                comparison: Comparison::GreaterThan,
            },
            AlertLevel::Critical,
        ))
        .await;

    // アラートトリガー
    let test_metrics = vec![MetricPoint::new(MetricType::Cpu, 98.0)];
    let alerts = manager.evaluate_metrics(&test_metrics).await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].level, AlertLevel::Critical);
}

#[tokio::test]
async fn test_metrics_history() {
    let metrics = RealtimeMetrics::new(10);

    // メトリクスを追加
    for i in 0..5 {
        metrics
            .add_metric(MetricPoint::new(MetricType::Cpu, 50.0 + i as f64))
            .await;
    }

    let history = metrics.get_all().await;
    assert_eq!(history.len(), 5);
}

#[tokio::test]
async fn test_iqr_anomaly_detection() {
    let mut detector =
        AnomalyDetector::new(100, AnomalyDetectionAlgorithm::Iqr { multiplier: 1.5 });

    for value in [10.0, 12.0, 11.0, 13.0, 12.0, 11.0, 10.0, 13.0] {
        detector.add_point(value);
    }

    let normal = detector.detect(12.0);
    assert!(!normal.is_anomaly);

    let anomaly = detector.detect(100.0);
    assert!(anomaly.is_anomaly);
}

#[tokio::test]
async fn test_moving_average_detection() {
    let mut detector = AnomalyDetector::new(
        100,
        AnomalyDetectionAlgorithm::MovingAverage {
            window: 4,
            threshold: 20.0,
        },
    );

    for value in [10.0, 11.0, 12.0, 11.0, 10.0] {
        detector.add_point(value);
    }

    let normal = detector.detect(11.0);
    assert!(!normal.is_anomaly);

    let anomaly = detector.detect(50.0);
    assert!(anomaly.is_anomaly);
}

#[tokio::test]
async fn test_alert_history() {
    let manager = AlertManager::new();

    manager
        .add_rule(AlertRule::new(
            "test-rule",
            "Test",
            AlertCondition::Threshold {
                metric_type: MetricType::Cpu,
                threshold: 80.0,
                comparison: Comparison::GreaterThan,
            },
            AlertLevel::Warning,
        ))
        .await;

    // アラート発火
    let test_metrics = vec![MetricPoint::new(MetricType::Cpu, 98.0)];
    manager.evaluate_metrics(&test_metrics).await;

    let history = manager.get_active_alerts().await;
    assert!(!history.is_empty());
}

#[tokio::test]
async fn test_metric_stats() {
    let metrics = RealtimeMetrics::new(10);

    for value in [10.0, 20.0, 30.0, 40.0, 50.0] {
        metrics
            .add_metric(MetricPoint::new(MetricType::Cpu, value))
            .await;
    }

    let stats = metrics.get_statistics(&MetricType::Cpu).await;
    assert!(stats.is_some());
    assert_eq!(stats.unwrap().mean, 30.0);
}
