//! IDS Configuration
//!
//! 侵入検知システムの設定と統計

use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::types::DetectionType;

/// IDS設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IDSConfig {
    /// 有効化フラグ
    pub enabled: bool,
    /// シグネチャベース検知の有効化
    pub signature_based_enabled: bool,
    /// 振る舞いベース検知の有効化
    pub behavioral_based_enabled: bool,
    /// ネットワーク検知の有効化
    pub network_based_enabled: bool,
    /// 最小信頼度閾値（0.0-1.0）
    pub min_confidence_threshold: f64,
    /// アラート送信の有効化
    pub alert_enabled: bool,
    /// 自動ブロックの有効化
    pub auto_block_enabled: bool,
    /// セッションタイムアウト
    pub session_timeout: Duration,
}

impl Default for IDSConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            signature_based_enabled: true,
            behavioral_based_enabled: true,
            network_based_enabled: true,
            min_confidence_threshold: 0.7,
            alert_enabled: true,
            auto_block_enabled: false,
            session_timeout: Duration::from_secs(3600),
        }
    }
}

/// IDS統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IDSStats {
    /// 検知総数
    pub total_detections: u64,
    /// ブロック総数
    pub total_blocks: u64,
    /// 誤検知数
    pub false_positives: u64,
    /// タイプ別検知数
    pub detections_by_type: std::collections::HashMap<DetectionType, u64>,
    /// 平均信頼度
    pub average_confidence: f64,
    /// 最終検知時刻
    pub last_detection: Option<chrono::DateTime<chrono::Utc>>,
    /// 最終更新時刻
    pub last_updated: chrono::DateTime<chrono::Utc>,
    /// アクティブな脅威数
    pub active_threats: u64,
    /// アラート総数
    pub total_alerts: u64,
    /// 検知率（0.0-1.0）
    pub detection_rate: f64,
}

impl Default for IDSStats {
    fn default() -> Self {
        Self {
            total_detections: 0,
            total_blocks: 0,
            false_positives: 0,
            detections_by_type: std::collections::HashMap::new(),
            average_confidence: 0.0,
            last_detection: None,
            last_updated: chrono::Utc::now(),
            active_threats: 0,
            total_alerts: 0,
            detection_rate: 0.0,
        }
    }
}
