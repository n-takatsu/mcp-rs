# 脅威インテリジェンス統合ガイド

**Issue #211: 動的ポリシー更新システムの実装**  
**Version:** v0.2.0-beta  
**Last Updated:** 2026-01-27

## 概要

mcp-rsの脅威インテリジェンス統合システムは、外部の脅威情報プロバイダーと連携して、リアルタイムでセキュリティポリシーを自動更新する機能を提供します。

### サポートされるプロバイダー

| プロバイダー | 用途 | 更新頻度 |
|------------|------|---------|
| **AbuseIPDB** | IP評価・ブロックリスト | 60分 |
| **CVE Database** | 脆弱性情報・攻撃シグネチャ | 120分 |
| **MITRE ATT&CK** | 攻撃パターン検知 | 週次 |

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────┐
│              External Threat Providers                  │
├──────────────┬──────────────┬──────────────────────────┤
│  AbuseIPDB   │  CVE Database │  MITRE ATT&CK Framework │
└──────┬───────┴──────┬───────┴──────────┬───────────────┘
       │              │                  │
       └──────────────┼──────────────────┘
                      │
          ┌───────────▼────────────┐
          │  ThreatProviders API   │
          │  (HTTP Clients)        │
          └───────────┬────────────┘
                      │
          ┌───────────▼────────────────────┐
          │ ThreatIntelligenceManager      │
          │ - データ収集・キャッシング        │
          │ - 自動更新スケジューリング        │
          │ - 脅威スコアリング               │
          └───────────┬────────────────────┘
                      │
          ┌───────────▼──────────────────┐
          │  AutoPolicyGenerator         │
          │  - IP/ドメインブロックルール    │
          │  - 攻撃パターン検知ルール      │
          │  - レート制限ルール            │
          └───────────┬──────────────────┘
                      │
          ┌───────────▼──────────────────┐
          │  DynamicPolicyUpdater        │
          │  - ポリシー適用               │
          │  - バージョン管理             │
          │  - ロールバック               │
          └──────────────────────────────┘
```

## セットアップ

### 1. 設定ファイルの準備

`configs/threat-intelligence.toml`を編集：

```toml
[abuseipdb]
enabled = true
api_key_env = "ABUSEIPDB_API_KEY"  # 環境変数名
fetch_interval_minutes = 60
confidence_threshold = 75  # 0-100
max_age_days = 90

[cve_database]
enabled = true
api_key_env = "CVE_API_KEY"  # オプション (NVD API)
fetch_interval_minutes = 120
severity_threshold = "MEDIUM"  # LOW, MEDIUM, HIGH, CRITICAL
cvss_min_score = 5.0

[mitre_attack]
enabled = true
framework_version = "v13"
fetch_interval_hours = 168  # 週次更新
auto_update = true
confidence_threshold = 0.8

[auto_policy_generator]
enabled = true
application_mode = "automatic"  # automatic | manual_review
ip_blocklist_threshold = 80
pattern_confidence_min = 0.75
auto_apply_high_confidence = true
```

### 2. 環境変数の設定

```bash
# AbuseIPDB APIキー (必須)
export ABUSEIPDB_API_KEY="your_api_key_here"

# NVD CVE API キー (オプション、レート制限緩和)
export CVE_API_KEY="your_nvd_api_key"
```

**Windowsの場合:**
```powershell
$env:ABUSEIPDB_API_KEY="your_api_key_here"
$env:CVE_API_KEY="your_nvd_api_key"
```

### 3. Cargo依存関係

`Cargo.toml`に以下が含まれていることを確認：

```toml
[dependencies]
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
chrono = "0.4"
```

## 使用例

### 基本的な使い方

```rust
use mcp_rs::policy::{
    ThreatIntelligenceManager,
    AutoPolicyGenerator,
    PolicyApplicationMode,
    DynamicPolicyUpdater,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. ポリシー更新システムの初期化
    let config = mcp_rs::policy_config::PolicyConfig::default();
    let policy_updater = Arc::new(DynamicPolicyUpdater::new(config));
    
    // 2. 脅威インテリジェンスマネージャーの作成
    let manager = ThreatIntelligenceManager::new(
        policy_updater.clone(),
        Some(0.7)  // 脅威スコア閾値
    )
    .with_abuseipdb(std::env::var("ABUSEIPDB_API_KEY")?)
    .with_cve_db(std::env::var("CVE_API_KEY").ok())
    .with_mitre_attack("v13".to_string());
    
    // 3. 自動更新を有効化
    manager.enable_auto_update().await;
    
    // 4. 自動ポリシー生成器の作成
    let generator = AutoPolicyGenerator::new(
        policy_updater.clone(),
        75,  // IP blocklist threshold
        0.75,  // Pattern confidence minimum
        PolicyApplicationMode::Automatic,
    );
    
    // 5. 脅威情報の取得とポリシー生成
    let ips = vec!["192.0.2.1".to_string()];
    let reports = manager.fetch_bulk_from_abuseipdb(&ips, 30).await?;
    
    let rules = generator.generate_ip_blocklist_from_abuseipdb(&reports).await?;
    
    // 6. ルールの適用
    let applied = generator.apply_all_rules(&rules).await?;
    
    println!("✅ Applied {} security rules", applied);
    
    Ok(())
}
```

### CVE脆弱性ベースの検知ルール生成

```rust
use mcp_rs::policy::AutoPolicyGenerator;

// CVE-2024-21413 (Microsoft Outlook RCE) の検知ルール生成
let cve_data = manager.fetch_from_cve_db("CVE-2024-21413").await?;

let detection_rules = generator.generate_cve_detection_rules(&[cve_data]).await?;

println!("Generated {} CVE-based detection rules", detection_rules.len());

for rule in &detection_rules {
    println!("Rule: {} - Severity: {:?}", rule.name, rule.severity);
}
```

### MITRE ATT&CK パターン検知

```rust
use mcp_rs::policy::MitreAttackClient;

let mitre_client = MitreAttackClient::new("v13".to_string());

// T1059: Command and Scripting Interpreter
let technique = mitre_client.fetch_technique("T1059").await?;

println!("Technique: {}", technique.name);
println!("Tactics: {:?}", technique.tactics);

let attack_rules = generator.generate_attack_pattern_rules(&[technique]).await?;

println!("Generated {} MITRE-based rules", attack_rules.len());
```

## 自動ポリシー適用モード

### Automaticモード

高信頼度の脅威情報に基づいて**自動的にポリシーを適用**します。

```rust
let generator = AutoPolicyGenerator::new(
    policy_updater.clone(),
    80,  // 高い閾値 (80%以上)
    0.85,  // 高い信頼度
    PolicyApplicationMode::Automatic,
);
```

**メリット:**
- 即座に脅威をブロック
- 人手不要で24/7保護

**リスク:**
- False Positiveによる誤ブロック
- 正規トラフィックの遮断可能性

### Manual Reviewモード

ルールを生成するが、**管理者の承認後に適用**します。

```rust
let generator = AutoPolicyGenerator::new(
    policy_updater.clone(),
    60,  // より低い閾値で検出
    0.70,
    PolicyApplicationMode::ManualReview,
);

let rules = generator.generate_ip_blocklist_from_abuseipdb(&reports).await?;

// ルールは生成されるが auto_apply = false
for rule in &rules {
    if rule.auto_apply {
        println!("⚠️  Rule will be applied automatically");
    } else {
        println!("📋 Rule pending manual review: {}", rule.name);
    }
}

// 手動承認後に適用
let approved_rules = review_and_approve(&rules).await?;
generator.apply_all_rules(&approved_rules).await?;
```

## モニタリング

### 脅威インテリジェンスメトリクス

```rust
// 自動更新状態の確認
let is_enabled = manager.is_auto_update_enabled().await;
println!("Auto-update enabled: {}", is_enabled);

// 期限切れ脅威のクリーンアップ
let cleaned = manager.cleanup_expired_threats().await;
println!("Cleaned up {} expired threats", cleaned);
```

### ログ出力

脅威インテリジェンスシステムは以下のイベントをログに記録します：

- 脅威情報の取得成功/失敗
- ポリシールールの生成
- ルールの適用/却下
- API レート制限エラー
- 設定の変更

```
[INFO] ThreatIntelligence: Fetched 15 AbuseIPDB reports
[INFO] AutoPolicyGenerator: Generated 8 IP block rules
[WARN] ThreatIntelligence: Rate limit reached for CVE API, retrying in 60s
[INFO] AutoPolicyGenerator: Applied 8 rules (Automatic mode)
[INFO] ThreatIntelligence: Cleaned up 23 expired threats
```

## パフォーマンス最適化

### キャッシング

脅威情報は自動的にキャッシュされ、不要なAPI呼び出しを削減します。

```rust
// キャッシュTTL設定
let manager = ThreatIntelligenceManager::new(policy_updater, Some(0.7))
    .with_cache_ttl(Duration::from_secs(3600));  // 1時間
```

### バッチ処理

複数のIPを一度に確認してAPI呼び出しを最小化：

```rust
let ips = vec![
    "192.0.2.1".to_string(),
    "192.0.2.2".to_string(),
    "192.0.2.3".to_string(),
    // ... 最大100件
];

let reports = manager.fetch_bulk_from_abuseipdb(&ips, 30).await?;
```

### レート制限の考慮

各プロバイダーのレート制限：

| プロバイダー | 無料プラン | 有料プラン |
|------------|-----------|-----------|
| AbuseIPDB | 1,000 req/日 | 100,000 req/日 |
| NVD CVE | 5 req/30秒 | 50 req/30秒 (API Key) |
| MITRE ATT&CK | 制限なし | - |

## トラブルシューティング

### API接続エラー

```
Error: Failed to fetch from AbuseIPDB: connection timeout
```

**解決策:**
1. インターネット接続を確認
2. ファイアウォール設定を確認
3. プロキシ設定が必要な場合は環境変数を設定:
   ```bash
   export HTTPS_PROXY=http://proxy.example.com:8080
   ```

### 認証エラー

```
Error: AbuseIPDB API authentication failed (401)
```

**解決策:**
1. APIキーが正しく設定されているか確認
2. APIキーの有効期限を確認
3. プロバイダーのダッシュボードで使用量を確認

### False Positiveの削減

IPが誤ってブロックされる場合：

1. **閾値を引き上げる:**
   ```toml
   [abuseipdb]
   confidence_threshold = 90  # より厳格に
   ```

2. **ホワイトリストを設定:**
   ```toml
   [security.ip_whitelist]
   trusted_ips = ["203.0.113.10", "198.51.100.0/24"]
   ```

3. **Manual Reviewモードに変更:**
   ```toml
   [auto_policy_generator]
   application_mode = "manual_review"
   ```

## セキュリティベストプラクティス

### 1. APIキーの保護

❌ **悪い例 (ハードコード):**
```rust
let api_key = "abc123...";  // NG!
```

✅ **良い例 (環境変数):**
```rust
let api_key = std::env::var("ABUSEIPDB_API_KEY")?;
```

### 2. ポリシーのバックアップ

自動ポリシー適用前に現在のポリシーをバックアップ：

```rust
use mcp_rs::policy::DynamicPolicyUpdater;

// ポリシー更新前
let current_policy = policy_updater.get_active_policy().await;
policy_updater.create_backup("before_threat_update").await?;

// ポリシー適用
generator.apply_all_rules(&rules).await?;

// 問題がある場合はロールバック
if issues_detected {
    policy_updater.rollback_to_backup("before_threat_update").await?;
}
```

### 3. 段階的なロールアウト

新しいルールセットは段階的に適用：

```rust
// Phase 1: ログのみ (監視)
let test_rules = generator.generate_with_log_only_mode(&reports).await?;
monitor_logs_for_24_hours(&test_rules).await?;

// Phase 2: 一部のトラフィックに適用
let partial_rules = generator.generate_with_sampling(0.1, &reports).await?;
monitor_for_48_hours(&partial_rules).await?;

// Phase 3: 全体適用
let full_rules = generator.generate_ip_blocklist_from_abuseipdb(&reports).await?;
generator.apply_all_rules(&full_rules).await?;
```

## リファレンス

### 関連ドキュメント

- [動的ポリシー更新ガイド](./policy-hot-reload-production-guide.md)
- [セキュリティポリシー設定](./security-policy-reference.md)
- [AbuseIPDB Provider Guide](./abuseipdb-provider-guide.md)
- [CVE Provider Guide](./cve-provider-guide.md)
- [MITRE ATT&CK Provider Guide](./mitre-attack-provider-guide.md)

### APIリファレンス

- `ThreatIntelligenceManager` - [src/policy/threat_intelligence.rs](../src/policy/threat_intelligence.rs)
- `AutoPolicyGenerator` - [src/policy/auto_policy_generator.rs](../src/policy/auto_policy_generator.rs)
- `AbuseIpDbClient` - [src/policy/threat_providers.rs](../src/policy/threat_providers.rs#L1-L150)
- `CveDbClient` - [src/policy/threat_providers.rs](../src/policy/threat_providers.rs#L152-L300)
- `MitreAttackClient` - [src/policy/threat_providers.rs](../src/policy/threat_providers.rs#L302-L450)

### 外部リソース

- [AbuseIPDB API Documentation](https://docs.abuseipdb.com/)
- [NVD CVE API](https://nvd.nist.gov/developers)
- [MITRE ATT&CK](https://attack.mitre.org/)

## サポート

問題が発生した場合は、以下の情報を含めてIssueを作成してください：

1. mcp-rsバージョン (`cargo --version`)
2. 設定ファイル (APIキーは除く)
3. エラーメッセージ全文
4. 関連するログ出力

**Issue Template:** `.github/ISSUE_TEMPLATE/bug_report.yml`
