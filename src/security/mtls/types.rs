//! mTLS Certificate Types
//!
//! mTLS証明書管理システムの型定義

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 証明書マネージャー設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CertificateManagerConfig {
    /// CA設定
    pub ca_config: CaConfig,
    /// ストア設定
    pub store_config: StoreConfig,
    /// OCSP設定
    pub ocsp_config: OcspConfig,
    /// ローテーション設定
    pub rotation_config: RotationConfig,
}

/// CA設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaConfig {
    /// ルート証明書パス
    pub root_cert_path: String,
    /// ルート秘密鍵パス
    pub root_key_path: String,
    /// 中間証明書パス
    pub intermediate_cert_path: Option<String>,
    /// 中間秘密鍵パス
    pub intermediate_key_path: Option<String>,
    /// 鍵アルゴリズム
    pub key_algorithm: KeyAlgorithm,
    /// CRLパス
    pub crl_path: Option<String>,
}

/// ストア設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConfig {
    /// 証明書保存パス
    pub cert_dir: String,
    /// データベースURL
    pub database_url: Option<String>,
    /// 最大保存数
    pub max_certificates: usize,
}

/// OCSP設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcspConfig {
    /// OCSPレスポンダーURL
    pub responder_url: String,
    /// キャッシュ有効期限（秒）
    pub cache_ttl: u64,
    /// Nonceサポート
    pub enable_nonce: bool,
}

/// ローテーション設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    /// 自動ローテーション有効化
    pub enable_auto_rotation: bool,
    /// ローテーション前日数
    pub rotation_days_before_expiry: u32,
    /// 猶予期間（日）
    pub grace_period_days: u32,
    /// 通知設定
    pub notification_config: Option<NotificationConfig>,
}

/// 通知設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    /// 通知先メールアドレス
    pub email_addresses: Vec<String>,
    /// Webhook URL
    pub webhook_url: Option<String>,
}

/// 鍵アルゴリズム
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyAlgorithm {
    /// RSA 2048ビット
    Rsa2048,
    /// RSA 4096ビット
    Rsa4096,
    /// ECDSA P-256
    EcdsaP256,
    /// ECDSA P-384
    EcdsaP384,
}

/// 証明書要求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateRequest {
    /// Common Name
    pub common_name: String,
    /// Subject Alternative Names
    pub subject_alt_names: Vec<String>,
    /// 有効期限（日数）
    pub validity_days: u32,
    /// Key Usage
    pub key_usage: Vec<KeyUsage>,
    /// Extended Key Usage
    pub extended_key_usage: Vec<ExtendedKeyUsage>,
}

/// 発行済み証明書
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedCertificate {
    /// シリアル番号
    pub serial_number: String,
    /// サブジェクト
    pub subject: Subject,
    /// 発行者
    pub issuer: Subject,
    /// 有効開始日時
    pub not_before: DateTime<Utc>,
    /// 有効終了日時
    pub not_after: DateTime<Utc>,
    /// 有効期限（日数）
    pub validity_days: u32,
    /// Subject Alternative Names
    pub subject_alt_names: Vec<String>,
    /// Key Usage
    pub key_usage: Vec<KeyUsage>,
    /// Extended Key Usage
    pub extended_key_usage: Vec<ExtendedKeyUsage>,
    /// 証明書PEM
    pub certificate_pem: String,
    /// 秘密鍵PEM
    pub private_key_pem: String,
    /// 証明書チェーンPEM
    pub chain_pem: Vec<String>,
    /// 発行日時
    pub issued_at: DateTime<Utc>,
    /// ステータス
    pub status: CertificateStatus,
}

/// 証明書
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    /// シリアル番号
    pub serial_number: String,
    /// サブジェクト
    pub subject: Subject,
    /// 発行者
    pub issuer: Subject,
    /// 有効開始日時
    pub not_before: DateTime<Utc>,
    /// 有効終了日時
    pub not_after: DateTime<Utc>,
    /// 有効期限（日数）
    pub validity_days: u32,
    /// Subject Alternative Names
    pub subject_alt_names: Vec<String>,
    /// Key Usage
    pub key_usage: Vec<KeyUsage>,
    /// Extended Key Usage
    pub extended_key_usage: Vec<ExtendedKeyUsage>,
    /// 証明書PEM
    pub certificate_pem: String,
    /// 証明書チェーンPEM
    pub chain_pem: Vec<String>,
    /// ステータス
    pub status: CertificateStatus,
}

/// サブジェクト情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subject {
    /// Common Name
    pub common_name: String,
    /// Organization
    pub organization: Option<String>,
    /// Organizational Unit
    pub organizational_unit: Option<String>,
    /// Country
    pub country: Option<String>,
    /// State/Province
    pub state: Option<String>,
    /// Locality
    pub locality: Option<String>,
}

/// Key Usage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyUsage {
    /// Digital Signature
    DigitalSignature,
    /// Non Repudiation
    NonRepudiation,
    /// Key Encipherment
    KeyEncipherment,
    /// Data Encipherment
    DataEncipherment,
    /// Key Agreement
    KeyAgreement,
    /// Certificate Signing
    CertificateSigning,
    /// CRL Signing
    CrlSigning,
}

/// Extended Key Usage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExtendedKeyUsage {
    /// Server Authentication
    ServerAuth,
    /// Client Authentication
    ClientAuth,
    /// Code Signing
    CodeSigning,
    /// Email Protection
    EmailProtection,
    /// Time Stamping
    TimeStamping,
    /// OCSP Signing
    OcspSigning,
}

/// 証明書ステータス
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CertificateStatus {
    /// アクティブ
    Active,
    /// 期限切れ
    Expired,
    /// 失効済み
    Revoked,
    /// 一時停止
    Suspended,
    /// 猶予期間中
    GracePeriod,
}

/// 失効理由
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RevocationReason {
    /// 未指定
    Unspecified,
    /// 鍵の危殆化
    KeyCompromise,
    /// CA危殆化
    CaCompromise,
    /// 所属変更
    AffiliationChanged,
    /// 上書き
    Superseded,
    /// 運用停止
    CessationOfOperation,
    /// 証明書保留
    CertificateHold,
}

/// 検証結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// 検証成功
    pub valid: bool,
    /// チェーン検証成功
    pub chain_valid: bool,
    /// 失効チェック成功
    pub not_revoked: bool,
    /// 有効期限内
    pub not_expired: bool,
    /// エラーメッセージ
    pub errors: Vec<String>,
    /// 警告メッセージ
    pub warnings: Vec<String>,
}

/// OCSP応答
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcspResponse {
    /// シリアル番号
    pub serial_number: String,
    /// ステータス
    pub status: OcspStatus,
    /// 応答日時
    pub produced_at: DateTime<Utc>,
    /// 次回更新日時
    pub next_update: Option<DateTime<Utc>>,
    /// 失効理由
    pub revocation_reason: Option<RevocationReason>,
    /// 失効日時
    pub revocation_time: Option<DateTime<Utc>>,
}

/// OCSPステータス
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OcspStatus {
    /// Good
    Good,
    /// Revoked
    Revoked,
    /// Unknown
    Unknown,
}

/// 証明書統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateStatistics {
    /// 総証明書数
    pub total_certificates: usize,
    /// アクティブ証明書数
    pub active_certificates: usize,
    /// 失効証明書数
    pub revoked_certificates: usize,
    /// 期限切れ間近（指定日数以内）
    pub expiring_soon: usize,
    /// スケジュール済みローテーション数
    pub scheduled_rotations: usize,
    /// 最終ローテーション日時
    pub last_rotation: Option<DateTime<Utc>>,
}

/// ローテーションイベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEvent {
    /// イベントID
    pub id: String,
    /// 旧証明書シリアル番号
    pub old_serial_number: String,
    /// 新証明書シリアル番号
    pub new_serial_number: String,
    /// ローテーション日時
    pub rotated_at: DateTime<Utc>,
    /// ステータス
    pub status: RotationStatus,
    /// エラーメッセージ
    pub error: Option<String>,
}

/// ローテーションステータス
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RotationStatus {
    /// スケジュール済み
    Scheduled,
    /// 進行中
    InProgress,
    /// 成功
    Success,
    /// 失敗
    Failed,
    /// ロールバック
    RolledBack,
}

impl Default for CaConfig {
    fn default() -> Self {
        Self {
            root_cert_path: "certs/root-ca.pem".to_string(),
            root_key_path: "certs/root-ca-key.pem".to_string(),
            intermediate_cert_path: None,
            intermediate_key_path: None,
            key_algorithm: KeyAlgorithm::EcdsaP384,
            crl_path: Some("certs/crl.pem".to_string()),
        }
    }
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            cert_dir: "certs/issued".to_string(),
            database_url: None,
            max_certificates: 10000,
        }
    }
}

impl Default for OcspConfig {
    fn default() -> Self {
        Self {
            responder_url: "http://ocsp.example.com".to_string(),
            cache_ttl: 3600,
            enable_nonce: true,
        }
    }
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            enable_auto_rotation: true,
            rotation_days_before_expiry: 7,
            grace_period_days: 30,
            notification_config: None,
        }
    }
}
