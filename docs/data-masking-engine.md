# データマスキングエンジン

Issue #87 (P1 High) - データベースクエリ結果に対する高度なデータマスキング機能

## 概要

データマスキングエンジンは、データベースから取得したデータに対して自動的にマスキング処理を適用し、個人情報や機密情報を保護します。GDPR/CCPA等のコンプライアンス要件に対応し、ロールベースの柔軟なマスキングポリシーを提供します。

## 主要機能

### 5つのマスキングタイプ

1. **FullMask (完全マスク)**
   - すべての文字をマスク文字 (`*`) に置換
   - 用途: パスワード、秘密鍵など完全に隠すべき情報

2. **PartialMask (部分マスク)**
   - 先頭と末尾の指定文字数のみ表示
   - 用途: クレジットカード番号、メールアドレス

3. **HashMask (ハッシュマスク)**
   - SHA256/SHA512でハッシュ化
   - 用途: 検索・比較が必要だが内容は隠したいデータ

4. **FormatPreserving (形式保持マスク)**
   - データ形式を保持したままマスク
   - 用途: 電話番号、社会保障番号 (SSN)

5. **TokenMask (トークン化)**
   - 一意のトークンに置換
   - 用途: 可逆的なマスキングが必要なデータ

### カラムパターンマッチング

- **完全一致**: `["email", "password"]`
- **ワイルドカード**: `["*_email", "credit_*"]`
- **正規表現**: `["^user_.*", ".*_ssn$"]`
- **データタイプ**: `[String, Integer, Date]`

### 権限ベースマスキング

- **ロールベース制御**: 管理者、ユーザー、ゲストで異なるマスキング
- **時間制約**: 営業時間外は強化マスク
- **ネットワーク制約**: IP/地域ベースの制御
- **用途ベース**: 通常利用、分析、監査で異なるマスキング

### パフォーマンス最適化

- **ルールキャッシュ**: カラム名→ルールのマッピングをキャッシュ
- **遅延評価**: 必要なカラムのみマスキング
- **バッチ処理**: 複数レコードを効率的に処理

### 監査・コンプライアンス

- **監査ログ**: すべてのマスキング操作を記録
- **統計情報**: マスキング適用状況の可視化
- **GDPR/CCPA対応**: アクセスログ、アンマスク記録

## 使用例

### 基本的な使用方法

```rust
use mcp_rs::handlers::database::{
    DataMaskingEngine, MaskingPolicy, MaskingRule, MaskingType,
    MaskingContext, MaskingPurpose, ColumnPattern,
};

// エンジンを作成
let engine = DataMaskingEngine::new();

// ポリシーを定義
let policy = MaskingPolicy {
    name: "user_data_policy".to_string(),
    roles: vec!["user".to_string()],
    permissions: vec![],
    time_constraints: None,
    network_constraints: None,
    rules: vec![
        MaskingRule {
            name: "email_rule".to_string(),
            description: Some("メールアドレスを部分マスク".to_string()),
            masking_type: MaskingType::PartialMask {
                prefix_visible: 1,
                suffix_visible: 0,
            },
            column_pattern: ColumnPattern {
                exact_match: Some(vec!["email".to_string()]),
                wildcard_patterns: None,
                regex_patterns: None,
                data_types: None,
            },
            priority: 10,
            enabled: true,
        },
    ],
};

engine.add_policy(policy).await?;

// データをマスキング
let mut data = serde_json::json!({
    "id": 1,
    "email": "user@example.com",
    "name": "John Doe"
});

let context = MaskingContext {
    roles: vec!["user".to_string()],
    permissions: vec![],
    source_ip: Some("192.168.1.1".to_string()),
    timestamp: chrono::Utc::now(),
    purpose: MaskingPurpose::Normal,
};

engine.mask_query_result(&mut data, &context).await?;
// => { "id": 1, "email": "u***", "name": "John Doe" }
```

### 事前定義フォーマッタ

```rust
use mcp_rs::handlers::database::PredefinedFormatters;

// クレジットカード (末尾4桁のみ表示)
let cc_masking = PredefinedFormatters::credit_card();

// メールアドレス (先頭1文字のみ表示)
let email_masking = PredefinedFormatters::email();

// 電話番号 (形式保持)
let phone_masking = PredefinedFormatters::phone_number();

// 社会保障番号 (SSN形式保持)
let ssn_masking = PredefinedFormatters::ssn();

// パスワード (完全マスク)
let password_masking = PredefinedFormatters::password();

// IPアドレス (先頭7文字のみ表示)
let ip_masking = PredefinedFormatters::ip_address();
```

### 時間制約付きポリシー

```rust
use mcp_rs::handlers::database::{TimeConstraints, TimeRange};

let policy = MaskingPolicy {
    name: "business_hours_policy".to_string(),
    roles: vec![],
    permissions: vec![],
    time_constraints: Some(TimeConstraints {
        allowed_weekdays: vec![1, 2, 3, 4, 5], // 月〜金
        allowed_time_ranges: vec![
            TimeRange {
                start: "09:00".to_string(),
                end: "18:00".to_string(),
            },
        ],
    }),
    network_constraints: None,
    rules: vec![/* ... */],
};
```

### ネットワーク制約付きポリシー

```rust
use mcp_rs::handlers::database::NetworkConstraints;

let policy = MaskingPolicy {
    name: "network_policy".to_string(),
    roles: vec![],
    permissions: vec![],
    time_constraints: None,
    network_constraints: Some(NetworkConstraints {
        allowed_ips: vec!["192.168.1.0/24".to_string()],
        denied_ips: vec!["192.168.1.100".to_string()],
        allowed_regions: vec!["JP".to_string(), "US".to_string()],
    }),
    rules: vec![/* ... */],
};
```

### 監査ログの取得

```rust
// 最新10件の監査ログを取得
let audit_log = engine.get_audit_log(Some(10)).await;

for entry in audit_log {
    println!(
        "[{}] カラム: {} | ルール: {} | ロール: {:?}",
        entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
        entry.column_name,
        entry.rule_name,
        entry.user_roles
    );
}
```

### 統計情報の取得

```rust
let stats = engine.get_statistics().await;

println!("総マスキング数: {}", stats.total_maskings);
println!("ポリシー数: {}", stats.policy_count);
println!("キャッシュサイズ: {}", stats.cache_size);

// マスキングタイプ別カウント
for (mask_type, count) in stats.masking_type_counts {
    println!("{}: {}", mask_type, count);
}

// カラム別カウント
for (column, count) in stats.column_counts {
    println!("{}: {}", column, count);
}
```

## デモプログラム

```bash
cargo run --example data_masking_demo --features database
```

5つのマスキングタイプすべてを実演するデモプログラムが実行されます。

## テスト

```bash
# ビルドテスト
cargo build --features database

# ユニットテスト
cargo test --features database --lib

# 全テスト
cargo test --features database
```

## ファイル構成

```
src/handlers/database/
├── data_masking.rs          # メインエンジン
├── masking_rules.rs         # ルール定義
├── masking_formatters.rs    # フォーマッタ実装
└── mod.rs                   # モジュール定義

examples/
└── data_masking_demo.rs     # デモプログラム
```

## アーキテクチャ

```
┌─────────────────────────────────────────┐
│      DataMaskingEngine                  │
├─────────────────────────────────────────┤
│  - ポリシー管理                         │
│  - ルールキャッシュ                     │
│  - 監査ログ                             │
│  - 統計情報                             │
└─────────────┬───────────────────────────┘
              │
              ├──► MaskingPolicy (複数)
              │      ├─ ロール/パーミッション
              │      ├─ 時間/ネットワーク制約
              │      └─ MaskingRule (複数)
              │           ├─ カランパターン
              │           └─ MaskingType
              │
              └──► MaskingFormatter
                     ├─ FullMask
                     ├─ PartialMask
                     ├─ HashMask
                     ├─ FormatPreserving
                     └─ TokenMask
```

## パフォーマンス

- **ルールキャッシュ**: カラム名→ルールのマッピングをキャッシュし、繰り返しのパターンマッチングを回避
- **遅延評価**: 必要なカラムのみマスキング処理を実行
- **非同期処理**: tokio async/awaitで並列処理

## セキュリティ

- **トークンマップ**: トークン化されたデータの元の値をメモリ内で安全に管理
- **監査ログ**: すべてのマスキング操作を記録し、コンプライアンス対応
- **権限チェック**: RBAC統合で細かいアクセス制御

## 今後の拡張予定

- [ ] **バッチ処理**: 大量データの効率的なマスキング
- [ ] **結果キャッシュ**: 同一データのマスキング結果をキャッシュ
- [ ] **カスタムルール**: ユーザー定義のマスキングロジック
- [ ] **データタイプ対応**: JSON/日付/数値等の型別マスキング
- [ ] **パフォーマンス計測**: マスキング処理時間の計測・最適化
- [ ] **コンプライアンスレポート**: GDPR/CCPA準拠レポート生成

## ライセンス

MIT OR Apache-2.0

## 関連Issue

- Issue #87: データマスキングエンジンの実装 (P1 High)
- Issue #86: カラムレベル暗号化 (P1 High) - 次のステップ
- Issue #99: WebSocket Phase 2 (P1 High) - 並行作業可能
