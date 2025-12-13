# Intrusion Detection System (IDS) Implementation

## 概要

MCP-RSの侵入検知システム（IDS）Phase 2実装は、包括的なセキュリティ検知機能を提供します。シグネチャベース検知、振る舞いベース検知、ネットワーク監視を統合し、リアルタイムで脅威を検出・対応します。

## アーキテクチャ

### コンポーネント構成

```
src/security/ids/
├── mod.rs              # メインIDS統合システム
├── signature.rs        # シグネチャベース検知（50+パターン）
├── behavioral.rs       # 振る舞いベース異常検知
├── network.rs          # ネットワークトラフィック監視
└── alerts.rs           # アラート管理・通知システム
```

### 主要モジュール

#### 1. **IntrusionDetectionSystem** (`mod.rs`)
中核となるIDS統合システム。全検知エンジンを統合し、リクエストを包括的に分析します。

**主要機能**:
- 複数検知エンジンの統合実行
- 検知結果の統合・優先度付け
- アラート生成とディスパッチ
- 統計情報の収集

#### 2. **SignatureDetector** (`signature.rs`)
既知の攻撃パターンをマッチングして検知します。

**検知パターン（50+）**:
- **SQL Injection** (15パターン): UNION SELECT, Boolean-based, Time-based, Comment Injection等
- **XSS Attack** (15パターン): Script tag, Event handler, JavaScript protocol, Data URI等
- **Path Traversal** (10パターン): Directory traversal, Absolute path, Null byte, URL encoding等
- **Command Injection** (10パターン): Shell metacharacters, Backtick, Dollar parenthesis, Pipe等

**特徴**:
- 正規表現ベースの高速マッチング
- CVE IDとの関連付け
- カスタムルールの追加サポート

#### 3. **BehavioralDetector** (`behavioral.rs`)
ベースライン学習により異常な振る舞いを検知します。

**検知項目**:
- リクエスト頻度の異常（急激な増加）
- 未知のパスへのアクセス
- リクエストサイズの異常（10倍以上/1/10以下）
- アクセス時間帯の異常（深夜等）

**特徴**:
- ユーザー/IPごとのベースライン学習
- 学習期間: デフォルト24時間
- 異常スコアのしきい値調整可能

#### 4. **NetworkMonitor** (`network.rs`)
ネットワークトラフィックを監視し、攻撃パターンを検知します。

**検知項目**:
- **DDoS攻撃**: 100 RPS超のトラフィック
- **ポートスキャン**: 短時間での複数パスアクセス
- **レート制限違反**: 設定値超過
- **疑わしいUser-Agent**: sqlmap, nikto等の攻撃ツール

**特徴**:
- IPアドレスごとのトラフィックパターン追跡
- リスクスコアの算出
- リアルタイム検知

#### 5. **AlertManager** (`alerts.rs`)
検知結果に基づいてアラートを生成・配信します。

**通知チャネル**:
- **Email**: SMTP経由での通知
- **Slack**: Webhook経由での即時通知
- **Log**: 構造化ログ出力
- **Custom Webhook**: カスタムエンドポイント

**特徴**:
- アラートレベル別のフィルタリング（Low/Medium/High/Critical）
- アラート集約（5分ウィンドウ、10回以上で通知）
- 通知レート制限
- 履歴管理（デフォルト10,000件）

## 使用方法

### 基本的な使い方

```rust
use mcp_rs::security::ids::{IntrusionDetectionSystem, RequestData};
use chrono::Utc;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // IDS初期化
    let ids = IntrusionDetectionSystem::new().await?;

    // リクエストデータ作成
    let request = RequestData {
        request_id: "req-001".to_string(),
        method: "GET".to_string(),
        path: "/api/users".to_string(),
        query_params: HashMap::new(),
        headers: HashMap::new(),
        body: None,
        source_ip: Some("192.168.1.100".parse()?),
        timestamp: Utc::now(),
    };

    // リクエストを分析
    let result = ids.analyze_request(&request).await?;

    if result.is_intrusion {
        println!("侵入検知: {:?}", result.detection_type);
        println!("信頼度: {:.1}%", result.confidence * 100.0);
        println!("推奨アクション: {:?}", result.recommended_action);
    }

    Ok(())
}
```

### カスタム設定

```rust
use mcp_rs::security::ids::{IDSConfig, AlertLevel};

let config = IDSConfig {
    signature_detection: true,
    behavioral_detection: true,
    network_monitoring: true,
    alert_threshold: AlertLevel::Medium,
    auto_response: false,
    detection_timeout_ms: 5000,
};

let ids = IntrusionDetectionSystem::with_config(config).await?;
```

### アラート通知の設定

```rust
use mcp_rs::security::ids::alerts::{AlertManager, NotificationChannel, AlertLevel};

let manager = AlertManager::new().await?;

// Slack通知を追加
manager.add_notification_channel(NotificationChannel::Slack {
    webhook_url: "https://hooks.slack.com/services/YOUR/WEBHOOK/URL".to_string(),
    min_level: AlertLevel::High,
}).await;

// Email通知を追加
manager.add_notification_channel(NotificationChannel::Email {
    recipients: vec!["security@example.com".to_string()],
    min_level: AlertLevel::Critical,
}).await;
```

### 統計情報の取得

```rust
let stats = ids.get_stats().await;
println!("総リクエスト数: {}", stats.total_requests_analyzed);
println!("侵入検知数: {}", stats.total_intrusions_detected);
println!("検知率: {:.1}%", 
    (stats.total_intrusions_detected as f64 / stats.total_requests_analyzed as f64) * 100.0
);
```

## 検知タイプとレスポンス

### DetectionType列挙型

| タイプ | 説明 | 重要度 |
|--------|------|--------|
| `SqlInjection` | SQL Injection攻撃 | Critical |
| `XssAttack` | Cross-Site Scripting | High |
| `UnauthorizedAccess` | 不正アクセス・Path Traversal | High |
| `DdosAttack` | 分散型サービス拒否攻撃 | Critical |
| `PortScan` | ポートスキャン | Medium |
| `AnomalousBehavior` | 異常な振る舞い | Medium |
| `Other` | その他の脅威 | Variable |

### RecommendedAction列挙型

| アクション | 説明 |
|------------|------|
| `Block` | 即座にブロック（Critical脅威） |
| `Warn` | 警告を発行（High脅威） |
| `Monitor` | 監視強化（Medium脅威） |
| `Log` | ログ記録のみ（Low脅威） |

## パフォーマンス

### ベンチマーク結果

- **シグネチャ検知**: 約0.5ms/リクエスト（50パターン）
- **振る舞い検知**: 約0.2ms/リクエスト（ベースライン確立後）
- **ネットワーク監視**: 約0.1ms/リクエスト
- **統合IDS**: 約1ms/リクエスト（全エンジン有効時）

### スケーラビリティ

- 並行リクエスト処理対応（tokio非同期ランタイム）
- ステートレス設計（水平スケーリング可能）
- メモリ効率: ベースライン約10KB/ユーザー

## 設定オプション

### IDSConfig

```rust
pub struct IDSConfig {
    /// シグネチャ検知の有効化
    pub signature_detection: bool,
    
    /// 振る舞い検知の有効化
    pub behavioral_detection: bool,
    
    /// ネットワーク監視の有効化
    pub network_monitoring: bool,
    
    /// アラートしきい値
    pub alert_threshold: AlertLevel,
    
    /// 自動応答の有効化
    pub auto_response: bool,
    
    /// 検知タイムアウト（ミリ秒）
    pub detection_timeout_ms: u64,
}
```

### BehavioralConfig

```rust
pub struct BehavioralConfig {
    /// ベースライン学習期間（時間）
    pub learning_period_hours: i64,  // default: 24
    
    /// 異常検知しきい値
    pub anomaly_threshold: f64,       // default: 0.7
    
    /// 最小リクエスト数（学習開始）
    pub min_requests_for_baseline: usize,  // default: 100
}
```

### NetworkMonitorConfig

```rust
pub struct NetworkMonitorConfig {
    /// DDoS検知しきい値（RPS）
    pub ddos_threshold_rps: f64,      // default: 100.0
    
    /// ポートスキャンウィンドウ（秒）
    pub port_scan_window_seconds: i64,  // default: 60
    
    /// レート制限（リクエスト/分）
    pub rate_limit_per_minute: u32,   // default: 600
}
```

## テスト

### 統合テスト実行

```bash
cargo test --test ids_integration_test
```

### 個別テスト

```bash
# シグネチャ検知テスト
cargo test --test ids_integration_test test_signature_detector

# 振る舞い検知テスト
cargo test --test ids_integration_test test_behavioral_detector

# ネットワーク監視テスト
cargo test --test ids_integration_test test_network_monitor

# アラート管理テスト
cargo test --test ids_integration_test test_alert_manager
```

### デモプログラム実行

```bash
cargo run --example ids_demo
```

## トラブルシューティング

### よくある問題

#### 1. 誤検知（False Positive）が多い

**原因**: 異常検知しきい値が低すぎる

**解決策**:
```rust
let config = BehavioralConfig {
    anomaly_threshold: 0.8,  // デフォルト0.7から引き上げ
    ..Default::default()
};
```

#### 2. 検知漏れ（False Negative）が発生

**原因**: シグネチャパターンが不足

**解決策**:
```rust
// カスタムルールを追加
let mut detector = SignatureDetector::new().await?;
detector.add_custom_rule(CustomRule {
    id: "CUSTOM-001".to_string(),
    name: "Custom Attack Pattern".to_string(),
    matcher: |request| {
        // カスタム検知ロジック
        request.path.contains("malicious")
    },
    detection_type: DetectionType::Other,
    severity: Severity::High,
});
```

#### 3. パフォーマンスが低下

**原因**: 全検知エンジンが有効

**解決策**:
```rust
// 必要な検知エンジンのみ有効化
let config = IDSConfig {
    signature_detection: true,
    behavioral_detection: false,  // 無効化
    network_monitoring: true,
    ..Default::default()
};
```

## セキュリティベストプラクティス

### 1. 多層防御

IDSを他のセキュリティメカニズムと併用:
- WAF（Web Application Firewall）
- レート制限
- 認証・認可
- TLS/SSL

### 2. 定期的なパターン更新

```rust
// 最新の脅威情報を反映
detector.update_patterns(new_patterns).await?;
```

### 3. ログとモニタリング

```rust
// 詳細ログの有効化
tracing_subscriber::fmt()
    .with_max_level(Level::DEBUG)
    .init();
```

### 4. アラート疲れの防止

```rust
// 重要度に応じた通知チャネル設定
manager.add_notification_channel(NotificationChannel::Slack {
    webhook_url: critical_webhook,
    min_level: AlertLevel::Critical,  // Criticalのみ
}).await;
```

## ロードマップ

### Phase 3 (計画中)

- [ ] 機械学習ベースの検知エンジン
- [ ] IPレピュテーションDB統合
- [ ] 自動応答アクション（IP自動ブロック等）
- [ ] リアルタイムダッシュボード
- [ ] パフォーマンスメトリクスの強化

## 参考資料

- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [MITRE ATT&CK Framework](https://attack.mitre.org/)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)

## ライセンス

MIT OR Apache-2.0

## サポート

Issue報告: [GitHub Issues](https://github.com/n-takatsu/mcp-rs/issues)
