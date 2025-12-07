# VirusTotal Provider Documentation

## 概要

VirusTotalプロバイダーは、[VirusTotal](https://www.virustotal.com/)の脅威インテリジェンスAPIと統合し、複数のセキュリティベンダーのスキャン結果に基づいた包括的な脅威検出を提供します。

## 特徴

- ✅ **多角的脅威検出**: 70以上のセキュリティエンジンによる検証
- ✅ **4種類の指標対応**: IP、ドメイン、URL、ファイルハッシュ
- ✅ **検出率ベース評価**: 複数エンジンの結果を統合した信頼性の高い判定
- ✅ **リッチなメタデータ**: カテゴリー、レピュテーション、検出統計
- ✅ **レート制限管理**: API制限の自動遵守
- ✅ **深刻度レベル自動判定**: 検出率に基づく5段階評価
- ✅ **バッチ処理対応**: 複数指標の効率的なチェック
- ✅ **ヘルスチェック**: プロバイダーの健全性監視

## セットアップ

### 1. APIキーの取得

1. [VirusTotal](https://www.virustotal.com/)でアカウント作成
2. [APIキー設定ページ](https://www.virustotal.com/gui/user/[username]/apikey)でAPIキーを取得
3. 環境変数に設定:

```powershell
# PowerShell
$env:VIRUSTOTAL_API_KEY="your_api_key_here"
```

```bash
# Bash/Zsh
export VIRUSTOTAL_API_KEY="your_api_key_here"
```

### 2. プロバイダーの初期化

```rust
use mcp_rs::threat_intelligence::{VirusTotalProvider, ThreatProvider};
use mcp_rs::threat_intelligence::{ProviderConfig};
use std::collections::HashMap;

let config = ProviderConfig {
    name: "VirusTotal".to_string(),
    enabled: true,
    api_key: std::env::var("VIRUSTOTAL_API_KEY").unwrap(),
    base_url: "https://www.virustotal.com/api/v3".to_string(),
    timeout_seconds: 15,
    rate_limit_per_minute: 4,  // 無料プラン: 4リクエスト/分
    reliability_factor: 0.98,  // 信頼度調整係数
    provider_specific: HashMap::new(),
};

let provider = VirusTotalProvider::new(config)?;
```

## 使用例

### 1. ドメインのチェック

```rust
use mcp_rs::threat_intelligence::{IndicatorType, ThreatIndicator};

let indicator = ThreatIndicator {
    indicator_type: IndicatorType::Domain,
    value: "malicious.example.com".to_string(),
    pattern: None,
    tags: vec!["phishing".to_string()],
    context: Some("Suspicious domain from email".to_string()),
    first_seen: chrono::Utc::now(),
};

match provider.check_indicator(&indicator).await {
    Ok(threats) => {
        if threats.is_empty() {
            println!("✅ Domain is clean");
        } else {
            for threat in threats {
                println!("⚠️  Threat detected:");
                println!("   Type: {:?}", threat.threat_type);
                println!("   Severity: {:?}", threat.severity);
                println!("   Confidence: {:.1}%", threat.confidence_score * 100.0);

                // 検出統計の確認
                if let Some(malicious) = threat.metadata.custom_attributes.get("malicious_count") {
                    println!("   Malicious detections: {}", malicious);
                }
                if let Some(ratio) = threat.metadata.custom_attributes.get("detection_ratio") {
                    println!("   Detection ratio: {}", ratio);
                }
            }
        }
    }
    Err(e) => eprintln!("❌ Error: {}", e),
}
```

### 2. IPアドレスのチェック

```rust
let indicator = ThreatIndicator {
    indicator_type: IndicatorType::IpAddress,
    value: "192.0.2.1".to_string(),
    pattern: None,
    tags: Vec::new(),
    context: Some("Suspicious connection attempt".to_string()),
    first_seen: chrono::Utc::now(),
};

let threats = provider.check_indicator(&indicator).await?;

for threat in threats {
    println!("Threat Type: {:?}", threat.threat_type);
    println!("Severity: {:?}", threat.severity);
    println!("Confidence: {:.1}%", threat.confidence_score * 100.0);
}
```

### 3. URLのチェック

```rust
let indicator = ThreatIndicator {
    indicator_type: IndicatorType::Url,
    value: "https://malicious.example.com/malware.exe".to_string(),
    pattern: None,
    tags: vec!["malware".to_string()],
    context: Some("URL from suspicious email".to_string()),
    first_seen: chrono::Utc::now(),
};

let threats = provider.check_indicator(&indicator).await?;

if !threats.is_empty() {
    println!("⚠️  Malicious URL detected!");
    for threat in threats {
        println!("   Severity: {:?}", threat.severity);
    }
}
```

### 4. ファイルハッシュのチェック

```rust
let indicator = ThreatIndicator {
    indicator_type: IndicatorType::FileHash,
    value: "44d88612fea8a8f36de82e1278abb02f".to_string(), // EICAR test file MD5
    pattern: None,
    tags: vec!["md5".to_string()],
    context: Some("Uploaded file hash".to_string()),
    first_seen: chrono::Utc::now(),
};

let threats = provider.check_indicator(&indicator).await?;

for threat in threats {
    println!("File Hash Threat:");
    println!("   Type: {:?}", threat.threat_type);
    println!("   Severity: {:?}", threat.severity);
}
```

### 5. バッチチェック

```rust
let domains = ["google.com", "malicious.example.com", "microsoft.com"];

let indicators: Vec<ThreatIndicator> = domains
    .iter()
    .map(|domain| ThreatIndicator {
        indicator_type: IndicatorType::Domain,
        value: domain.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    })
    .collect();

let all_threats = provider.batch_check_indicators(&indicators).await?;

println!("Total threats found: {}", all_threats.len());
```

### 6. ヘルスチェック

```rust
let health = provider.health_check().await?;

println!("Provider: {}", health.provider_name);
println!("Status: {:?}", health.status);
println!("Response Time: {}ms", health.response_time_ms);

if let Some(error) = health.error_message {
    println!("Error: {}", error);
}
```

### 7. レート制限の確認

```rust
let rate_limit = provider.get_rate_limit_status().await?;

println!("Rate Limit Status:");
println!("   Limit per minute: {}", rate_limit.limit_per_minute);
println!("   Remaining requests: {}", rate_limit.remaining_requests);
println!("   Reset at: {}", rate_limit.reset_at);
println!("   Is limited: {}", rate_limit.is_limited);
```

## API仕様

### エンドポイント

VirusTotalプロバイダーは以下のAPIエンドポイントを使用します:

| 指標タイプ | エンドポイント | 説明 |
|----------|-------------|------|
| IPアドレス | `GET /ip-addresses/{ip}` | IP評価とレピュテーション |
| ドメイン | `GET /domains/{domain}` | ドメインの脅威分析 |
| URL | `GET /urls/{url_id}` | URL検査結果 |
| ファイルハッシュ | `GET /files/{hash}` | ファイルスキャン結果 |

### レート制限

| プラン | リクエスト数/分 | リクエスト数/日 | 備考 |
|-------|--------------|--------------|------|
| 無料 | 4 | 500 | 登録ユーザー |
| プレミアム | 1,000 | 制限なし | 有料プラン |

### レスポンス構造

VirusTotal APIは以下の構造でデータを返します:

```json
{
  "data": {
    "type": "domain",
    "id": "example.com",
    "attributes": {
      "last_analysis_stats": {
        "harmless": 75,
        "malicious": 5,
        "suspicious": 0,
        "undetected": 10
      },
      "reputation": 0,
      "categories": {
        "BitDefender": "malware"
      }
    }
  }
}
```

### 深刻度マッピング

検出率に基づいて深刻度を自動判定します:

| 検出率 | 深刻度 | 説明 |
|-------|-------|------|
| ≥ 70% | Critical | 大多数のエンジンが脅威として検出 |
| 50-69% | High | 過半数のエンジンが脅威として検出 |
| 30-49% | Medium | 相当数のエンジンが脅威として検出 |
| 10-29% | Low | 一部のエンジンが脅威として検出 |
| < 10% | Info | 極めて少数のエンジンのみ検出 |

**検出率の計算:**
```
detection_ratio = malicious_count / (malicious + suspicious + undetected + harmless)
```

### 脅威タイプマッピング

VirusTotalのカテゴリーを脅威タイプに分類:

| カテゴリー | 脅威タイプ |
|---------|----------|
| malware, trojan, virus | Malware |
| phishing | Phishing |
| spam | Spam |
| c2, command and control | C2 |
| その他 | Unknown |

## 設定

### プロバイダー固有設定

`provider_specific`フィールドで追加設定が可能:

```rust
use std::collections::HashMap;

let mut provider_specific = HashMap::new();
provider_specific.insert(
    "max_age_days".to_string(),
    "7".to_string()
);

let config = ProviderConfig {
    // ... 基本設定 ...
    provider_specific,
};
```

利用可能な設定:

| キー | デフォルト値 | 説明 |
|-----|------------|------|
| `max_age_days` | 7 | 脅威情報の有効期限（日数） |
| `min_detection_ratio` | 0.1 | 脅威として扱う最小検出率 |

## テスト

### 単体テスト

```powershell
# 単体テストのみ実行
cargo test virustotal_provider_tests
```

### 統合テスト

統合テストは実際のAPIキーが必要です:

```powershell
# APIキーを設定
$env:VIRUSTOTAL_API_KEY="your_api_key_here"

# 統合テストを実行（#[ignore]を含む）
cargo test virustotal_provider_tests -- --ignored
```

### テストスクリプト

統合テストスクリプトを使用:

```powershell
# APIキーを設定
$env:VIRUSTOTAL_API_KEY="your_api_key_here"

# 包括的なテストを実行
.\scripts\test-virustotal.ps1
```

このスクリプトは以下を実行します:
1. 単体テスト
2. 統合テスト（APIキーが設定されている場合）
3. デモアプリケーション

### デモアプリケーション

```powershell
# APIキーを設定
$env:VIRUSTOTAL_API_KEY="your_api_key_here"

# デモを実行
cargo run --example virustotal_demo
```

デモには以下が含まれます:
- ヘルスチェック
- 悪意のあるドメインのチェック
- 正規ドメインのチェック
- IPアドレスのチェック
- URLのチェック
- ファイルハッシュのチェック（EICAR test file）
- レート制限ステータスの確認

## エラーハンドリング

### 一般的なエラー

```rust
use mcp_rs::threat_intelligence::ThreatError;

match provider.check_indicator(&indicator).await {
    Ok(threats) => {
        // 成功時の処理
    }
    Err(ThreatError::ApiError(msg)) => {
        eprintln!("API Error: {}", msg);
    }
    Err(ThreatError::RateLimitExceeded) => {
        eprintln!("Rate limit exceeded. Please wait.");
    }
    Err(ThreatError::NetworkError(e)) => {
        eprintln!("Network error: {}", e);
    }
    Err(ThreatError::ConfigurationError(msg)) => {
        eprintln!("Configuration error: {}", msg);
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

### APIエラーコード

| HTTPステータス | 説明 | 対処方法 |
|--------------|------|---------|
| 400 | Bad Request | リクエストパラメータを確認 |
| 401 | Unauthorized | APIキーを確認 |
| 403 | Forbidden | APIキーの権限を確認 |
| 429 | Too Many Requests | レート制限待機 |
| 500 | Internal Server Error | 後で再試行 |

## トラブルシューティング

### APIキーエラー

**問題**: `Configuration error: VirusTotal API key is required`

**解決策**:
```powershell
# APIキーを設定
$env:VIRUSTOTAL_API_KEY="your_api_key_here"

# 設定を確認
echo $env:VIRUSTOTAL_API_KEY
```

### レート制限エラー

**問題**: `Rate limit exceeded`

**解決策**:
- 無料プランでは1分あたり4リクエストまで
- `rate_limit_per_minute`設定を調整
- レート制限ステータスを確認:

```rust
let status = provider.get_rate_limit_status().await?;
println!("Remaining: {}", status.remaining_requests);
```

### タイムアウトエラー

**問題**: リクエストがタイムアウト

**解決策**:
```rust
let config = ProviderConfig {
    // ...
    timeout_seconds: 30, // タイムアウトを延長
    // ...
};
```

### URL検査エラー

**問題**: URLチェックが失敗する

**注意**: URLはBase64エンコードが必要
- プロバイダーが自動的にエンコード処理を実行
- 完全なURL（スキーム含む）を指定: `https://example.com/path`

### 検出結果が少ない

**問題**: 既知の脅威が検出されない

**確認事項**:
1. 検出率閾値の確認:
   ```rust
   // 検出率10%以上を脅威とする
   provider_specific.insert("min_detection_ratio".to_string(), "0.1".to_string());
   ```

2. データの鮮度:
   - VirusTotalのデータは定期的に更新されます
   - 最新の脅威は数時間後に反映される場合があります

3. 指標タイプの確認:
   - ファイルハッシュ: MD5, SHA1, SHA256をサポート
   - IP: IPv4とIPv6の両方をサポート

## ベストプラクティス

### 1. レート制限の遵守

```rust
// バッチ処理時はレート制限を考慮
let mut results = Vec::new();
for chunk in indicators.chunks(4) {  // 4リクエスト/分
    for indicator in chunk {
        results.push(provider.check_indicator(indicator).await?);
    }
    tokio::time::sleep(Duration::from_secs(60)).await;
}
```

### 2. エラーハンドリング

```rust
// リトライロジックの実装
let mut retries = 3;
loop {
    match provider.check_indicator(&indicator).await {
        Ok(threats) => break threats,
        Err(ThreatError::RateLimitExceeded) if retries > 0 => {
            retries -= 1;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
        Err(e) => return Err(e),
    }
}
```

### 3. キャッシング

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// 結果をキャッシュして重複チェックを避ける
let cache: Arc<Mutex<HashMap<String, Vec<ThreatIntelligence>>>> =
    Arc::new(Mutex::new(HashMap::new()));

let key = indicator.value.clone();
let mut cache_lock = cache.lock().await;

if let Some(cached) = cache_lock.get(&key) {
    return Ok(cached.clone());
}

let threats = provider.check_indicator(&indicator).await?;
cache_lock.insert(key, threats.clone());
```

### 4. 複数プロバイダーの組み合わせ

```rust
// VirusTotalと他のプロバイダーを組み合わせた判定
let vt_threats = virustotal_provider.check_indicator(&indicator).await?;
let abuse_threats = abuseipdb_provider.check_indicator(&indicator).await?;

let combined_confidence = (
    vt_threats.iter().map(|t| t.confidence_score).sum::<f64>() +
    abuse_threats.iter().map(|t| t.confidence_score).sum::<f64>()
) / 2.0;

println!("Combined confidence: {:.1}%", combined_confidence * 100.0);
```

## API統合テスト

### 準備

1. VirusTotal APIキーを取得
2. 環境変数に設定
3. 統合テストを実行

### テスト手順

```powershell
# 1. APIキーを設定
$env:VIRUSTOTAL_API_KEY="your_api_key_here"

# 2. 統合テストスクリプトを実行
.\scripts\test-virustotal.ps1
```

スクリプトの実行内容:
- ✅ 単体テスト実行
- ✅ 統合テスト実行（実APIキー使用）
- ✅ デモアプリケーション実行

### 期待される結果

**単体テスト** (8テスト):
- プロバイダー作成
- APIキー検証
- ファクトリー作成
- 設定検証
- 指標作成（Domain, IP, URL, FileHash）

**統合テスト** (8テスト、#[ignore]):
- ヘルスチェック
- 悪意のあるドメインチェック（027.ru）
- 正規ドメインチェック（google.com）
- IPアドレスチェック（1.1.1.1）
- URLチェック
- ファイルハッシュチェック（EICAR test file）
- レート制限ステータス
- バッチチェック

**デモアプリケーション**:
7つのデモシナリオを順次実行し、各APIエンドポイントの動作を確認

## パフォーマンス

### レスポンス時間

| 指標タイプ | 平均レスポンス時間 |
|----------|-----------------|
| ドメイン | 500-1000ms |
| IPアドレス | 400-800ms |
| URL | 600-1200ms |
| ファイルハッシュ | 300-700ms |

### 最適化のヒント

1. **バッチ処理**: 複数指標を効率的に処理
2. **キャッシング**: 重複チェックを避ける
3. **並列処理**: レート制限内で並列実行
4. **タイムアウト設定**: 適切なタイムアウト値を設定

## 参考資料

- [VirusTotal API Documentation](https://developers.virustotal.com/reference/overview)
- [VirusTotal API v3](https://developers.virustotal.com/v3.0/reference)
- [プロバイダー実装](../src/threat_intelligence/providers.rs)
- [統合テスト](../tests/threat_intelligence/virustotal_provider_tests.rs)
- [デモアプリケーション](../examples/virustotal_demo.rs)
- [テストスクリプト](../scripts/test-virustotal.ps1)

## サポート

問題が発生した場合:

1. [Issue](https://github.com/n-takatsu/mcp-rs/issues)を作成
2. ログレベルを`debug`に設定して詳細情報を取得:
   ```rust
   env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
   ```
3. VirusTotal APIの[ステータスページ](https://status.virustotal.com/)を確認
