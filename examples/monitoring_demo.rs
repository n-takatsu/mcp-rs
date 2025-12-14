//! リアルタイム監視システムのデモ

use mcp_rs::monitoring::{
    alerts::{AlertCondition, AlertLevel, AlertManager, AlertRule, Comparison},
    dashboard::{DashboardConfig, DashboardManager, DashboardWidget, WidgetType},
    MetricPoint, MetricType, RealtimeMetrics, RealtimeMonitor, SystemMetricsCollector,
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("=== リアルタイム監視システム デモ ===\n");

    // 1. メトリクス収集の設定
    println!("1. メトリクス収集システムの初期化");
    let metrics = RealtimeMetrics::new(1000);
    let mut monitor = RealtimeMonitor::new(Duration::from_secs(1), 1000);

    // システムメトリクス収集器を追加
    monitor.add_collector(Box::new(SystemMetricsCollector::new()));

    // ダッシュボード設定
    let mut config = DashboardConfig::new("Main Dashboard");
    config.add_widget(DashboardWidget::new(
        "cpu-widget",
        "CPU Usage",
        WidgetType::Gauge,
        MetricType::Cpu,
    ));
    config.add_widget(DashboardWidget::new(
        "memory-widget",
        "Memory Usage",
        WidgetType::LineChart,
        MetricType::Memory,
    ));

    let dashboard = DashboardManager::new(metrics.clone(), config);

    // メトリクス収集を開始
    let _ = monitor.start().await;

    // 2. アラートマネージャーの設定
    println!("2. アラートシステムの初期化");
    let alert_manager = AlertManager::new();

    // カスタムルール追加
    alert_manager
        .add_rule(AlertRule::new(
            "cpu-high",
            "High CPU Usage",
            AlertCondition::Threshold {
                metric_type: MetricType::Cpu,
                threshold: 80.0,
                comparison: Comparison::GreaterThan,
            },
            AlertLevel::Warning,
        ))
        .await;

    println!("登録済みルール: 1 件\n");

    // 3. 異常検知器の初期化
    println!("3. 異常検知システムの初期化");
    use mcp_rs::analytics::{AnomalyDetectionAlgorithm, AnomalyDetector};
    let mut detector =
        AnomalyDetector::new(100, AnomalyDetectionAlgorithm::ZScore { threshold: 3.0 });

    // 4. メトリクス収集とモニタリング
    println!("4. メトリクス収集とリアルタイム監視開始\n");

    for i in 1..=10 {
        println!("--- サイクル {} ---", i);

        // シミュレーション: メトリクスを追加
        let cpu_value = 50.0 + (i as f64 * 5.0);
        let memory_value = 60.0 + (i as f64 * 3.0);

        metrics
            .add_metric(MetricPoint::new(MetricType::Cpu, cpu_value))
            .await;
        metrics
            .add_metric(MetricPoint::new(MetricType::Memory, memory_value))
            .await;

        // 少し待機
        tokio::time::sleep(Duration::from_millis(500)).await;

        // ダッシュボードデータ取得
        let widget_data = dashboard.get_all_widget_data().await;
        for data in &widget_data {
            println!(
                "ウィジェット {}: 現在値 {:.2}, 平均 {:.2}",
                data.widget_id, data.current_value, data.statistics.mean
            );
        }

        // アラートチェック
        let test_metrics = vec![MetricPoint::new(MetricType::Cpu, cpu_value)];
        let alerts = alert_manager.evaluate_metrics(&test_metrics).await;
        if !alerts.is_empty() {
            println!("\n⚠️ アラート発火:");
            for alert in &alerts {
                println!("  - [{:?}] {}", alert.level, alert.message);
            }
        }

        // 異常検知（CPU使用率）
        detector.add_point(cpu_value);
        let anomaly_result = detector.detect(cpu_value);

        if anomaly_result.is_anomaly {
            println!("\n🔍 異常検知:");
            println!("  - スコア: {:.2}", anomaly_result.score);
            println!("  - 説明: {}", anomaly_result.explanation);
        }

        // 統計情報表示
        if let Some(stats) = metrics.get_statistics(&MetricType::Cpu).await {
            println!("\n📊 統計情報:");
            println!(
                "  CPU - 平均: {:.2}%, 最小: {:.2}%, 最大: {:.2}%, 中央値: {:.2}%",
                stats.mean, stats.min, stats.max, stats.median,
            );
        }

        println!();
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // 5. アラート履歴表示
    println!("\n5. アラート履歴");
    let alert_history = alert_manager.get_active_alerts().await;
    if alert_history.is_empty() {
        println!("アラートなし");
    } else {
        for (idx, alert) in alert_history.iter().enumerate() {
            println!("{}. [{:?}] {}", idx + 1, alert.level, alert.message);
        }
    }

    // 6. アクティブなアラート
    println!("\n6. アクティブなアラート");
    let active_alerts = alert_manager.get_active_alerts().await;
    println!("アクティブアラート数: {}", active_alerts.len());

    println!("\n=== デモ完了 ===");
}
