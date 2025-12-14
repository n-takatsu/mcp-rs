//! IDS Detector
//!
//! 侵入検知システムのメイン実装

use std::sync::Arc;

use chrono::Utc;
use log::{debug, warn};
use tokio::sync::RwLock;

use crate::error::McpError;

use super::alerts::{Alert, AlertLevel, AlertManager};
use super::behavioral::BehavioralDetector;
use super::config::{IDSConfig, IDSStats};
use super::network::NetworkMonitor;
use super::signature::SignatureDetector;
use super::types::{
    AttackDetails, DetectionResult, DetectionType, RecommendedAction, RequestData, Severity,
    SourceInfo,
};

/// 侵入検知システム
pub struct IntrusionDetectionSystem {
    config: IDSConfig,
    signature_detector: Arc<SignatureDetector>,
    behavioral_detector: Arc<BehavioralDetector>,
    network_monitor: Arc<NetworkMonitor>,
    alert_manager: Arc<AlertManager>,
    threat_engine: Option<Arc<dyn std::any::Any + Send + Sync>>,
    stats: Arc<RwLock<IDSStats>>,
}

impl IntrusionDetectionSystem {
    /// 新規作成
    pub async fn new(config: IDSConfig) -> Result<Self, McpError> {
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
        if self.config.signature_based_enabled {
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
        if self.config.behavioral_based_enabled {
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
        if self.config.network_based_enabled {
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
        stats.total_blocks += 1;

        Ok(())
    }

    /// 統計情報を取得
    pub async fn get_stats(&self) -> IDSStats {
        self.stats.read().await.clone()
    }

    /// 統計を更新（内部メソッド）
    async fn update_stats(&self, detection_type: DetectionType, _analysis_time_ms: u64) {
        let mut stats = self.stats.write().await;
        stats.total_detections += 1;
        *stats.detections_by_type.entry(detection_type).or_insert(0) += 1;
        stats.last_detection = Some(Utc::now());
        stats.last_updated = Utc::now();
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
