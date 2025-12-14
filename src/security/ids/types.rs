//! IDS Types
//!
//! 侵入検知システムの型定義

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

/// 検知タイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DetectionType {
    /// SQLインジェクション
    SqlInjection,
    /// クロスサイトスクリプティング
    XssAttack,
    /// ブルートフォース攻撃
    BruteForce,
    /// DDoS攻撃
    DdosAttack,
    /// ポートスキャン
    PortScan,
    /// 不正アクセス試行
    UnauthorizedAccess,
    /// データ窃取
    DataExfiltration,
    /// マルウェア活動
    MalwareActivity,
    /// 異常な振る舞い
    AnomalousBehavior,
    /// その他
    Other,
}

/// 深刻度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
    /// 緊急
    Critical,
}

/// 検知結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// 侵入検知フラグ
    pub is_intrusion: bool,
    /// 信頼度（0.0-1.0）
    pub confidence: f64,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// 攻撃タイプの詳細
    pub attack_details: AttackDetails,
    /// 送信元情報
    pub source_info: SourceInfo,
    /// 推奨アクション
    pub recommended_action: RecommendedAction,
    /// 検知時刻
    pub detected_at: DateTime<Utc>,
    /// 分析時間（ミリ秒）
    pub analysis_time_ms: u64,
}

/// 攻撃詳細
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackDetails {
    /// 攻撃パターン名
    pub pattern_names: Vec<String>,
    /// 深刻度
    pub severity: Severity,
    /// 説明
    pub description: String,
    /// 影響範囲
    pub impact: String,
    /// CVE ID（該当する場合）
    pub cve_ids: Vec<String>,
}

/// 送信元情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// IPアドレス
    pub ip_address: Option<IpAddr>,
    /// ユーザーID
    pub user_id: Option<String>,
    /// セッションID
    pub session_id: Option<String>,
    /// User-Agent
    pub user_agent: Option<String>,
    /// リファラー
    pub referer: Option<String>,
    /// 地理的位置
    pub geo_location: Option<GeoLocation>,
}

/// 地理的位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoLocation {
    /// 国
    pub country: String,
    /// 都市
    pub city: Option<String>,
    /// 緯度
    pub latitude: f64,
    /// 経度
    pub longitude: f64,
}

/// 推奨アクション
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendedAction {
    /// 監視のみ
    Monitor,
    /// 警告
    Warn,
    /// ブロック
    Block,
    /// セッション無効化
    InvalidateSession,
    /// IPアドレスをブロックリストに追加
    BlocklistIp,
    /// 緊急対応
    EmergencyResponse,
}

/// リクエストデータ
#[derive(Debug, Clone)]
pub struct RequestData {
    /// リクエストID
    pub request_id: String,
    /// HTTPメソッド
    pub method: String,
    /// パス
    pub path: String,
    /// クエリパラメータ
    pub query_params: HashMap<String, String>,
    /// ヘッダー
    pub headers: HashMap<String, String>,
    /// ボディ
    pub body: Option<Vec<u8>>,
    /// 送信元IP
    pub source_ip: Option<IpAddr>,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
}
