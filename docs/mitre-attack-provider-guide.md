# MITRE ATT&CK プロバイダーガイド

## 概要

MITRE ATT&CKプロバイダーは、MITRE ATT&CKフレームワークとの統合を提供し、攻撃手法（Techniques）、戦術（Tactics）、グループ情報に基づいた脅威インテリジェンスを提供します。

### 主要機能

- **テクニック検索**: MITRE ATT&CK テクニックIDによる直接検索（例: T1566）
- **キーワード検索**: 攻撃手法のキーワードによる検索（例: "phishing"）
- **7日間キャッシュ**: テクニック情報を7日間キャッシュして高速応答
- **戦術マッピング**: 攻撃戦術に基づく深刻度自動判定
- **詳細メタデータ**: プラットフォーム、データソース、検出方法、緩和策

### 技術仕様

- **API**: MITRE ATT&CK STIX 2.0/2.1
- **エンドポイント**: GitHub CTI Repository
- **認証**: APIキー不要（公開データ）
- **レート制限**: 60リクエスト/分（デフォルト）
- **キャッシュTTL**: 7日間
- **信頼度**: 0.95（デフォルト）

## セットアップ

### 1. プロバイダーの初期化

```rust
use mcp_rs::threat_intelligence::*;
use std::collections::HashMap;

let config = ProviderConfig {
    name: "MITRE-ATTACK".to_string(),
    enabled: true,
    api_key: String::new(), // APIキー不要
    base_url: "https://raw.githubusercontent.com/mitre/cti/master".to_string(),
    timeout_seconds: 30,
    rate_limit_per_minute: 60,
    reliability_factor: 0.95,
    provider_specific: HashMap::new(),
};

let provider = ProviderFactory::create_provider(config)?;
```

### 2. 基本的な使用方法

#### テクニックID検索

```rust
// Phishing (T1566) テクニックを検索
let indicator = ThreatIndicator {
    indicator_type: IndicatorType::FileHash,
    value: "T1566".to_string(),
    pattern: None,
    tags: Vec::new(),
    context: None,
    first_seen: chrono::Utc::now(),
};

let threats = provider.check_indicator(&indicator).await?;

for threat in threats {
    if let Some(technique) = threat.metadata.mitre_attack_techniques.first() {
        println!("Technique: {} ({})", technique.name, technique.technique_id);
        println!("Tactics: {}", technique.tactics.join(", "));
        println!("Platforms: {}", technique.platforms.join(", "));
    }
}
```

#### サブテクニック検索

```rust
// PowerShell (T1059.001) サブテクニックを検索
let indicator = ThreatIndicator {
    indicator_type: IndicatorType::FileHash,
    value: "T1059.001".to_string(),
    pattern: None,
    tags: Vec::new(),
    context: None,
    first_seen: chrono::Utc::now(),
};

let threats = provider.check_indicator(&indicator).await?;
```

#### キーワード検索

```rust
// キーワードで検索
let keywords = vec!["phishing", "credential dumping", "lateral movement"];

for keyword in keywords {
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: keyword.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    };
    
    let threats = provider.check_indicator(&indicator).await?;
    println!("Found {} threats for '{}'", threats.len(), keyword);
}
```

### 3. バッチ検索

```rust
let technique_ids = vec!["T1566", "T1003", "T1059.001", "T1021"];
let mut indicators = Vec::new();

for id in &technique_ids {
    indicators.push(ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: id.to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    });
}

let threats = provider.batch_check_indicators(&indicators).await?;
println!("Total threats found: {}", threats.len());
```

### 4. キャッシュ管理

```rust
// キャッシュサイズを確認（MitreAttackProviderにキャスト必要）
if let Some(mitre_provider) = provider.as_any().downcast_ref::<MitreAttackProvider>() {
    let cache_size = mitre_provider.techniques_cache_size().await;
    println!("Cache size: {} techniques", cache_size);
    
    // キャッシュをクリア
    mitre_provider.clear_cache().await;
}
```

## MITRE ATT&CK テクニック情報

### テクニック構造

```rust
pub struct MitreAttackTechnique {
    /// テクニックID (例: T1566)
    pub technique_id: String,
    /// サブテクニックID (例: T1566.001)
    pub sub_technique_id: Option<String>,
    /// テクニック名
    pub name: String,
    /// 戦術 (Tactic)
    pub tactics: Vec<String>,
    /// プラットフォーム
    pub platforms: Vec<String>,
    /// データソース
    pub data_sources: Vec<String>,
    /// 説明
    pub description: Option<String>,
    /// 検出方法
    pub detection: Option<String>,
    /// 緩和策
    pub mitigation: Vec<String>,
}
```

### 戦術（Tactics）一覧

MITRE ATT&CKフレームワークの14の戦術:

1. **Reconnaissance** (偵察)
2. **Resource Development** (リソース開発)
3. **Initial Access** (初期アクセス)
4. **Execution** (実行)
5. **Persistence** (永続化)
6. **Privilege Escalation** (権限昇格)
7. **Defense Evasion** (防御回避)
8. **Credential Access** (認証情報アクセス)
9. **Discovery** (探索)
10. **Lateral Movement** (横展開)
11. **Collection** (収集)
12. **Command and Control** (C2)
13. **Exfiltration** (データ持ち出し)
14. **Impact** (影響)

### 深刻度マッピング

戦術に基づく深刻度の自動判定:

| 深刻度 | 戦術 |
|--------|------|
| **High** | Impact, Exfiltration, Lateral Movement |
| **Medium** | Privilege Escalation, Credential Access, Defense Evasion |
| **Low** | その他の戦術 |

## サポートされているキーワード

以下のキーワードが自動的にテクニックIDにマッピングされます:

| キーワード | テクニックID | テクニック名 |
|-----------|-------------|-------------|
| `phishing`, `spearphishing` | T1566 | Phishing |
| `credential dumping`, `credentials` | T1003 | OS Credential Dumping |
| `powershell` | T1059.001 | PowerShell |
| `command and control`, `c2`, `c&c` | T1071 | Application Layer Protocol |
| `remote desktop`, `rdp` | T1021.001 | Remote Desktop Protocol |
| `lateral movement` | T1021 | Remote Services |
| `privilege escalation` | T1068 | Exploitation for Privilege Escalation |
| `persistence` | T1547 | Boot or Logon Autostart Execution |
| `defense evasion` | T1562 | Impair Defenses |
| `exfiltration` | T1041 | Exfiltration Over C2 Channel |

## デモプログラム

### 実行方法

```powershell
# 全デモを実行
cargo run --example mitre_attack_demo

# 特定の機能のみテスト
cargo run --example mitre_attack_demo -- --test phishing
```

### デモ内容

1. **ヘルスチェック**: プロバイダーの健全性確認
2. **Phishing テクニック**: T1566の詳細情報取得
3. **PowerShell テクニック**: T1059.001のサブテクニック情報
4. **キーワード検索**: 複数キーワードでの検索
5. **バッチ検索**: 複数テクニックの一括検索
6. **レート制限**: レート制限ステータスの確認

## テスト

### 単体テスト

```powershell
# 全テストを実行
cargo test --test test_mitre_attack_provider

# 特定のテストを実行
cargo test --test test_mitre_attack_provider test_valid_technique_id_format
```

### 統合テスト（実API使用）

```powershell
# 統合テストを実行（APIアクセスあり）
cargo test --test test_mitre_attack_provider --ignored

# 特定の統合テストを実行
cargo test --test test_mitre_attack_provider test_real_api_phishing_technique --ignored
```

## ベストプラクティス

### 1. **キャッシュの活用**

```rust
// 頻繁にアクセスされるテクニックは自動的にキャッシュされる
// 7日間有効なので、繰り返しクエリは高速
let phishing = provider.check_indicator(&phishing_indicator).await?;
```

### 2. **バッチ処理の使用**

```rust
// 複数テクニックを一度に検索する際はバッチを使用
let indicators = vec![
    create_indicator("T1566"),
    create_indicator("T1003"),
    create_indicator("T1059.001"),
];

let threats = provider.batch_check_indicators(&indicators).await?;
```

### 3. **エラーハンドリング**

```rust
match provider.check_indicator(&indicator).await {
    Ok(threats) => {
        if threats.is_empty() {
            println!("No matching techniques found");
        } else {
            process_threats(threats);
        }
    }
    Err(ThreatError::RateLimitExceeded(_)) => {
        // レート制限時の処理
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

### 4. **メタデータの活用**

```rust
for threat in threats {
    for technique in &threat.metadata.mitre_attack_techniques {
        // 戦術に基づいた処理
        if technique.tactics.contains(&"initial-access".to_string()) {
            handle_initial_access(technique);
        }
        
        // プラットフォーム固有の処理
        if technique.platforms.contains(&"Windows".to_string()) {
            apply_windows_detection(technique);
        }
        
        // 検出方法の実装
        if let Some(ref detection) = technique.detection {
            implement_detection(detection);
        }
    }
}
```

## トラブルシューティング

### 1. テクニックが見つからない

**原因**: 無効なテクニックIDまたはキーワード

**解決策**:
- テクニックID形式を確認（T1234 または T1234.001）
- サポートされているキーワードリストを参照
- MITRE ATT&CKウェブサイトで正確なIDを確認

### 2. タイムアウトエラー

**原因**: ネットワーク遅延またはGitHubの応答遅延

**解決策**:
```rust
let mut config = create_config();
config.timeout_seconds = 60; // タイムアウトを延長
```

### 3. キャッシュが古い

**原因**: 7日以上経過したキャッシュエントリ

**解決策**:
```rust
// キャッシュを手動でクリア
mitre_provider.clear_cache().await;

// 再度検索
let threats = provider.check_indicator(&indicator).await?;
```

### 4. 空の結果が返される

**原因**: マッピングされていないキーワードまたは存在しないテクニック

**解決策**:
- 直接テクニックIDを使用
- キーワードマッピングリストを確認
- ログを確認して詳細を調査

## パフォーマンス考慮事項

### キャッシュ効率

- **初回アクセス**: 2-5秒（GitHub APIレスポンス時間）
- **キャッシュヒット**: < 10ms
- **キャッシュサイズ**: メモリ使用量は最小限（テクニック情報のみ）
- **有効期限**: 7日間（MITRE ATT&CKの更新頻度を考慮）

### レート制限

- **デフォルト**: 60リクエスト/分
- **推奨**: 実運用では30-60リクエスト/分
- **調整**: `rate_limit_per_minute`設定で変更可能

### メモリ使用量

- **テクニックあたり**: 約2-5KB
- **100テクニックキャッシュ**: 約200-500KB
- **推奨最大**: 1000テクニックまでキャッシュ（約2-5MB）

## 関連リソース

- [MITRE ATT&CK公式サイト](https://attack.mitre.org/)
- [MITRE ATT&CK Navigator](https://mitre-attack.github.io/attack-navigator/)
- [ATT&CK STIX Data](https://github.com/mitre/cti)
- [ATT&CK Matrix for Enterprise](https://attack.mitre.org/matrices/enterprise/)

## 次のステップ

1. **複数プロバイダー統合**: AbuseIPDB、CVE、MITRE ATT&CKを組み合わせた総合的な脅威評価
2. **ATT&CKグループ検索**: 脅威アクターグループ情報の統合
3. **カスタムマッピング**: 組織固有の攻撃手法マッピング
4. **自動検出ルール生成**: ATT&CKテクニックから検出ルールを自動生成
