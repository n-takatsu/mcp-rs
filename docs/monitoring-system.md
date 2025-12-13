# リアルタイム監視分析システム

## 概要

リアルタイム監視分析システムは、MCPサーバーのパフォーマンスを継続的に監視し、異常を検知してアラートを発火する包括的な監視ソリューションです。

## 主要機能

### 1. メトリクス収集

システムメトリクスを定期的に収集し、履歴データを保持します。

```rust
use mcp_rs::monitoring::collector::{CollectorConfig, MetricsCollector};

let config = CollectorConfig {
    collection_interval_secs: 1,
    history_size: 3600, // 1時間分
};

let mut collector = MetricsCollector::new(config);
collector.collect().await;

let current = collector.current_metrics();
println!("CPU: {:.2}%", current.cpu_usage);
println!("Memory: {:.2}%", current.memory_usage);
```

#### 収集されるメトリクス

- **CPU使用率**: プロセスのCPU使用率（%）
- **メモリ使用率**: プロセスのメモリ使用率（%）
- **リクエスト数**: 処理されたリクエスト総数
- **エラー率**: エラーレスポンスの割合（%）
- **応答時間**: 平均応答時間（ミリ秒）
- **アクティブ接続数**: 現在のアクティブ接続数
- **スループット**: ネットワークスループット（Mbps）
- **ディスク使用率**: ディスク使用率（%）

### 2. アラートシステム

メトリクスが閾値を超えた場合にアラートを発火します。

```rust
use mcp_rs::monitoring::alerts::{AlertManager, AlertRule, AlertLevel};
use mcp_rs::monitoring::metrics::MetricType;

let mut manager = AlertManager::new();

// デフォルトルールを追加
manager.add_default_rules();

// カスタムルールを追加
manager.add_rule(AlertRule::new(
    "high_cpu".to_string(),
    MetricType::CpuUsage,
    AlertLevel::Critical,
    95.0,
    "CPU usage exceeded 95%".to_string(),
));

// アラートチェック
let alerts = manager.check_alerts(98.0, MetricType::CpuUsage);
for alert in alerts {
    println!("[{:?}] {}", alert.level, alert.message);
}
```

#### アラートレベル

- **Info**: 情報提供レベル
- **Warning**: 警告レベル（注意が必要）
- **Error**: エラーレベル（対応が必要）
- **Critical**: 重大レベル（即座の対応が必要）

#### デフォルトアラートルール

1. **高CPU使用率**: CPU > 90% → Critical
2. **高メモリ使用率**: Memory > 85% → Warning
3. **高エラー率**: Error Rate > 5% → Error

### 3. 異常検知

統計的手法を用いてメトリクスの異常を検知します。

```rust
use mcp_rs::monitoring::detector::AnomalyDetector;
use mcp_rs::monitoring::metrics::MetricStats;

let detector = AnomalyDetector::new();

// Z-scoreベースの異常検知
let values = vec![10.0, 12.0, 11.0, 13.0, 50.0];
let stats = MetricStats::from_values(values.clone());
let result = detector.detect_zscore(50.0, &stats);

if result.is_anomaly {
    println!("異常検知: スコア {:.2}, {}", result.score, result.reason);
}

// IQRベースの異常検知（外れ値検出）
let results = detector.detect_iqr(&values);
for (i, result) in results.iter().enumerate() {
    if result.is_anomaly {
        println!("値 {} は異常: {}", values[i], result.reason);
    }
}

// 移動平均ベースの異常検知
let result = detector.detect_moving_average(&values, 4);
if result.is_anomaly {
    println!("移動平均からの逸脱を検知: {}", result.reason);
}
```

#### 異常検知手法

1. **Z-score検知**: 標準偏差ベースの異常検知（3σルール）
2. **IQR検知**: 四分位範囲ベースの外れ値検出
3. **移動平均検知**: 移動平均からの大きな逸脱を検知

### 4. ダッシュボードAPI

リアルタイムダッシュボード用のAPIを提供します。

```rust
use mcp_rs::monitoring::dashboard::DashboardManager;
use std::sync::Arc;
use tokio::sync::RwLock;

let collector = Arc::new(RwLock::new(MetricsCollector::new(config)));
let dashboard = DashboardManager::new(collector);

// ダッシュボードデータ取得
let response = dashboard.get_dashboard().await;
println!("CPU: {:.2}% (avg: {:.2}%, p95: {:.2}%)",
    response.current.cpu_usage,
    response.stats.cpu_stats.avg,
    response.stats.cpu_stats.p95,
);

// 時系列データ取得
let cpu_timeseries = dashboard.get_metric_timeseries(
    MetricType::CpuUsage,
    60 // 直近60サンプル
).await;

// メトリクス履歴取得
let history = dashboard.get_metrics_history(10).await;
```

## アーキテクチャ

```
┌─────────────────────────────────────────┐
│      リアルタイム監視システム              │
├─────────────────────────────────────────┤
│                                         │
│  ┌──────────────┐   ┌───────────────┐  │
│  │ Metrics      │   │ Dashboard     │  │
│  │ Collector    │──▶│ Manager       │  │
│  └──────────────┘   └───────────────┘  │
│         │                    │          │
│         │                    │          │
│         ▼                    ▼          │
│  ┌──────────────┐   ┌───────────────┐  │
│  │ Alert        │   │ Anomaly       │  │
│  │ Manager      │   │ Detector      │  │
│  └──────────────┘   └───────────────┘  │
│                                         │
└─────────────────────────────────────────┘
```

### コンポーネント

1. **MetricsCollector**: システムメトリクスの収集と履歴管理
2. **AlertManager**: アラートルールの管理とアラート発火
3. **AnomalyDetector**: 統計的異常検知
4. **DashboardManager**: ダッシュボードAPIの提供

## 使用例

### 基本的な監視システム

```rust
use mcp_rs::monitoring::{
    collector::{CollectorConfig, MetricsCollector},
    alerts::AlertManager,
    dashboard::DashboardManager,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    // 初期化
    let config = CollectorConfig {
        collection_interval_secs: 1,
        history_size: 3600,
    };
    
    let collector = Arc::new(RwLock::new(MetricsCollector::new(config)));
    let dashboard = DashboardManager::new(collector.clone());
    let mut alert_manager = AlertManager::new();
    alert_manager.add_default_rules();
    
    // 監視ループ
    loop {
        // メトリクス収集
        {
            let mut c = collector.write().await;
            c.collect().await;
        }
        
        // ダッシュボードデータ取得
        let data = dashboard.get_dashboard().await;
        
        // アラートチェック
        let alerts = alert_manager.check_alerts(
            data.current.cpu_usage,
            MetricType::CpuUsage,
        );
        
        for alert in alerts {
            eprintln!("[ALERT] {:?}: {}", alert.level, alert.message);
        }
        
        sleep(Duration::from_secs(1)).await;
    }
}
```

### カスタムメトリクスの記録

```rust
let mut collector = MetricsCollector::new(config);

// リクエストを記録
collector.record_request(200, 150); // 200 OK, 150ms
collector.record_request(500, 100); // 500 Error, 100ms

let metrics = collector.current_metrics();
println!("Total requests: {}", metrics.request_count);
println!("Error rate: {:.2}%", metrics.error_rate);
println!("Avg response time: {:.2}ms", metrics.response_time_ms);
```

## 設定

### CollectorConfig

```rust
pub struct CollectorConfig {
    /// 収集間隔（秒）
    pub collection_interval_secs: u64,
    
    /// 履歴サイズ（サンプル数）
    pub history_size: usize,
}
```

推奨設定:
- 開発環境: `collection_interval_secs: 5, history_size: 720` (1時間)
- 本番環境: `collection_interval_secs: 1, history_size: 3600` (1時間)

## パフォーマンス

- メトリクス収集オーバーヘッド: < 1ms
- メモリ使用量: 約100KB（3600サンプル保持時）
- CPU使用率: < 0.1%（1秒間隔収集時）

## ベストプラクティス

1. **適切な履歴サイズ**: 必要な期間のデータを保持し、メモリ使用量とのバランスを取る
2. **アラートルールの調整**: 環境に応じて閾値を調整
3. **異常検知の活用**: 動的な異常検知で予期しない問題を早期発見
4. **ダッシュボード統合**: WebUIやグラフツールとの統合でリアルタイム可視化

## トラブルシューティング

### メトリクスが収集されない

- `sysinfo` crateが正しくインストールされているか確認
- 必要な権限があるか確認（特にLinux環境）

### アラートが発火しない

- アラートルールの閾値を確認
- メトリクスが正しく収集されているか確認
- `check_alerts()`が定期的に呼ばれているか確認

### 異常検知の誤検知が多い

- Z-scoreの閾値を調整（デフォルト: 3.0）
- IQR検知の係数を調整
- 移動平均のウィンドウサイズを調整

## 今後の拡張

- [ ] Prometheusエクスポーター
- [ ] Grafanaダッシュボード
- [ ] WebSocket APIによるリアルタイムストリーミング
- [ ] 機械学習ベースの異常検知
- [ ] 分散トレーシング統合

## ライセンス

MIT/Apache-2.0
