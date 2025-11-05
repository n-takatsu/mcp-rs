# Policy Hot-Reload System User Guide

## 概要

Policy Hot-Reload システムは、mcp-rs プロジェクトにおいて**リアルタイムでポリシー設定を動的に更新・適用**するためのシステムです。ファイル監視、検証、適用を一元的に管理し、本番環境での安全で効率的なポリシー管理を実現します。

## 🎯 主要機能

### 1. **ポリシー設定管理**
- **多形式対応**: TOML、YAML、JSON形式のポリシーファイルに対応
- **統一インターフェース**: 形式に関係なく統一されたAPIで操作
- **設定検証**: 読み込み時の自動検証機能

### 2. **リアルタイムファイル監視**
- **ファイル変更検知**: 作成、更新、削除イベントのリアルタイム監視
- **高速応答**: 変更検知から適用まで 15-20ms の高速処理
- **マルチファイル対応**: 複数ポリシーファイルの同時監視

### 3. **包括的ポリシー検証**
- **4段階検証レベル**: Basic, Standard, Strict, Custom
- **セキュリティ要件チェック**: 暗号化、TLS、認証設定の検証
- **論理整合性チェック**: 設定間の相互関係の確認
- **環境固有ルール**: 本番/開発環境別の検証ルール

### 4. **安全なポリシー適用**
- **検証合格後適用**: 検証を通過したポリシーのみ適用
- **ロールバック機能**: 問題発生時の自動復旧
- **イベント通知**: 適用結果のリアルタイム通知

## 🚀 クイックスタート

### 基本的な使用例

```rust
use mcp_rs::policy_application::PolicyApplicationEngine;
use mcp_rs::policy_validation::ValidationLevel;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. エンジンを作成（厳格な検証レベル）
    let mut engine = PolicyApplicationEngine::with_validation_level(
        "/path/to/policies", 
        ValidationLevel::Strict
    );
    
    // 2. 監視するポリシーファイルを追加
    engine.add_policy_file("/path/to/policies/security.toml");
    engine.add_policy_file("/path/to/policies/monitoring.yaml");
    
    // 3. イベント監視を設定
    let mut event_receiver = engine.subscribe();
    
    // 4. エンジンを開始
    engine.start().await?;
    
    // 5. ポリシー変更イベントを処理
    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            match event.event_type {
                PolicyApplicationEventType::PolicyApplied => {
                    println!("✅ ポリシー適用成功: {}", event.policy_id);
                }
                PolicyApplicationEventType::PolicyValidationFailed => {
                    println!("❌ ポリシー検証失敗: {}", event.policy_id);
                }
                _ => {}
            }
        }
    });
    
    // アプリケーションを実行
    // ...
    
    // 6. 終了時にエンジンを停止
    engine.stop();
    
    Ok(())
}
```

## 📝 ポリシーファイル形式

### TOML形式の例

```toml
id = "production-security-policy"
name = "Production Security Policy"
version = "2.1.0"
description = "本番環境向け厳格セキュリティポリシー"
created_at = "2024-11-04T18:00:00Z"
updated_at = "2024-11-04T18:00:00Z"

[security]
enabled = true

[security.encryption]
enabled = true
algorithm = "AES-256-GCM"
key_size = 256
pbkdf2_iterations = 100000

[security.tls]
enforce = true
min_version = "TLSv1.3"
cipher_suites = [
    "TLS_AES_256_GCM_SHA384",
    "TLS_CHACHA20_POLY1305_SHA256"
]

[security.rate_limiting]
enabled = true
requests_per_minute = 100
burst_size = 20
window_size_seconds = 60

[security.input_validation]
enabled = true
max_input_length = 1048576
sql_injection_protection = true
xss_protection = true

[monitoring]
enabled = true
interval_seconds = 60
log_level = "warn"
alerts_enabled = true

[monitoring.metrics]
enabled = true
sampling_rate = 1.0
buffer_size = 2000

[authentication]
enabled = true
method = "oauth2"
require_mfa = true
session_timeout_seconds = 3600

[custom]
environment = "production"
compliance_mode = "strict"
```

### YAML形式の例

```yaml
id: "development-policy"
name: "Development Policy"
version: "1.0.0"
description: "開発環境向けポリシー"
created_at: "2024-11-04T18:00:00Z"
updated_at: "2024-11-04T18:00:00Z"

security:
  enabled: true
  encryption:
    enabled: true
    algorithm: "AES-256-GCM"
    key_size: 256
    pbkdf2_iterations: 50000
  tls:
    enforce: false
    min_version: "TLSv1.2"
    cipher_suites:
      - "TLS_AES_256_GCM_SHA384"
  rate_limiting:
    enabled: true
    requests_per_minute: 300
    burst_size: 50
    window_size_seconds: 60
  input_validation:
    enabled: true
    max_input_length: 2097152
    sql_injection_protection: true
    xss_protection: true

monitoring:
  enabled: true
  interval_seconds: 30
  log_level: "debug"
  alerts_enabled: false
  metrics:
    enabled: true
    sampling_rate: 0.5
    buffer_size: 1000

authentication:
  enabled: true
  method: "basic"
  require_mfa: false
  session_timeout_seconds: 7200

custom:
  environment: "development"
  compliance_mode: "basic"
```

## 🔍 検証レベルの詳細

### Basic（基本検証）
- 構文チェック（必須フィールド、データ型）
- 基本的な値の範囲チェック
- 高速処理重視

### Standard（標準検証）
- Basic + 論理整合性チェック
- 設定間の相互関係確認
- 推奨設定の提案

### Strict（厳格検証）
- Standard + セキュリティ要件チェック
- 暗号化強度、TLSバージョンなどの厳格な検証
- 本番環境での使用推奨

### Custom（カスタム検証）
- Strict + 環境固有ルール
- カスタムフィールドの検証
- 企業ポリシーに応じたルール適用

## 📊 パフォーマンス特性

### 処理時間

| 処理 | 平均時間 | 最大時間 |
|------|----------|----------|
| ファイル変更検知 | 5ms | 15ms |
| ポリシー検証 | 1-3ms | 10ms |
| ポリシー適用 | 10-15ms | 25ms |
| **合計** | **15-20ms** | **50ms** |

### 推奨制限

| 項目 | 推奨値 | 最大値 |
|------|--------|--------|
| 監視ファイル数 | 10個以下 | 50個 |
| ポリシーファイルサイズ | 1MB以下 | 10MB |
| 更新頻度 | 1分間隔以上 | 10秒間隔 |

## 🛡️ セキュリティガイドライン

### 本番環境での推奨設定

```rust
// 厳格な検証レベルを使用
let engine = PolicyApplicationEngine::with_validation_level(
    policy_dir,
    ValidationLevel::Strict
);

// 適切なファイル権限
// chmod 600 /path/to/policies/*.toml

// ログ監視の設定
engine.subscribe().await.for_each(|event| {
    match event.event_type {
        PolicyApplicationEventType::PolicyValidationFailed => {
            // セキュリティログに記録
            security_log::warn("Policy validation failed", &event);
        }
        _ => {}
    }
});
```

### セキュリティ検証項目

1. **暗号化設定**
   - キーサイズ: 256ビット以上
   - PBKDF2反復: 100,000回以上
   - 安全なアルゴリズムの使用

2. **TLS設定**
   - 最小バージョン: TLSv1.2以上（推奨: TLSv1.3）
   - 安全な暗号スイートの使用
   - 証明書検証の有効化

3. **認証設定**
   - MFA（多要素認証）の有効化
   - 適切なセッションタイムアウト
   - 安全な認証方式の選択

## 🚨 トラブルシューティング

### よくある問題と解決方法

#### 1. ポリシー検証失敗

**症状**: `PolicyValidationFailed` イベントが発生

**原因と解決方法**:
```bash
# ログで詳細エラーを確認
RUST_LOG=debug cargo run

# 一般的なエラー:
# - 空のID/名前 → 適切な値を設定
# - 不正なバージョン形式 → "x.y.z" 形式を使用
# - セキュリティ要件不足 → 推奨値に調整
```

#### 2. ファイル変更が検知されない

**症状**: ポリシーファイルを更新してもイベントが発生しない

**解決方法**:
```rust
// ファイル権限を確認
// パス指定が正しいかチェック
// 一時的にValidationLevel::Basicで試行
```

#### 3. パフォーマンスの低下

**症状**: ポリシー適用に時間がかかる

**解決方法**:
```rust
// 検証レベルを下げる
let engine = PolicyApplicationEngine::with_validation_level(
    policy_dir,
    ValidationLevel::Basic  // Strictから変更
);

// ファイルサイズを確認（1MB以下推奨）
// 監視ファイル数を減らす
```

## 📚 API リファレンス

### PolicyApplicationEngine

#### 主要メソッド

```rust
// エンジン作成
pub fn new<P: AsRef<Path>>(watch_path: P) -> Self
pub fn with_validation_level<P: AsRef<Path>>(
    watch_path: P, 
    validation_level: ValidationLevel
) -> Self

// ファイル管理
pub fn add_policy_file<P: AsRef<Path>>(&mut self, path: P)

// エンジン制御
pub async fn start(&self) -> Result<(), McpError>
pub fn stop(&self)

// イベント監視
pub fn subscribe(&self) -> broadcast::Receiver<PolicyApplicationEvent>

// 状態取得
pub async fn get_current_policy(&self) -> PolicyConfig
pub async fn get_validation_stats(&self) -> ValidationStats
```

### PolicyValidationEngine

#### 主要メソッド

```rust
// エンジン作成
pub fn new() -> Self
pub fn with_rules(rules: ValidationRules) -> Self

// 検証実行
pub async fn validate_policy(
    &mut self,
    policy: &PolicyConfig,
    level: ValidationLevel,
) -> ValidationResult

// 統計取得
pub fn get_stats(&self) -> &ValidationStats
```

### イベント型

```rust
pub enum PolicyApplicationEventType {
    PolicyLoaded,              // ポリシー読み込み
    PolicyApplied,            // ポリシー適用成功
    PolicyApplicationFailed,   // ポリシー適用失敗
    PolicyValidationFailed,   // ポリシー検証失敗
}
```

## 🧪 テストとデバッグ

### 統合テストの実行

```bash
# 全統合テストを実行
cargo test --test policy_hot_reload_integration

# 特定のテストを実行
cargo test test_complete_policy_hot_reload_workflow
cargo test test_performance_bulk_policy_updates
cargo test test_validation_integration
```

### デモの実行

```bash
# ポリシー設定管理デモ
cargo run --example policy_config_demo

# ポリシー検証システムデモ
cargo run --example policy_validation_demo

# 統合ポリシーデモ
cargo run --example integrated_policy_demo
```

### デバッグログの有効化

```bash
# 詳細ログを有効化
RUST_LOG=debug cargo run

# 特定モジュールのログのみ
RUST_LOG=mcp_rs::policy_application=debug cargo run
```

## 🔧 高度な設定

### カスタム検証ルールの作成

```rust
use mcp_rs::policy_validation::{ValidationRules, PolicyValidationEngine};

let custom_rules = ValidationRules {
    require_mandatory_fields: true,
    strict_security_validation: true,
    validate_custom_fields: true,
    validate_value_ranges: true,
    validate_logical_consistency: true,
};

let mut engine = PolicyValidationEngine::with_rules(custom_rules);
```

### 環境固有の設定

```rust
// 環境別設定例
match environment {
    "production" => {
        PolicyApplicationEngine::with_validation_level(
            policy_dir, 
            ValidationLevel::Strict
        )
    }
    "development" => {
        PolicyApplicationEngine::with_validation_level(
            policy_dir, 
            ValidationLevel::Basic
        )
    }
    _ => {
        PolicyApplicationEngine::with_validation_level(
            policy_dir, 
            ValidationLevel::Standard
        )
    }
}
```

## 📈 モニタリングと運用

### 推奨監視項目

1. **ポリシー適用成功率**: 95%以上を維持
2. **検証失敗率**: 5%以下を目標
3. **平均適用時間**: 50ms以下
4. **ファイル監視の応答性**: 100ms以下

### ログ分析

```bash
# エラーログの分析
grep "PolicyValidationFailed" application.log | wc -l

# パフォーマンス分析
grep "ポリシー適用成功" application.log | \
  grep -o '[0-9]\+ms' | sort -n
```

## 🔄 アップグレードガイド

### バージョン間の移行

Policy Hot-Reload システムは後方互換性を維持していますが、新機能を活用するためのアップグレード手順:

1. **設定ファイルの更新**: 新しいフィールドの追加
2. **検証レベルの見直し**: より厳格な検証への移行
3. **イベント処理の更新**: 新しいイベント型への対応

## 🤝 サポートとコミュニティ

### リソース

- **GitHub Issues**: バグレポートや機能要求
- **Documentation**: 詳細なAPIドキュメント
- **Examples**: 実用的な使用例

### 貢献方法

1. Fork the repository
2. Create a feature branch
3. Implement your changes
4. Add tests
5. Submit a pull request

---

この Guide は Policy Hot-Reload システムの包括的な利用ガイドです。さらに詳細な情報が必要な場合は、プロジェクトのドキュメントやソースコードを参照してください。