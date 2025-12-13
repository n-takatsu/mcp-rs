# リアルタイム脅威インテリジェンスエンジン

## 概要

このドキュメントは、MCP-RSプロジェクトのリアルタイム脅威インテリジェンスエンジンの使用方法、設定、ベストプラクティスを説明します。

## アーキテクチャ

### システム構成

```
┌─────────────────────────────────────────────────────────────────┐
│                  Threat Intelligence Engine                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌────────────────────┐  ┌────────────────────┐                │
│  │ Detection Engine   │  │ Provider Manager   │                │
│  │                    │  │                    │                │
│  │ • IOC Extraction   │  │ • VirusTotal       │                │
│  │ • Pattern Matching │  │ • AbuseIPDB        │                │
│  │ • Correlation      │  │ • AlienVault OTX   │                │
│  └────────────────────┘  └────────────────────┘                │
│           │                      │                               │
│           └──────────┬───────────┘                               │
│                      ▼                                           │
│           ┌─────────────────────┐                                │
│           │ Threat Manager      │                                │
│           │                     │                                │
│           │ • Assessment        │                                │
│           │ • Caching           │                                │
│           │ • Rate Limiting     │                                │
│           └─────────────────────┘                                │
│                      │                                           │
│        ┌─────────────┴─────────────┐                            │
│        ▼                           ▼                            │
│  ┌─────────────────┐     ┌────────────────────┐                │
│  │ IOC Database    │     │ Threat Feed         │                │
│  │                 │     │                     │                │
│  │ • IP Addresses  │     │ • Real-time Updates │                │
│  │ • Domains       │     │ • Subscriptions     │                │
│  │ • File Hashes   │     │ • Event Broadcasting│                │
│  │ • Email Addr    │     │ • Filtering         │                │
│  └─────────────────┘     └────────────────────┘                │
└─────────────────────────────────────────────────────────────────┘
```

## 主要機能

### 1. IOC (Indicator of Compromise) 検出

システムは以下のIOCタイプを自動検出します：

- **IPアドレス**: IPv4とIPv6の両方をサポート
- **ドメイン/URL**: 悪意のあるウェブサイトを識別
- **ファイルハッシュ**: MD5、SHA1、SHA256をサポート
- **メールアドレス**: フィッシング送信元を特定
- **CVE識別子**: 既知の脆弱性参照

#### 使用例

```rust
use mcp_rs::threat_intelligence::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // エンジンの初期化
    let manager = Arc::new(ThreatIntelligenceManager::new(
        ThreatIntelligenceConfig::default(),
    ));
    let engine = ThreatDetectionEngine::new(manager);

    // テキストから脅威を検出
    let text = "Suspicious activity from 198.51.100.42 accessing https://malware.example.com";
    let result = engine.detect_threats(text).await.unwrap();

    // 検出結果の確認
    println!("検出された指標: {}", result.indicators.len());
    for assessment in result.assessments {
        if assessment.is_threat {
            println!("脅威検出: {} (信頼度: {:.2})",
                assessment.indicator.value,
                assessment.confidence_score
            );
        }
    }
}
```

### 2. 脅威プロバイダー統合

#### VirusTotal

ファイルハッシュ、URL、IPアドレスの評価を提供します。

**設定例:**

```rust
let vt_config = ProviderConfig {
    name: "VirusTotal".to_string(),
    api_key: Some("your-api-key-here".to_string()),
    base_url: "https://www.virustotal.com/api/v3".to_string(),
    enabled: true,
    timeout_seconds: 30,
    rate_limit_per_minute: 4, // 無料プランの制限
};

let provider = VirusTotalProvider::new(vt_config)?;
```

#### AbuseIPDB

IPアドレスの評価と報告を提供します。

**設定例:**

```rust
let abuseipdb_config = ProviderConfig {
    name: "AbuseIPDB".to_string(),
    api_key: Some("your-api-key-here".to_string()),
    base_url: "https://api.abuseipdb.com/api/v2".to_string(),
    enabled: true,
    timeout_seconds: 15,
    rate_limit_per_minute: 1000,
};

let provider = AbuseIPDBProvider::new(abuseipdb_config)?;
```

#### AlienVault OTX

包括的な脅威インテリジェンスフィードを提供します。

**設定例:**

```rust
let otx_config = ProviderConfig {
    name: "AlienVault OTX".to_string(),
    api_key: Some("your-api-key-here".to_string()),
    base_url: "https://otx.alienvault.com/api/v1".to_string(),
    enabled: true,
    timeout_seconds: 20,
    rate_limit_per_minute: 60,
};

let provider = AlienVaultProvider::new(otx_config)?;
```

### 3. レピュテーション評価

#### IPレピュテーション

```rust
let indicator = ThreatIndicator {
    indicator_type: IndicatorType::IpAddress,
    value: "203.0.113.1".to_string(),
    context: None,
};

let assessment = manager.assess_threat(indicator).await?;

match assessment.threat_level {
    SeverityLevel::Critical => println!("危険なIP"),
    SeverityLevel::High => println!("高リスクIP"),
    SeverityLevel::Medium => println!("中リスクIP"),
    SeverityLevel::Low => println!("低リスクIP"),
    SeverityLevel::Info => println!("安全なIP"),
}
```

#### ドメインレピュテーション

```rust
let indicator = ThreatIndicator {
    indicator_type: IndicatorType::Domain,
    value: "example.com".to_string(),
    context: None,
};

let assessment = manager.assess_threat(indicator).await?;
println!("信頼度スコア: {:.2}", assessment.confidence_score);
```

### 4. リアルタイム脅威フィード

#### サブスクリプション作成

```rust
let feed = ThreatFeed::new(manager, ThreatFeedConfig::default());

// フィルター設定
let filters = ThreatFeedFilters {
    threat_types: Some(vec![ThreatType::Malware, ThreatType::Phishing]),
    min_severity: Some(SeverityLevel::High),
    indicator_types: Some(vec![IndicatorType::IpAddress, IndicatorType::Url]),
    providers: None,
    min_confidence: Some(0.8),
    tags: Some(vec!["ransomware".to_string()]),
};

// サブスクリプション
let subscription_id = feed.subscribe("my_service".to_string(), filters).await?;

// イベントの受信
let mut receiver = feed.get_receiver().await?;
while let Ok(event) = receiver.recv().await {
    match event {
        ThreatFeedEvent::NewThreat(threat) => {
            println!("新しい脅威: {:?}", threat);
        },
        ThreatFeedEvent::ThreatUpdate(update) => {
            println!("脅威更新: {:?}", update);
        },
        _ => {}
    }
}
```

#### サブスクリプション解除

```rust
feed.unsubscribe(subscription_id).await?;
```

## 設定ガイド

### 基本設定

```rust
let config = ThreatIntelligenceConfig {
    enabled: true,
    auto_block: false,
    confidence_threshold: 0.7,
    max_cache_size: 10000,
    cache_ttl_seconds: 3600, // 1時間
    max_concurrent_checks: 10,
    default_timeout_seconds: 30,
};
```

### 検出設定

```rust
let detection_config = DetectionConfig {
    ip_detection_enabled: true,
    url_detection_enabled: true,
    file_hash_detection_enabled: true,
    email_detection_enabled: true,
    max_concurrent_detections: 5,
    detection_timeout_seconds: 10,
    auto_extraction_enabled: true,
};
```

### フィード設定

```rust
let feed_config = ThreatFeedConfig {
    enabled: true,
    max_subscriptions: 1000,
    default_update_interval_secs: 60,
    batch_size: 100,
    event_buffer_size: 10000,
    cleanup_interval_secs: 300,
};
```

## パフォーマンス要件

### 応答時間目標

- **IPチェック**: < 50ms
- **URLチェック**: < 100ms
- **ファイルハッシュチェック**: < 200ms
- **バッチチェック（10件）**: < 500ms

### キャッシュ効率

- **キャッシュヒット率**: > 90%
- **キャッシュTTL**: 1時間（デフォルト）
- **最大キャッシュサイズ**: 10,000エントリ

### レート制限

各プロバイダーのAPI制限を遵守：

| プロバイダー | 無料プラン | 有料プラン |
|-------------|-----------|-----------|
| VirusTotal  | 4/分      | 1000/日   |
| AbuseIPDB   | 1000/日   | 無制限    |
| AlienVault  | 60/分     | 無制限    |

## セキュリティ要件

### APIキー管理

環境変数または設定ファイルで管理：

```bash
export VIRUSTOTAL_API_KEY="your-key-here"
export ABUSEIPDB_API_KEY="your-key-here"
export ALIENVAULT_API_KEY="your-key-here"
```

Rustコードでの読み込み：

```rust
use std::env;

let vt_key = env::var("VIRUSTOTAL_API_KEY")
    .expect("VIRUSTOTAL_API_KEY not set");
```

### TLS通信

すべてのプロバイダー通信は必ずHTTPS経由で行います。

### プライバシー保護

個人識別情報（PII）は自動的に除外されます：

- 内部IPアドレス（192.168.x.x、10.x.x.x）は外部プロバイダーに送信されません
- ユーザー名やパスワードはIOCから除外されます

## ベストプラクティス

### 1. キャッシュの活用

頻繁にチェックされる指標はキャッシュを利用：

```rust
let config = ThreatIntelligenceConfig {
    cache_ttl_seconds: 3600, // 1時間キャッシュ
    max_cache_size: 50000,   // 50,000エントリ
    ..Default::default()
};
```

### 2. バッチ処理

複数の指標を一度にチェック：

```rust
let indicators = vec![
    ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: "192.0.2.1".to_string(),
        context: None,
    },
    // ... more indicators
];

let assessments = manager.batch_assess_threats(indicators).await?;
```

### 3. エラーハンドリング

```rust
match manager.assess_threat(indicator).await {
    Ok(assessment) => {
        if assessment.is_threat {
            // 脅威対応
        }
    },
    Err(ThreatError::RateLimitExceeded { .. }) => {
        // レート制限対応
        tokio::time::sleep(Duration::from_secs(60)).await;
    },
    Err(e) => {
        eprintln!("エラー: {:?}", e);
    }
}
```

### 4. フェイルセーフ

プロバイダーが利用できない場合の処理：

```rust
let config = ThreatIntelligenceConfig {
    enabled: true,
    confidence_threshold: 0.7,
    // タイムアウトを短く設定
    default_timeout_seconds: 5,
    ..Default::default()
};
```

## トラブルシューティング

### 問題: レート制限エラー

**症状**: `ThreatError::RateLimitExceeded`

**解決策**:
1. API呼び出しを減らす
2. キャッシュTTLを延長
3. 有料プランへのアップグレード

```rust
let config = ThreatIntelligenceConfig {
    cache_ttl_seconds: 7200, // 2時間に延長
    max_concurrent_checks: 5, // 並列数を減少
    ..Default::default()
};
```

### 問題: タイムアウトエラー

**症状**: 応答に時間がかかりすぎる

**解決策**:
1. タイムアウト値を調整
2. 不要なプロバイダーを無効化
3. ネットワーク接続を確認

```rust
let provider_config = ProviderConfig {
    timeout_seconds: 60, // タイムアウトを延長
    ..Default::default()
};
```

### 問題: 偽陽性が多い

**症状**: 安全なIPやURLが脅威と判定される

**解決策**:
1. 信頼度閾値を上げる
2. ホワイトリストを設定

```rust
let config = ThreatIntelligenceConfig {
    confidence_threshold: 0.9, // 閾値を上げる
    ..Default::default()
};
```

### 問題: プロバイダーに接続できない

**症状**: `ThreatError::ProviderUnavailable`

**解決策**:
1. APIキーの確認
2. ネットワーク接続の確認
3. プロバイダーのステータスページ確認

```bash
# APIキー確認
echo $VIRUSTOTAL_API_KEY

# ネットワーク接続確認
curl -I https://www.virustotal.com/api/v3/
```

## パフォーマンスモニタリング

### 統計情報の取得

```rust
let stats = manager.get_statistics().await;
println!("チェック総数: {}", stats.total_checks);
println!("脅威検出数: {}", stats.threats_detected);
println!("キャッシュヒット率: {:.2}%", stats.cache_hit_rate * 100.0);
println!("平均応答時間: {}ms", stats.avg_response_time_ms);
```

### ヘルスチェック

```rust
let health = manager.check_provider_health().await?;
for (provider, status) in health {
    println!("{}: {:?} ({}ms)",
        provider,
        status.status,
        status.response_time_ms
    );
}
```

## API リファレンス

### ThreatDetectionEngine

- `new(manager: Arc<ThreatIntelligenceManager>) -> Self`
- `detect_threats(text: &str) -> Result<DetectionResult, ThreatError>`
- `detect_specific_threats(text: &str, types: &[IndicatorType]) -> Result<DetectionResult, ThreatError>`
- `extract_indicators(text: &str) -> Result<Vec<ThreatIndicator>, ThreatError>`

### ThreatIntelligenceManager

- `new(config: ThreatIntelligenceConfig) -> Self`
- `assess_threat(indicator: ThreatIndicator) -> Result<ThreatAssessment, ThreatError>`
- `batch_assess_threats(indicators: Vec<ThreatIndicator>) -> Result<Vec<ThreatAssessment>, ThreatError>`
- `check_provider_health() -> Result<HashMap<String, ProviderHealth>, ThreatError>`

### ThreatFeed

- `new(manager: Arc<ThreatIntelligenceManager>, config: ThreatFeedConfig) -> Self`
- `subscribe(subscriber_id: String, filters: ThreatFeedFilters) -> Result<Uuid, ThreatError>`
- `unsubscribe(subscription_id: Uuid) -> Result<(), ThreatError>`
- `get_receiver() -> Result<broadcast::Receiver<ThreatFeedEvent>, ThreatError>`

## まとめ

リアルタイム脅威インテリジェンスエンジンは、以下の原則に基づいています：

1. **高速応答**: 50ms以内のIPチェック
2. **高精度**: 90%以上の脅威検出率、5%未満の偽陽性率
3. **高可用性**: 99.9%以上のAPI可用性
4. **拡張性**: 複数プロバイダーの統合サポート

これにより、リアルタイムでの脅威検知と対応が可能になり、システムのセキュリティが大幅に向上します。
