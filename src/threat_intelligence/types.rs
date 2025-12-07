//! Threat Intelligence Types
//!
//! 脅威インテリジェンス統合で使用される基本的なデータ構造を定義

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 脅威インテリジェンス情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIntelligence {
    /// 一意識別子
    pub id: String,
    /// 脅威の種類
    pub threat_type: ThreatType,
    /// 深刻度レベル
    pub severity: SeverityLevel,
    /// 脅威指標のリスト
    pub indicators: Vec<ThreatIndicator>,
    /// 情報源
    pub source: ThreatSource,
    /// 信頼度スコア (0.0 - 1.0)
    pub confidence_score: f64,
    /// 初回検出時刻
    pub first_seen: DateTime<Utc>,
    /// 最終検出時刻
    pub last_seen: DateTime<Utc>,
    /// 有効期限
    pub expiration: Option<DateTime<Utc>>,
    /// 追加メタデータ
    pub metadata: ThreatMetadata,
}

/// 脅威の種類
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThreatType {
    /// マルウェア
    Malware,
    /// フィッシング
    Phishing,
    /// C&C (Command and Control) サーバー
    CommandAndControl,
    /// ボットネット
    Botnet,
    /// スパム送信元
    Spam,
    /// 悪意あるIP
    MaliciousIp,
    /// 悪意あるドメイン
    MaliciousDomain,
    /// 悪意あるURL
    MaliciousUrl,
    /// 脆弱性悪用
    Exploit,
    /// その他
    Other(String),
}

/// 深刻度レベル
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SeverityLevel {
    /// 情報レベル
    Info,
    /// 低
    Low,
    /// 中
    #[default]
    Medium,
    /// 高
    High,
    /// 緊急
    Critical,
}

/// 脅威指標 (IOC - Indicator of Compromise)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIndicator {
    /// 指標の種類
    pub indicator_type: IndicatorType,
    /// 指標の値
    pub value: String,
    /// マッチングパターン（正規表現等）
    pub pattern: Option<String>,
    /// タグ
    pub tags: Vec<String>,
    /// コンテキスト情報
    pub context: Option<String>,
    /// 初回発見時刻
    pub first_seen: chrono::DateTime<chrono::Utc>,
}

/// 指標の種類
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndicatorType {
    /// IPアドレス
    IpAddress,
    /// ドメイン名
    Domain,
    /// URL
    Url,
    /// ファイルハッシュ (MD5, SHA1, SHA256)
    FileHash,
    /// メールアドレス
    Email,
    /// メールアドレス
    EmailAddress,
    /// User-Agent文字列
    UserAgent,
    /// HTTPヘッダー
    HttpHeader,
    /// SSL証明書
    Certificate,
    /// レジストリキー
    RegistryKey,
    /// ファイルパス
    FilePath,
}

/// 脅威情報源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatSource {
    /// プロバイダー名
    pub provider: String,
    /// フィード名
    pub feed_name: String,
    /// ソースの信頼度 (0.0 - 1.0)
    pub reliability: f64,
    /// 最終更新時刻
    pub last_updated: DateTime<Utc>,
}

/// 脅威メタデータ
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThreatMetadata {
    /// 説明
    pub description: Option<String>,
    /// 関連する攻撃手法
    pub attack_techniques: Vec<String>,
    /// 関連するCVE
    pub cve_references: Vec<String>,
    /// 関連するマルウェアファミリー
    pub malware_families: Vec<String>,
    /// 地理的情報
    pub geolocation: Option<GeolocationInfo>,
    /// カスタム属性
    pub custom_attributes: HashMap<String, String>,
}

/// 地理的情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeolocationInfo {
    /// 国コード
    pub country_code: String,
    /// 国名
    pub country_name: String,
    /// 地域
    pub region: Option<String>,
    /// 都市
    pub city: Option<String>,
    /// 緯度
    pub latitude: Option<f64>,
    /// 経度
    pub longitude: Option<f64>,
}

/// 脅威評価結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatAssessment {
    /// 評価対象の指標
    pub indicator: ThreatIndicator,
    /// 脅威が検出されたかどうか
    pub is_threat: bool,
    /// 脅威レベル
    pub threat_level: SeverityLevel,
    /// 総合信頼度スコア
    pub confidence_score: f64,
    /// マッチした脅威インテリジェンス
    pub matched_threats: Vec<ThreatIntelligence>,
    /// 評価実行時刻
    pub assessed_at: DateTime<Utc>,
    /// 評価にかかった時間（ミリ秒）
    pub assessment_duration_ms: u64,
    /// 追加コンテキスト
    pub context: ThreatAssessmentContext,
}

/// 脅威評価コンテキスト
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThreatAssessmentContext {
    /// 評価に使用されたプロバイダー
    pub providers_used: Vec<String>,
    /// キャッシュからの結果かどうか
    pub from_cache: bool,
    /// レスポンス時間の内訳
    pub timing_breakdown: HashMap<String, u64>,
    /// エラーメッセージ（部分的な失敗の場合）
    pub warnings: Vec<String>,
}

/// プロバイダー設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// プロバイダー名
    pub name: String,
    /// 有効/無効
    pub enabled: bool,
    /// APIキー
    pub api_key: String,
    /// ベースURL
    pub base_url: String,
    /// タイムアウト（秒）
    pub timeout_seconds: u32,
    /// レート制限（1分あたりのリクエスト数）
    pub rate_limit_per_minute: u32,
    /// 信頼度調整係数
    pub reliability_factor: f64,
    /// プロバイダー固有の設定
    pub provider_specific: HashMap<String, String>,
}

/// 脅威インテリジェンスエラー
#[derive(Debug, thiserror::Error)]
pub enum ThreatError {
    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid configuration: {0}")]
    ConfigurationError(String),

    #[error("Rate limit exceeded for provider: {0}")]
    RateLimitExceeded(String),

    #[error("Parsing error: {0}")]
    ParsingError(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Evaluation error: {0}")]
    EvaluationError(String),
}

/// 脅威検出統計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThreatDetectionStats {
    /// 総チェック数
    pub total_checks: u64,
    /// 脅威検出数
    pub threats_detected: u64,
    /// プロバイダー別統計
    pub provider_stats: HashMap<String, ProviderStats>,
    /// 脅威タイプ別統計
    pub threat_type_stats: HashMap<String, u64>,
    /// 深刻度別統計
    pub severity_stats: HashMap<String, u64>,
    /// キャッシュヒット率
    pub cache_hit_rate: f64,
    /// 平均応答時間（ミリ秒）
    pub avg_response_time_ms: f64,
    /// 統計期間
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// プロバイダー統計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderStats {
    /// リクエスト数
    pub requests: u64,
    /// 成功数
    pub successes: u64,
    /// エラー数
    pub errors: u64,
    /// 平均応答時間（ミリ秒）
    pub avg_response_time_ms: f64,
    /// 脅威検出数
    pub threats_found: u64,
}

impl ThreatIntelligence {
    /// 新しい脅威インテリジェンス情報を作成
    pub fn new(
        threat_type: ThreatType,
        severity: SeverityLevel,
        indicators: Vec<ThreatIndicator>,
        source: ThreatSource,
        confidence_score: f64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            threat_type,
            severity,
            indicators,
            source,
            confidence_score,
            first_seen: now,
            last_seen: now,
            expiration: None,
            metadata: ThreatMetadata::default(),
        }
    }

    /// 脅威が有効かどうかを確認
    pub fn is_valid(&self) -> bool {
        if let Some(expiration) = self.expiration {
            Utc::now() < expiration
        } else {
            true
        }
    }

    /// 指定された指標とマッチするかどうかを確認
    pub fn matches_indicator(&self, indicator: &ThreatIndicator) -> bool {
        self.indicators
            .iter()
            .any(|ti| ti.indicator_type == indicator.indicator_type && ti.value == indicator.value)
    }
}

impl SeverityLevel {
    /// 数値スコアを取得（高いほど危険）
    pub fn score(&self) -> u8 {
        match self {
            SeverityLevel::Info => 1,
            SeverityLevel::Low => 2,
            SeverityLevel::Medium => 3,
            SeverityLevel::High => 4,
            SeverityLevel::Critical => 5,
        }
    }

    /// 文字列から変換
    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "info" => Some(SeverityLevel::Info),
            "low" => Some(SeverityLevel::Low),
            "medium" => Some(SeverityLevel::Medium),
            "high" => Some(SeverityLevel::High),
            "critical" => Some(SeverityLevel::Critical),
            _ => None,
        }
    }
}
