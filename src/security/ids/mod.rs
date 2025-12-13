//! Intrusion Detection System (IDS)
//!
//! 包括的な侵入検知システムで、高度な攻撃の早期検知を実現します。
//!
//! ## 主要機能
//!
//! - **シグネチャベース検知**: 既知の攻撃パターンのマッチング
//! - **振る舞いベース検知**: ベースライン学習と異常行動検出
//! - **ネットワーク監視**: リクエストパターン分析とDDoS検知
//! - **アラート管理**: 重要度別アラート生成と通知
//! - **リアルタイム分析**: 低レイテンシ検知（<1秒）
//!
//! ## 使用例
//!
//! ```rust,no_run
//! use mcp_rs::security::ids::{IntrusionDetectionSystem, IDSConfig, RequestData};
//! use std::collections::HashMap;
//! use chrono::Utc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let ids = IntrusionDetectionSystem::new(IDSConfig::default()).await?;
//!
//! let request_data = RequestData {
//!     request_id: "req-123".to_string(),
//!     method: "GET".to_string(),
//!     path: "/api/data".to_string(),
//!     query_params: HashMap::new(),
//!     headers: HashMap::new(),
//!     body: None,
//!     source_ip: Some("192.168.1.1".parse().unwrap()),
//!     timestamp: Utc::now(),
//! };
//!
//! let result = ids.analyze_request(&request_data).await?;
//! if result.is_intrusion {
//!     ids.generate_alert(result.into()).await?;
//! }
//! # Ok(())
//! # }
//! ```

pub mod alerts;
pub mod behavioral;
pub mod network;
pub mod signature;

#[cfg(feature = "ml-anomaly-detection")]
pub mod ml;

use crate::error::McpError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

pub use alerts::{Alert, AlertLevel, AlertManager};
pub use behavioral::{BehaviorBaseline, BehavioralDetector};
pub use network::{NetworkMonitor, TrafficPattern};
pub use signature::{AttackPattern, SignatureDetector};

/// 侵入検知システム
pub struct IntrusionDetectionSystem {
    /// 設定
    config: IDSConfig,
    /// シグネチャ検知エンジン
    signature_detector: Arc<SignatureDetector>,
    /// 振る舞い検知エンジン
    behavioral_detector: Arc<BehavioralDetector>,
    /// ネットワーク監視エンジン
    network_monitor: Arc<NetworkMonitor>,
    /// アラート管理システム
    alert_manager: Arc<AlertManager>,
    /// 脅威検知エンジン（既存システム統合）- オプション
    threat_engine: Option<Arc<dyn std::any::Any + Send + Sync>>,
    /// 検知統計
    stats: Arc<RwLock<IDSStats>>,
}

/// IDS設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IDSConfig {
    /// シグネチャ検知の有効化
    pub signature_detection: bool,
    /// 振る舞い検知の有効化
    pub behavioral_detection: bool,
    /// ネットワーク監視の有効化
    pub network_monitoring: bool,
    /// アラートしきい値
    pub alert_threshold: AlertLevel,
    /// 自動応答の有効化
    pub auto_response: bool,
    /// 検知タイムアウト（ミリ秒）
    pub detection_timeout_ms: u64,
    /// 最大並列分析数
    pub max_concurrent_analysis: usize,
    /// 学習モード（誤検知削減）
    pub learning_mode: bool,
}

impl Default for IDSConfig {
    fn default() -> Self {
        Self {
            signature_detection: true,
            behavioral_detection: true,
            network_monitoring: true,
            alert_threshold: AlertLevel::Medium,
            auto_response: false,
            detection_timeout_ms: 1000,
            max_concurrent_analysis: 100,
            learning_mode: false,
        }
    }
}

/// IDS統計情報
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IDSStats {
    /// 総検知数
    pub total_detections: u64,
    /// 検知タイプ別統計
    pub detections_by_type: HashMap<DetectionType, u64>,
    /// アラート数
    pub total_alerts: u64,
    /// レベル別アラート数
    pub alerts_by_level: HashMap<AlertLevel, u64>,
    /// 誤検知数
    pub false_positives: u64,
    /// 検知率
    pub detection_rate: f64,
    /// 平均検知時間（ミリ秒）
    pub avg_detection_time_ms: f64,
    /// 最終更新時刻
    pub last_update: Option<DateTime<Utc>>,
}

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

impl IntrusionDetectionSystem {
    /// 新しいIDSインスタンスを作成
    pub async fn new(config: IDSConfig) -> Result<Self, McpError> {
        info!("Initializing Intrusion Detection System");

        let signature_detector = Arc::new(SignatureDetector::new().await.map_err(|e| {
            McpError::SecurityFailure(format!("Signature detector init failed: {}", e))
        })?);

        let behavioral_detector = Arc::new(BehavioralDetector::new().await.map_err(|e| {
            McpError::SecurityFailure(format!("Behavioral detector init failed: {}", e))
        })?);

        let network_monitor = Arc::new(NetworkMonitor::new().await.map_err(|e| {
            McpError::SecurityFailure(format!("Network monitor init failed: {}", e))
        })?);

        let alert_manager =
            Arc::new(AlertManager::new().await.map_err(|e| {
                McpError::SecurityFailure(format!("Alert manager init failed: {}", e))
            })?);

        Ok(Self {
            config,
            signature_detector,
            behavioral_detector,
            network_monitor,
            alert_manager,
            threat_engine: None,
            stats: Arc::new(RwLock::new(IDSStats::default())),
        })
    }

    /// 脅威検知エンジンを統合
    pub fn with_threat_engine(mut self, engine: Arc<dyn std::any::Any + Send + Sync>) -> Self {
        self.threat_engine = Some(engine);
        self
    }

    /// リクエストを分析
    pub async fn analyze_request(
        &self,
        request: &RequestData,
    ) -> Result<DetectionResult, McpError> {
        let start_time = std::time::Instant::now();
        debug!("Analyzing request: {}", request.request_id);

        let mut is_intrusion = false;
        let mut max_confidence: f64 = 0.0;
        let mut detection_type = DetectionType::Other;
        let mut pattern_names = Vec::new();
        let mut max_severity = Severity::Low;

        // 1. シグネチャベース検知
        if self.config.signature_detection {
            if let Ok(sig_result) = self.signature_detector.detect(request).await {
                if sig_result.matched {
                    is_intrusion = true;
                    max_confidence = max_confidence.max(sig_result.confidence);
                    detection_type = sig_result.detection_type;
                    pattern_names.extend(sig_result.pattern_names);
                    max_severity = max_severity.max(sig_result.severity);
                }
            }
        }

        // 2. 振る舞いベース検知
        if self.config.behavioral_detection {
            if let Ok(behavior_result) = self.behavioral_detector.analyze(request).await {
                if behavior_result.is_anomalous {
                    is_intrusion = true;
                    // シグネチャ検知が優先されるため、シグネチャ未検知の場合のみ上書き
                    if detection_type == DetectionType::Other {
                        if behavior_result.anomaly_score > max_confidence {
                            max_confidence = behavior_result.anomaly_score;
                            detection_type = DetectionType::AnomalousBehavior;
                        }
                    } else {
                        max_confidence = max_confidence.max(behavior_result.anomaly_score);
                    }
                }
            }
        }

        // 3. ネットワーク監視
        if self.config.network_monitoring {
            if let Ok(network_result) = self.network_monitor.check_traffic(request).await {
                if network_result.is_suspicious {
                    is_intrusion = true;
                    max_confidence = max_confidence.max(network_result.risk_score);
                    // ネットワーク攻撃は他の検知がない場合のみ設定
                    // （シグネチャや振る舞い検知が優先）
                }
            }
        }

        let analysis_time_ms = start_time.elapsed().as_millis() as u64;

        // 統計更新
        self.update_stats(detection_type, analysis_time_ms).await;

        let result = DetectionResult {
            is_intrusion,
            confidence: max_confidence,
            detection_type,
            attack_details: AttackDetails {
                pattern_names,
                severity: max_severity,
                description: format!(
                    "Detected {:?} with confidence {:.2}",
                    detection_type, max_confidence
                ),
                impact: self.assess_impact(detection_type, max_severity),
                cve_ids: Vec::new(),
            },
            source_info: SourceInfo {
                ip_address: request.source_ip,
                user_id: request.headers.get("X-User-ID").cloned(),
                session_id: request.headers.get("X-Session-ID").cloned(),
                user_agent: request.headers.get("User-Agent").cloned(),
                referer: request.headers.get("Referer").cloned(),
                geo_location: None,
            },
            recommended_action: self.determine_action(max_confidence, max_severity),
            detected_at: Utc::now(),
            analysis_time_ms,
        };

        if is_intrusion {
            warn!(
                "Intrusion detected: type={:?}, confidence={:.2}, analysis_time={}ms",
                detection_type, max_confidence, analysis_time_ms
            );
        }

        Ok(result)
    }

    /// アラートを生成
    pub async fn generate_alert(&self, result: DetectionResult) -> Result<(), McpError> {
        let alert = Alert {
            id: uuid::Uuid::new_v4().to_string(),
            level: self.severity_to_alert_level(result.attack_details.severity),
            detection_type: result.detection_type,
            confidence: result.confidence,
            source_ip: result.source_info.ip_address,
            description: result.attack_details.description.clone(),
            recommended_action: result.recommended_action,
            created_at: result.detected_at,
        };

        self.alert_manager
            .send_alert(alert)
            .await
            .map_err(|e| McpError::SecurityFailure(format!("Failed to send alert: {}", e)))?;

        // アラート統計更新
        let mut stats = self.stats.write().await;
        stats.total_alerts += 1;
        *stats
            .alerts_by_level
            .entry(self.severity_to_alert_level(result.attack_details.severity))
            .or_insert(0) += 1;

        Ok(())
    }

    /// 統計情報を取得
    pub async fn get_stats(&self) -> IDSStats {
        self.stats.read().await.clone()
    }

    /// 統計を更新（内部メソッド）
    async fn update_stats(&self, detection_type: DetectionType, analysis_time_ms: u64) {
        let mut stats = self.stats.write().await;
        stats.total_detections += 1;
        *stats.detections_by_type.entry(detection_type).or_insert(0) += 1;

        // 移動平均で分析時間を更新
        let alpha = 0.1;
        stats.avg_detection_time_ms =
            alpha * (analysis_time_ms as f64) + (1.0 - alpha) * stats.avg_detection_time_ms;

        stats.last_update = Some(Utc::now());
    }

    /// 影響を評価（内部メソッド）
    fn assess_impact(&self, detection_type: DetectionType, severity: Severity) -> String {
        match (detection_type, severity) {
            (DetectionType::SqlInjection, Severity::Critical) => {
                "データベース全体が危険にさらされる可能性".to_string()
            }
            (DetectionType::DdosAttack, _) => "サービス停止の可能性".to_string(),
            (DetectionType::DataExfiltration, _) => "機密データ漏洩の可能性".to_string(),
            _ => format!("{:?} attack detected", detection_type),
        }
    }

    /// アクションを決定（内部メソッド）
    fn determine_action(&self, confidence: f64, severity: Severity) -> RecommendedAction {
        match (confidence, severity) {
            (c, Severity::Critical) if c >= 0.8 => RecommendedAction::EmergencyResponse,
            (c, Severity::High) if c >= 0.7 => RecommendedAction::BlocklistIp,
            (c, Severity::High) if c >= 0.5 => RecommendedAction::Block,
            (c, Severity::Medium) if c >= 0.6 => RecommendedAction::InvalidateSession,
            (c, _) if c >= 0.5 => RecommendedAction::Warn,
            _ => RecommendedAction::Monitor,
        }
    }

    /// 深刻度をアラートレベルに変換（内部メソッド）
    fn severity_to_alert_level(&self, severity: Severity) -> AlertLevel {
        match severity {
            Severity::Critical => AlertLevel::Critical,
            Severity::High => AlertLevel::High,
            Severity::Medium => AlertLevel::Medium,
            Severity::Low => AlertLevel::Low,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ids_initialization() {
        let config = IDSConfig::default();
        let ids = IntrusionDetectionSystem::new(config).await;
        assert!(ids.is_ok());
    }

    #[tokio::test]
    async fn test_detection_result_creation() {
        let result = DetectionResult {
            is_intrusion: true,
            confidence: 0.95,
            detection_type: DetectionType::SqlInjection,
            attack_details: AttackDetails {
                pattern_names: vec!["UNION attack".to_string()],
                severity: Severity::High,
                description: "SQL injection detected".to_string(),
                impact: "Database compromise".to_string(),
                cve_ids: vec![],
            },
            source_info: SourceInfo {
                ip_address: Some("192.168.1.100".parse().unwrap()),
                user_id: None,
                session_id: None,
                user_agent: None,
                referer: None,
                geo_location: None,
            },
            recommended_action: RecommendedAction::Block,
            detected_at: Utc::now(),
            analysis_time_ms: 50,
        };

        assert!(result.is_intrusion);
        assert_eq!(result.confidence, 0.95);
        assert_eq!(result.detection_type, DetectionType::SqlInjection);
    }
}
