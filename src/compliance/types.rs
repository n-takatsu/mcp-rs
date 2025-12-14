//! Compliance Types
//!
//! GDPR/CCPAコンプライアンスに関連する型定義

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// データ主体リクエストの種類
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestType {
    /// 削除権（GDPR Art.17, CCPA §1798.105）
    Erasure,
    /// アクセス権（GDPR Art.15, CCPA §1798.100）
    Access,
    /// ポータビリティ権（GDPR Art.20）
    Portability,
    /// 訂正権（GDPR Art.16）
    Rectification,
    /// 処理制限権（GDPR Art.18）
    Restriction,
    /// 異議申立権（GDPR Art.21, CCPA §1798.120）
    Objection,
}

/// データ主体リクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSubjectRequest {
    /// リクエストID
    pub id: String,
    /// データ主体の識別子（メールアドレス等）
    pub subject_id: String,
    /// リクエストの種類
    pub request_type: RequestType,
    /// リクエスト作成日時
    pub created_at: DateTime<Utc>,
    /// 処理期限
    pub deadline: DateTime<Utc>,
    /// リクエストステータス
    pub status: RequestStatus,
    /// 完了日時
    pub completed_at: Option<DateTime<Utc>>,
    /// 追加情報
    pub metadata: HashMap<String, String>,
}

/// リクエスト処理ステータス
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RequestStatus {
    /// 受付済み
    Pending,
    /// 本人確認中
    VerificationRequired,
    /// 処理中
    Processing,
    /// 完了
    Completed,
    /// 拒否
    Rejected,
    /// 期限延長
    Extended,
}

/// リクエスト処理結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestResult {
    /// リクエストID
    pub request_id: String,
    /// 処理ステータス
    pub status: RequestStatus,
    /// 処理完了日時
    pub completed_at: Option<DateTime<Utc>>,
    /// 処理結果データ（エクスポートデータ等）
    pub data: Option<String>,
    /// 証明書
    pub certificate: Option<String>,
    /// エラーメッセージ
    pub error: Option<String>,
}

/// 同意記録
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// 同意ID
    pub id: String,
    /// データ主体の識別子
    pub subject_id: String,
    /// 同意目的
    pub purpose: String,
    /// 同意スコープ
    pub scope: Vec<String>,
    /// 同意取得日時
    pub granted_at: DateTime<Utc>,
    /// 同意撤回日時
    pub revoked_at: Option<DateTime<Utc>>,
    /// 同意バージョン
    pub version: String,
    /// 法的根拠
    pub legal_basis: LegalBasis,
}

/// 法的根拠
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LegalBasis {
    /// 同意（GDPR Art.6(1)(a)）
    Consent,
    /// 契約履行（GDPR Art.6(1)(b)）
    Contract,
    /// 法的義務（GDPR Art.6(1)(c)）
    LegalObligation,
    /// 重要な利益（GDPR Art.6(1)(d)）
    VitalInterests,
    /// 公共の利益（GDPR Art.6(1)(e)）
    PublicInterest,
    /// 正当な利益（GDPR Art.6(1)(f)）
    LegitimateInterests,
}

/// データカテゴリ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DataCategory {
    /// 個人識別情報
    PersonalIdentifiable,
    /// 連絡先情報
    ContactInformation,
    /// 財務情報
    Financial,
    /// 健康情報
    Health,
    /// 位置情報
    Location,
    /// オンライン識別子
    OnlineIdentifiers,
    /// 行動データ
    Behavioral,
    /// センシティブデータ
    Sensitive,
}

/// データ保持ポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// ポリシーID
    pub id: String,
    /// データカテゴリ
    pub data_category: DataCategory,
    /// 保持期間（日数）
    pub retention_days: u32,
    /// 保持理由
    pub reason: String,
    /// 法的根拠
    pub legal_basis: LegalBasis,
    /// 削除方法
    pub deletion_method: DeletionMethod,
}

/// 削除方法
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeletionMethod {
    /// 論理削除（マーク）
    SoftDelete,
    /// 物理削除
    HardDelete,
    /// 匿名化
    Anonymize,
    /// 仮名化
    Pseudonymize,
}

/// 監査ログエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// ログID
    pub id: String,
    /// アクション種別
    pub action: String,
    /// データ主体の識別子
    pub subject_id: String,
    /// 実行者
    pub actor: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// 詳細情報
    pub details: HashMap<String, String>,
    /// 結果
    pub result: String,
}

/// コンプライアンスレポート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// レポートID
    pub id: String,
    /// レポート期間（開始）
    pub period_start: DateTime<Utc>,
    /// レポート期間（終了）
    pub period_end: DateTime<Utc>,
    /// 処理されたリクエスト数
    pub total_requests: usize,
    /// リクエストタイプ別統計
    pub requests_by_type: HashMap<String, usize>,
    /// 平均処理時間（秒）
    pub avg_processing_time_seconds: f64,
    /// コンプライアンス違反数
    pub violations: usize,
    /// 監査ログエントリ数
    pub audit_entries: usize,
}

impl DataSubjectRequest {
    /// 新しいデータ主体リクエストを作成
    pub fn new(subject_id: impl Into<String>, request_type: RequestType) -> Self {
        let created_at = Utc::now();
        // GDPR: 30日、CCPA: 45日の期限
        let deadline_days = match request_type {
            RequestType::Erasure | RequestType::Access | RequestType::Portability => 30,
            _ => 30,
        };

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            subject_id: subject_id.into(),
            request_type,
            created_at,
            deadline: created_at + chrono::Duration::days(deadline_days),
            status: RequestStatus::Pending,
            completed_at: None,
            metadata: HashMap::new(),
        }
    }
}

impl ConsentRecord {
    /// 新しい同意記録を作成
    pub fn new(
        subject_id: impl Into<String>,
        purpose: impl Into<String>,
        scope: Vec<String>,
        legal_basis: LegalBasis,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            subject_id: subject_id.into(),
            purpose: purpose.into(),
            scope,
            granted_at: Utc::now(),
            revoked_at: None,
            version: "1.0".to_string(),
            legal_basis,
        }
    }

    /// 同意を撤回
    pub fn revoke(&mut self) {
        self.revoked_at = Some(Utc::now());
    }

    /// 同意が有効かどうか
    pub fn is_valid(&self) -> bool {
        self.revoked_at.is_none()
    }
}
