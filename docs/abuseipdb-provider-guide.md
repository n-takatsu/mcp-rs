# AbuseIPDB Provider Documentation

## 概要

AbuseIPDBプロバイダーは、[AbuseIPDB](https://www.abuseipdb.com/)の脅威インテリジェンスAPIと統合し、IPアドレスの評価と脅威検出を行います。

## 特徴

- ✅ **IPアドレス評価**: IPv4/IPv6アドレスの脅威スコアリング
- ✅ **レート制限管理**: API制限の自動遵守
- ✅ **詳細な脅威情報**: 報告回数、地理情報、ISP情報を含む
- ✅ **深刻度レベル自動判定**: スコアに基づく5段階評価
- ✅ **脅威タイプ分類**: マルウェア、フィッシング、C&C等の自動分類
- ✅ **バッチ処理対応**: 複数IPの効率的なチェック
- ✅ **ヘルスチェック**: プロバイダーの健全性監視

## セットアップ

### 1. APIキーの取得

1. [AbuseIPDB](https://www.abuseipdb.com/)でアカウント作成
2. [API設定ページ](https://www.abuseipdb.com/account/api)でAPIキーを生成
3. 環境変数に設定:

```powershell
# PowerShell
$env:ABUSEIPDB_API_KEY="your_api_key_here"
```

```bash
# Bash/Zsh
export ABUSEIPDB_API_KEY="your_api_key_here"
```

### 2. プロバイダーの初期化

```rust
use mcp_rs::threat_intelligence::providers::{AbuseIPDBProvider, ThreatProvider};
use mcp_rs::threat_intelligence::types::ProviderConfig;
use std::collections::HashMap;

let config = ProviderConfig {
    name: "AbuseIPDB".to_string(),
    enabled: true,
    api_key: std::env::var("ABUSEIPDB_API_KEY").unwrap(),
    base_url: "https://api.abuseipdb.com".to_string(),
    timeout_seconds: 10,
    rate_limit_per_minute: 60, // 無料プラン: 60リクエスト/分
    reliability_factor: 0.95,   // 信頼度調整係数
    provider_specific: HashMap::new(),
};

let provider = AbuseIPDBProvider::new(config)?;
```

## 使用例

### 単一IPアドレスのチェック

```rust
use mcp_rs::threat_intelligence::types::{IndicatorType, ThreatIndicator};

let indicator = ThreatIndicator {
    indicator_type: IndicatorType::IpAddress,
    value: "118.25.6.39".to_string(),
    pattern: None,
    tags: Vec::new(),
    context: Some("Suspicious login attempt".to_string()),
    first_seen: chrono::Utc::now(),
};

match provider.check_indicator(&indicator).await {
    Ok(threats) => {
        if threats.is_empty() {
            println!("✅ IP is clean");
        } else {
            for threat in threats {
                println!("⚠️  Threat detected:");
                println!("   Type: {:?}", threat.threat_type);
                println!("   Severity: {:?}", threat.severity);
                println!("   Confidence: {:.1}%", threat.confidence_score * 100.0);
            }
        }
    }
    Err(e) => eprintln!("❌ Error: {}", e),
}
```

### バッチチェック

```rust
let ips = ["8.8.8.8", "1.1.1.1", "198.51.100.42"];

let indicators: Vec<ThreatIndicator> = ips
    .iter()
    .map(|ip| ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: ip.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    })
    .collect();

let threats = provider.batch_check_indicators(&indicators).await?;
println!("Total threats detected: {}", threats.len());
```

### ヘルスチェック

```rust
let health = provider.health_check().await?;
println!("Status: {:?}", health.status);
println!("Response time: {}ms", health.response_time_ms);
```

### レート制限ステータス確認

```rust
let status = provider.get_rate_limit_status().await?;
println!("Remaining requests: {}", status.remaining_requests);
println!("Reset at: {}", status.reset_at);
```

## APIレスポンス詳細

### 脅威情報フィールド

AbuseIPDBから返される情報:

- **abuse_confidence_score**: 0-100の脅威スコア（高いほど危険）
- **total_reports**: 報告された回数
- **country_code/country_name**: IPの所在国
- **is_public**: パブリックIPかどうか
- **is_whitelisted**: ホワイトリスト登録されているか
- **isp/domain**: ISP情報とドメイン
- **usage_type**: 使用タイプ（Datacenter、Residential等）
- **reports**: 個別の報告詳細（カテゴリー含む）

### 深刻度レベルマッピング

| Abuse Score | 深刻度レベル |
|-------------|-------------|
| 80-100      | Critical    |
| 60-79       | High        |
| 40-59       | Medium      |
| 20-39       | Low         |
| 0-19        | Info        |

### カテゴリーマッピング

AbuseIPDBのカテゴリー番号を脅威タイプに変換:

| カテゴリー | 脅威タイプ |
|-----------|----------|
| 3-11      | Malware  |
| 12-13     | Phishing |
| 14        | Spam     |
| 15-17     | CommandAndControl |
| 18-20     | Botnet   |
| 21        | Exploit  |
| その他     | MaliciousIp |

## レート制限

### 無料プラン

- **1日あたり**: 1,000リクエスト
- **1分あたり**: 60リクエスト
- **最大IPアドレス数/リクエスト**: 1

### 有料プラン

- **Basic ($19/月)**: 3,000リクエスト/日
- **Premium ($49/月)**: 10,000リクエスト/日
- **Enterprise**: カスタム制限

プロバイダーは自動的にレート制限を管理し、超過時には `ThreatError::RateLimitExceeded` を返します。

## エラーハンドリング

```rust
use mcp_rs::threat_intelligence::types::ThreatError;

match provider.check_indicator(&indicator).await {
    Ok(threats) => {
        // 成功処理
    }
    Err(ThreatError::RateLimitExceeded(provider)) => {
        eprintln!("Rate limit exceeded for {}", provider);
        // リトライロジック実装
    }
    Err(ThreatError::NetworkError(msg)) => {
        eprintln!("Network error: {}", msg);
        // ネットワークエラー処理
    }
    Err(ThreatError::ConfigurationError(msg)) => {
        eprintln!("Configuration error: {}", msg);
        // 設定エラー処理
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

## デモプログラム

完全なデモプログラムを実行:

```powershell
# 環境変数設定
$env:ABUSEIPDB_API_KEY="your_api_key_here"

# デモ実行
cargo run --example abuseipdb_demo
```

デモには以下が含まれます:

1. ヘルスチェック
2. 既知の悪意あるIPチェック
3. 安全なIPチェック
4. バッチIPチェック
5. レート制限ステータス確認
6. 無効なIP形式のエラーハンドリング
7. IPv6アドレスのチェック

## テスト

### 単体テスト実行

```powershell
cargo test --test test_abuseipdb_provider
```

### 統合テスト実行（実際のAPI使用）

```powershell
# APIキー設定
$env:ABUSEIPDB_API_KEY="your_api_key_here"

# 統合テスト実行（ignoreフラグ付きテスト含む）
cargo test --test test_abuseipdb_provider -- --ignored
```

## ベストプラクティス

### 1. APIキーの管理

- **環境変数を使用**: ハードコーディングしない
- **秘密管理サービス**: 本番環境ではVault等を使用
- **キーローテーション**: 定期的なキーの更新

```rust
use std::env;

let api_key = env::var("ABUSEIPDB_API_KEY")
    .expect("ABUSEIPDB_API_KEY must be set");
```

### 2. レート制限の遵守

```rust
// プロバイダーは自動的にレート制限をチェック
match provider.check_indicator(&indicator).await {
    Err(ThreatError::RateLimitExceeded(_)) => {
        // 1分待機してリトライ
        tokio::time::sleep(Duration::from_secs(60)).await;
        // リトライロジック
    }
    Ok(threats) => { /* 処理 */ }
    Err(e) => { /* エラー処理 */ }
}
```

### 3. キャッシング戦略

```rust
use lru::LruCache;
use std::sync::Arc;
use tokio::sync::RwLock;

struct CachedAbuseIPDB {
    provider: AbuseIPDBProvider,
    cache: Arc<RwLock<LruCache<String, Vec<ThreatIntelligence>>>>,
}

impl CachedAbuseIPDB {
    async fn check_with_cache(&self, ip: &str) -> Result<Vec<ThreatIntelligence>, ThreatError> {
        // キャッシュチェック
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.peek(ip) {
                return Ok(cached.clone());
            }
        }
        
        // API呼び出し
        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: ip.to_string(),
            // ... 他のフィールド
        };
        
        let result = self.provider.check_indicator(&indicator).await?;
        
        // キャッシュ更新
        {
            let mut cache = self.cache.write().await;
            cache.put(ip.to_string(), result.clone());
        }
        
        Ok(result)
    }
}
```

### 4. バッチ処理の最適化

```rust
// 大量のIPを小さなバッチに分割
let batch_size = 100;
for chunk in ips.chunks(batch_size) {
    let indicators: Vec<_> = chunk.iter()
        .map(|ip| create_indicator(ip))
        .collect();
    
    let threats = provider.batch_check_indicators(&indicators).await?;
    
    // バッチ間で少し待機（レート制限対策）
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

## トラブルシューティング

### よくある問題

#### 1. 認証エラー

```
Error: ProviderError("AbuseIPDB API error: 401 - Invalid API key")
```

**解決策**:
- APIキーが正しいか確認
- 環境変数が設定されているか確認
- APIキーの有効期限を確認

#### 2. レート制限超過

```
Error: RateLimitExceeded("AbuseIPDB")
```

**解決策**:
- リクエスト頻度を下げる
- キャッシングを実装
- 有料プランにアップグレード

#### 3. タイムアウト

```
Error: NetworkError("timeout")
```

**解決策**:
- `timeout_seconds` を増やす
- ネットワーク接続を確認
- AbuseIPDBのステータスページを確認

#### 4. 無効なIP形式

```
Error: ConfigurationError("Invalid IP address format: ...")
```

**解決策**:
- IP形式が正しいか確認（IPv4/IPv6）
- 入力のバリデーションを実装

## パフォーマンス考慮事項

### レスポンス時間

- **平均**: 200-500ms（ネットワーク状況による）
- **最大**: タイムアウト設定値（デフォルト10秒）

### 最適化テクニック

1. **並列処理**: 複数の独立したリクエストを同時実行
2. **キャッシング**: 頻繁にチェックするIPをキャッシュ
3. **バッチサイズ調整**: レート制限内で最大化
4. **非同期処理**: tokioの非同期機能を活用

## セキュリティ考慮事項

1. **APIキーの保護**: 環境変数や秘密管理サービスを使用
2. **入力検証**: IPアドレス形式の厳密な検証
3. **エラーログ**: 機密情報を含まない適切なロギング
4. **タイムアウト設定**: DoS攻撃対策のための適切なタイムアウト

## 関連リソース

- [AbuseIPDB API Documentation](https://docs.abuseipdb.com/)
- [AbuseIPDB カテゴリーリスト](https://www.abuseipdb.com/categories)
- [Issue #77: 脅威インテリジェンス統合](https://github.com/n-takatsu/mcp-rs/issues/77)

## ライセンス

MIT OR Apache-2.0

## サポート

問題が発生した場合は、GitHubリポジトリでIssueを作成してください。
