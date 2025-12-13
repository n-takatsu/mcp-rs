//! リアルタイム監視システムのデモ

use mcp_rs::monitoring::{
    alerts::{AlertLevel, AlertManager, AlertRule},
    collector::{CollectorConfig, MetricsCollector},
    dashboard::DashboardManager,
    detector::AnomalyDetector,
    metrics::{MetricStats, MetricType},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    println!("=== リアルタイム監視システム デモ ===\n");

    // 1. メトリクス収集の設定
    println!("1. メトリクス収集システムの初期化");
    let config = CollectorConfig {
        interval: Duration::from_secs(1),
        history_size: 100,
        enable_system_metrics: true,
    };

    let collector = Arc::new(RwLock::new(MetricsCollector::new(config)));
    let dashboard = DashboardManager::new(collector.clone());

    // メトリクス収集を開始
    collector.read().await.start().await;

    // 2. アラートマネージャーの設定
    println!("2. アラートシステムの初期化");
    let alert_manager = AlertManager::new();
    alert_manager.add_default_rules().await;

    // カスタムルール追加
    alert_manager
        .add_rule(AlertRule {
            name: "custom_high_requests".to_string(),
            metric_type: MetricType::RequestCount,
            threshold: 1000.0,
            greater_than: true,
            level: AlertLevel::Warning,
            message_template: "Request count exceeded 1000/sec".to_string(),
            enabled: true,
        })
        .await;

    println!("登録済みルール: 5 件\n");

    // 3. 異常検知器の初期化
    println!("3. 異常検知システムの初期化");
    let detector = AnomalyDetector::new();

    // 4. メトリクス収集とモニタリング
    println!("4. メトリクス収集とリアルタイム監視開始\n");

    for i in 1..=10 {
        println!("--- サイクル {} ---", i);

        // シミュレーション: リクエストを記録
        for _ in 0..10 {
            collector
                .read()
                .await
                .record_request(100.0 + (i as f64 * 10.0), false)
                .await;
        }

        // 少し待機してメトリクスが収集されるのを待つ
        sleep(Duration::from_millis(500)).await;

        // ダッシュボードデータ取得
        let dashboard_data = dashboard.get_dashboard().await;
        println!("CPU使用率: {:.2}%", dashboard_data.current.cpu_usage);
        println!("メモリ使用率: {:.2}%", dashboard_data.current.memory_usage);
        println!("リクエスト数: {}", dashboard_data.current.request_count);
        println!(
            "応答時間: {:.2}ms",
            dashboard_data.current.avg_response_time
        );
        println!("エラー率: {:.2}%", dashboard_data.current.error_rate());

        // アラートチェック
        let cpu_alerts = alert_manager.check_metrics(&dashboard_data.current).await;
        if !cpu_alerts.is_empty() {
            println!("\n⚠️ アラート発火:");
            for alert in &cpu_alerts {
                println!("  - [{:?}] {}", alert.level, alert.message);
            }
        }

        // 異常検知（CPU使用率）
        let cpu_history = dashboard
            .get_metric_timeseries(MetricType::CpuUsage, 10)
            .await;
        if cpu_history.len() >= 4 {
            let stats = MetricStats::from_values(cpu_history.clone());
            let anomaly_result = detector.detect_zscore(dashboard_data.current.cpu_usage, &stats);

            if anomaly_result.is_anomaly {
                println!("\n🔍 異常検知:");
                println!("  - スコア: {:.2}", anomaly_result.score);
                println!("  - 理由: {}", anomaly_result.reason);
            }
        }

        // 統計情報表示
        println!("\n📊 統計情報:");
        println!(
            "  CPU - 平均: {:.2}%, 最小: {:.2}%, 最大: {:.2}%, P95: {:.2}%",
            dashboard_data.stats.cpu_stats.avg,
            dashboard_data.stats.cpu_stats.min,
            dashboard_data.stats.cpu_stats.max,
            dashboard_data.stats.cpu_stats.p95,
        );

        println!();
        sleep(Duration::from_secs(2)).await;
    }

    // 5. アラート履歴表示
    println!("\n5. アラート履歴");
    let alert_history = alert_manager.get_alert_history(10).await;
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
