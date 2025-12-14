//! Advanced Audit Log Analysis System
//!
//! 高度な監査ログ分析システム
//!
//! このモジュールは以下の機能を提供します：
//! - 権限昇格検知
//! - データ流出パターン分析
//! - セキュリティイベント相関分析
//! - 自動アラート生成

pub mod alert_manager;
pub mod correlation_engine;
pub mod exfiltration_detector;
pub mod privilege_detector;
pub mod types;

pub use alert_manager::AlertManager;
pub use correlation_engine::CorrelationEngine;
pub use exfiltration_detector::ExfiltrationDetector;
pub use privilege_detector::PrivilegeDetector;
pub use types::*;

use crate::error::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 高度な監査ログ分析エンジン
pub struct AuditAnalysisEngine {
    /// 権限昇格検知
    privilege_detector: Arc<PrivilegeDetector>,
    /// データ流出検知
    exfiltration_detector: Arc<ExfiltrationDetector>,
    /// 相関分析エンジン
    correlation_engine: Arc<CorrelationEngine>,
    /// アラート管理
    alert_manager: Arc<RwLock<AlertManager>>,
}

impl AuditAnalysisEngine {
    /// 新しい分析エンジンを作成
    pub fn new() -> Self {
        Self {
            privilege_detector: Arc::new(PrivilegeDetector::new()),
            exfiltration_detector: Arc::new(ExfiltrationDetector::new()),
            correlation_engine: Arc::new(CorrelationEngine::new()),
            alert_manager: Arc::new(RwLock::new(AlertManager::new())),
        }
    }

    /// 監査ログエントリを分析
    pub async fn analyze_log(&self, entry: AuditLogEntry) -> Result<AnalysisResult> {
        let mut result = AnalysisResult::default();

        // 権限昇格検知
        if let Some(privilege_event) = self.privilege_detector.detect(&entry).await? {
            result.privilege_events.push(privilege_event);
        }

        // データ流出検知
        if let Some(exfiltration_event) = self.exfiltration_detector.detect(&entry).await? {
            result.exfiltration_events.push(exfiltration_event);
        }

        // 相関分析
        let correlated_events = self.correlation_engine.analyze(&entry).await?;
        result.correlated_events.extend(correlated_events);

        // アラート生成
        if result.should_generate_alert() {
            let alert = self.generate_alert(&result).await?;
            let mut alert_manager = self.alert_manager.write().await;
            alert_manager.add_alert(alert).await?;
            result.alert_generated = true;
        }

        Ok(result)
    }

    /// アラートを生成
    pub async fn generate_alert(&self, analysis_result: &AnalysisResult) -> Result<Alert> {
        let severity = analysis_result.calculate_severity();
        let description = analysis_result.generate_description();

        Ok(Alert {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            severity,
            title: "セキュリティ異常検出".to_string(),
            description,
            source: "AuditAnalysisEngine".to_string(),
            affected_resources: analysis_result.get_affected_resources(),
            recommended_actions: analysis_result.get_recommended_actions(),
            status: AlertStatus::New,
        })
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> AnalysisStatistics {
        let alert_manager = self.alert_manager.read().await;

        AnalysisStatistics {
            total_logs_analyzed: self.privilege_detector.get_analyzed_count().await,
            privilege_escalations_detected: self.privilege_detector.get_detection_count().await,
            exfiltrations_detected: self.exfiltration_detector.get_detection_count().await,
            total_alerts: alert_manager.get_alert_count().await,
            high_severity_alerts: alert_manager.get_high_severity_count().await,
        }
    }
}

impl Default for AuditAnalysisEngine {
    fn default() -> Self {
        Self::new()
    }
}
