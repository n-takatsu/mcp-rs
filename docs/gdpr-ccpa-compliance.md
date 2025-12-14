# GDPR/CCPA Compliance Engine

## 概要

このモジュールは、EU一般データ保護規則（GDPR）およびカリフォルニア州消費者プライバシー法（CCPA）に準拠したコンプライアンスエンジンを提供します。

## 主要機能

### 1. データ主体の権利（Data Subject Rights）

#### GDPR対応
- **削除権（Right to Erasure）** - GDPR Art.17, CCPA §1798.105
- **アクセス権（Right of Access）** - GDPR Art.15, CCPA §1798.100
- **データポータビリティ権（Right to Data Portability）** - GDPR Art.20
- **訂正権（Right to Rectification）** - GDPR Art.16
- **処理制限権（Right to Restriction of Processing）** - GDPR Art.18
- **異議申立権（Right to Object）** - GDPR Art.21

### 2. 同意管理（Consent Management）

- 同意の記録と追跡
- 同意の撤回
- 同意履歴の管理
- バージョン管理

### 3. データライフサイクル管理

- カテゴリ別保持ポリシー
- 自動削除ジョブ
- データアーカイブ
- 削除予定データの追跡

### 4. 監査とレポート

- 全処理の監査ログ記録
- コンプライアンスレポート生成
- リクエスト統計
- 期限切れリクエストの追跡

## 使用例

### 基本的な使い方

```rust
use mcp_rs::compliance::*;

#[tokio::main]
async fn main() {
    // コンプライアンスエンジンの初期化
    let engine = ComplianceEngine::new();
    
    // 削除リクエストの処理
    let request = DataSubjectRequest::new(
        "user@example.com",
        RequestType::Erasure,
    );
    
    let result = engine.process_request(request).await;
    
    if let Ok(result) = result {
        println!("リクエストID: {}", result.request_id);
        println!("ステータス: {:?}", result.status);
        
        if let Some(cert) = result.certificate {
            println!("削除証明書: {}", cert);
        }
    }
}
```

### 同意管理

```rust
use mcp_rs::compliance::consent_manager::*;

#[tokio::main]
async fn main() {
    let manager = ConsentManager::new();
    
    // 同意を付与
    let consent = ConsentRecord::new(
        "user@example.com",
        "marketing",
        vec!["email".to_string(), "sms".to_string()],
        LegalBasis::Consent,
    );
    
    let consent_id = manager.grant_consent(consent).await.unwrap();
    
    // 同意を確認
    let has_consent = manager.check_consent("user@example.com", "marketing").await.unwrap();
    
    if has_consent {
        println!("ユーザーはマーケティングに同意しています");
    }
    
    // 同意を撤回
    manager.revoke_consent("user@example.com", "marketing").await.unwrap();
}
```

### データライフサイクル管理

```rust
use mcp_rs::compliance::lifecycle_manager::*;

#[tokio::main]
async fn main() {
    let manager = LifecycleManager::new();
    
    // カスタム保持ポリシーを追加
    let policy = RetentionPolicy {
        id: uuid::Uuid::new_v4().to_string(),
        data_category: DataCategory::Behavioral,
        retention_days: 365, // 1年
        reason: "ユーザー行動分析".to_string(),
        legal_basis: LegalBasis::LegitimateInterests,
        deletion_method: DeletionMethod::Anonymize,
    };
    
    manager.add_retention_policy(policy).await.unwrap();
    
    // データを登録
    let entry_id = manager.register_data(
        "user@example.com".to_string(),
        DataCategory::Behavioral,
    ).await.unwrap();
    
    // 自動削除ジョブを実行
    let deleted_count = manager.run_auto_deletion_job().await.unwrap();
    println!("{}件のデータを削除しました", deleted_count);
}
```

### リクエスト処理

```rust
use mcp_rs::compliance::data_subject_requests::*;

#[tokio::main]
async fn main() {
    let handler = DataSubjectRequestHandler::new();
    
    // リクエストを提出
    let request = DataSubjectRequest::new(
        "user@example.com",
        RequestType::Access,
    );
    
    let request_id = handler.submit_request(request).await.unwrap();
    
    // ステータスを更新
    handler.update_status(&request_id, RequestStatus::Processing).await.unwrap();
    
    // 期限切れリクエストを確認
    let overdue = handler.get_overdue_requests().await;
    
    for req in overdue {
        println!("期限切れリクエスト: {}", req.id);
        // 期限を延長
        handler.extend_deadline(&req.id, 30).await.unwrap();
    }
    
    // 統計を取得
    let stats = handler.get_statistics().await;
    println!("総リクエスト数: {}", stats.total_requests);
    println!("期限切れ: {}", stats.overdue_requests);
}
```

### コンプライアンスレポート

```rust
use mcp_rs::compliance::*;

#[tokio::main]
async fn main() {
    let engine = ComplianceEngine::new();
    
    // 過去30日間のレポートを生成
    let start_date = chrono::Utc::now() - chrono::Duration::days(30);
    let end_date = chrono::Utc::now();
    
    let report = engine.generate_report(start_date, end_date).await;
    
    println!("期間: {} - {}", report.start_date, report.end_date);
    println!("総リクエスト数: {}", report.total_requests);
    println!("処理済み: {}", report.processed_requests);
    println!("監査ログエントリ: {}", report.audit_entries);
}
```

## データカテゴリ

以下のデータカテゴリがサポートされています：

- **PersonalIdentifiable**: 氏名、住所、生年月日など
- **ContactInformation**: メールアドレス、電話番号など
- **Financial**: 銀行口座、クレジットカード情報など
- **Health**: 健康情報、医療記録など
- **Location**: GPS座標、位置履歴など
- **OnlineIdentifiers**: IPアドレス、Cookie IDなど
- **Behavioral**: 閲覧履歴、購買履歴など
- **Sensitive**: 思想、宗教、性的指向など（GDPR Art.9）

## 法的根拠（Legal Basis）

GDPR Art.6(1)に基づく以下の法的根拠がサポートされています：

- **Consent**: 同意（Art.6(1)(a)）
- **Contract**: 契約の履行（Art.6(1)(b)）
- **LegalObligation**: 法的義務の遵守（Art.6(1)(c)）
- **VitalInterests**: 重要な利益の保護（Art.6(1)(d)）
- **PublicInterest**: 公共の利益のための業務の遂行（Art.6(1)(e)）
- **LegitimateInterests**: 正当な利益の追求（Art.6(1)(f)）

## 削除方法

以下の削除方法がサポートされています：

- **SoftDelete**: 論理削除（データは保持されるがアクセス不可）
- **HardDelete**: 物理削除（データを完全に削除）
- **Anonymize**: 匿名化（個人を特定できないように変換）
- **Pseudonymize**: 仮名化（追加情報なしでは個人特定不可）

## デフォルト保持ポリシー

| データカテゴリ | 保持期間 | 削除方法 |
|--------------|---------|---------|
| PersonalIdentifiable | 7年 | HardDelete |
| ContactInformation | 3年 | SoftDelete |
| Behavioral | 1年 | Anonymize |

## 期限管理

- **GDPR**: リクエストは30日以内に処理（最大60日まで延長可能）
- **CCPA**: リクエストは45日以内に処理（最大90日まで延長可能）

期限延長には正当な理由が必要で、データ主体に通知する必要があります。

## 監査証跡

すべての処理は監査ログに記録されます：

- リクエスト受付
- ステータス変更
- データアクセス
- データ削除
- 同意の付与/撤回
- 期限延長
- リクエスト拒否

監査ログは法的監査や規制当局の要求に対応するために使用できます。

## トラブルシューティング

### リクエストが期限内に処理されない

```rust
// 期限切れリクエストを確認
let overdue = handler.get_overdue_requests().await;

for req in overdue {
    // 期限を延長（正当な理由が必要）
    handler.extend_deadline(&req.id, 30).await?;
    
    // または処理を完了
    handler.update_status(&req.id, RequestStatus::Completed).await?;
}
```

### 削除証明書が生成されない

削除証明書は`RequestType::Erasure`の完了時のみ生成されます。
証明書にはSHA-256検証ハッシュが含まれます。

### 同意撤回後もデータが残る

同意撤回はライフサイクル管理とは独立しています。
同意撤回後にデータを削除するには、削除リクエストを別途処理する必要があります。

```rust
// 同意撤回
manager.revoke_consent("user@example.com", "marketing").await?;

// データ削除
let request = DataSubjectRequest::new(
    "user@example.com",
    RequestType::Erasure,
);
engine.process_request(request).await?;
```

## APIリファレンス

### ComplianceEngine

- `new()`: 新しいエンジンインスタンスを作成
- `process_request(request)`: データ主体リクエストを処理
- `generate_report(start, end)`: コンプライアンスレポートを生成

### ConsentManager

- `grant_consent(consent)`: 同意を記録
- `revoke_consent(subject_id, purpose)`: 同意を撤回
- `check_consent(subject_id, purpose)`: 同意状態を確認
- `get_consent_history(subject_id, purpose)`: 同意履歴を取得
- `get_consent_statistics()`: 同意統計を取得

### LifecycleManager

- `add_retention_policy(policy)`: 保持ポリシーを追加
- `register_data(subject_id, category)`: データを登録
- `get_data_for_deletion()`: 削除予定データを取得
- `delete_data(entry_id)`: データを削除
- `archive_data(entry_id)`: データをアーカイブ
- `run_auto_deletion_job()`: 自動削除ジョブを実行

### DataSubjectRequestHandler

- `submit_request(request)`: リクエストを提出
- `update_status(request_id, status)`: ステータスを更新
- `get_request(request_id)`: リクエストを取得
- `get_overdue_requests()`: 期限切れリクエストを取得
- `extend_deadline(request_id, days)`: 期限を延長
- `reject_request(request_id, reason)`: リクエストを拒否
- `get_statistics()`: リクエスト統計を取得

## ベストプラクティス

1. **定期的な監査ログレビュー**: 監査ログを定期的にレビューし、異常なパターンを検出します。

2. **自動削除ジョブの定期実行**: 保持期間を超えたデータを自動的に削除するジョブを定期実行します。

3. **期限管理**: 期限切れリクエストを定期的にチェックし、適切に処理または延長します。

4. **削除証明書の保管**: 削除証明書は安全に保管し、監査時に提示できるようにします。

5. **同意のバージョン管理**: プライバシーポリシー変更時には新しいバージョンで同意を取り直します。

6. **データ最小化**: 必要最小限のデータのみを保持し、不要なデータは速やかに削除します。

7. **透明性**: データ主体に対して処理内容を明確に伝えます。

## コンプライアンスチェックリスト

### GDPR
- [ ] データ主体の権利（Art.15-21）を実装
- [ ] 30日以内の応答時間を遵守
- [ ] 削除証明書の発行
- [ ] 監査証跡の記録
- [ ] データポータビリティの機械可読形式対応
- [ ] センシティブデータの特別な取り扱い（Art.9）

### CCPA
- [ ] 消費者権利（§1798.100-120）を実装
- [ ] 45日以内の応答時間を遵守
- [ ] オプトアウト機能
- [ ] データ販売の開示
- [ ] 差別禁止の遵守

## 参考資料

- [GDPR 公式テキスト](https://eur-lex.europa.eu/eli/reg/2016/679/oj)
- [CCPA 公式テキスト](https://oag.ca.gov/privacy/ccpa)
- [GDPR ガイドライン](https://edpb.europa.eu/our-work-tools/general-guidance/gdpr-guidelines-recommendations-best-practices_en)

## ライセンス

このコードはMIT/Apache-2.0デュアルライセンスの下で提供されています。
