//! Threat Response and Auto-Action
//!
//! 脅威に対する自動対応アクション

use crate::error::McpError;
use crate::threat_intelligence::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// 脅威対応マネージャー
pub struct ThreatResponseManager {
    /// 対応設定
    config: Arc<RwLock<ResponseConfig>>,
    /// アクション履歴
    action_history: Arc<RwLock<Vec<ResponseAction>>>,
    /// ブロックリスト
    block_list: Arc<RwLock<HashMap<String, BlockEntry>>>,
    /// 統計情報
    stats: Arc<RwLock<ResponseStats>>,
}

/// 対応設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseConfig {
    /// 自動ブロック有効化
    pub auto_block_enabled: bool,
    /// 自動アラート有効化
    pub auto_alert_enabled: bool,
    /// インシデント記録有効化
    pub incident_logging_enabled: bool,
    /// 脅威レベル別アクション
    pub severity_actions: HashMap<String, Vec<ActionType>>,
    /// ブロック期間（秒）
    pub default_block_duration_secs: u64,
    /// 最大ブロックエントリ数
    pub max_block_entries: usize,
}

impl Default for ResponseConfig {
    fn default() -> Self {
        let mut severity_actions = HashMap::new();

        // Critical: 即座にブロック + アラート + インシデント記録
        severity_actions.insert(
            "Critical".to_string(),
            vec![
                ActionType::Block,
                ActionType::Alert,
                ActionType::LogIncident,
            ],
        );

        // High: ブロック + アラート
        severity_actions.insert(
            "High".to_string(),
            vec![ActionType::Block, ActionType::Alert],
        );

        // Medium: アラート + ログ
        severity_actions.insert(
            "Medium".to_string(),
            vec![ActionType::Alert, ActionType::LogIncident],
        );

        // Low: ログのみ
        severity_actions.insert("Low".to_string(), vec![ActionType::LogIncident]);

        Self {
            auto_block_enabled: true,
            auto_alert_enabled: true,
            incident_logging_enabled: true,
            severity_actions,
            default_block_duration_secs: 3600, // 1時間
            max_block_entries: 10000,
        }
    }
}

/// アクションタイプ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ActionType {
    /// ブロック
    Block,
    /// アラート生成
    Alert,
    /// インシデント記録
    LogIncident,
    /// 通知
    Notify,
    /// カスタムアクション
    Custom(String),
}

/// 対応アクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseAction {
    /// アクションID
    pub id: Uuid,
    /// アクションタイプ
    pub action_type: ActionType,
    /// 対象指標
    pub indicator: ThreatIndicator,
    /// 脅威評価
    pub assessment: ThreatAssessment,
    /// 実行時刻
    pub executed_at: DateTime<Utc>,
    /// 実行結果
    pub result: ActionResult,
    /// メタデータ
    pub metadata: HashMap<String, String>,
}

/// アクション結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionResult {
    /// 成功
    Success {
        /// メッセージ
        message: String,
    },
    /// 失敗
    Failure {
        /// エラーメッセージ
        error: String,
    },
    /// 部分的成功
    Partial {
        /// メッセージ
        message: String,
        /// 警告
        warnings: Vec<String>,
    },
}

/// ブロックエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockEntry {
    /// 指標値
    pub indicator_value: String,
    /// 指標タイプ
    pub indicator_type: IndicatorType,
    /// ブロック開始時刻
    pub blocked_at: DateTime<Utc>,
    /// ブロック期限
    pub expires_at: DateTime<Utc>,
    /// 理由
    pub reason: String,
    /// 脅威レベル
    pub severity: SeverityLevel,
}

/// 対応統計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResponseStats {
    /// 総アクション数
    pub total_actions: u64,
    /// ブロック数
    pub blocks_executed: u64,
    /// アラート数
    pub alerts_generated: u64,
    /// インシデント記録数
    pub incidents_logged: u64,
    /// アクティブブロック数
    pub active_blocks: usize,
    /// アクションタイプ別カウント
    pub action_type_counts: HashMap<String, u64>,
}

impl ThreatResponseManager {
    /// 新しい脅威対応マネージャーを作成
    pub fn new(config: ResponseConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            action_history: Arc::new(RwLock::new(Vec::new())),
            block_list: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(ResponseStats::default())),
        }
    }

    /// 脅威に対して対応アクションを実行
    pub async fn respond_to_threat(
        &self,
        assessment: ThreatAssessment,
    ) -> Result<Vec<ResponseAction>, McpError> {
        let config = self.config.read().await;

        // 深刻度に基づいてアクションを決定
        let severity_key = format!("{:?}", assessment.threat_level);
        let actions = config
            .severity_actions
            .get(&severity_key)
            .cloned()
            .unwrap_or_default();

        drop(config);

        let mut executed_actions = Vec::new();

        for action_type in actions {
            match self
                .execute_action(action_type.clone(), assessment.clone())
                .await
            {
                Ok(action) => {
                    executed_actions.push(action);
                }
                Err(e) => {
                    error!("Failed to execute action {:?}: {}", action_type, e);
                }
            }
        }

        Ok(executed_actions)
    }

    /// 個別アクションを実行
    async fn execute_action(
        &self,
        action_type: ActionType,
        assessment: ThreatAssessment,
    ) -> Result<ResponseAction, McpError> {
        let config = self.config.read().await;
        let indicator = assessment.indicator.clone();

        let result = match action_type {
            ActionType::Block if config.auto_block_enabled => {
                self.execute_block(&indicator, &assessment).await?
            }
            ActionType::Alert if config.auto_alert_enabled => {
                self.execute_alert(&indicator, &assessment).await?
            }
            ActionType::LogIncident if config.incident_logging_enabled => {
                self.execute_log_incident(&indicator, &assessment).await?
            }
            ActionType::Notify => self.execute_notify(&indicator, &assessment).await?,
            ActionType::Custom(ref name) => {
                self.execute_custom(name, &indicator, &assessment).await?
            }
            _ => ActionResult::Failure {
                error: "Action disabled in configuration".to_string(),
            },
        };

        let action = ResponseAction {
            id: Uuid::new_v4(),
            action_type: action_type.clone(),
            indicator,
            assessment,
            executed_at: Utc::now(),
            result,
            metadata: HashMap::new(),
        };

        // アクション履歴に追加
        let mut history = self.action_history.write().await;
        history.push(action.clone());

        // 統計更新
        self.update_stats(&action).await;

        Ok(action)
    }

    /// ブロックアクションを実行
    async fn execute_block(
        &self,
        indicator: &ThreatIndicator,
        assessment: &ThreatAssessment,
    ) -> Result<ActionResult, McpError> {
        let config = self.config.read().await;
        let mut block_list = self.block_list.write().await;

        // 最大エントリ数チェック
        if block_list.len() >= config.max_block_entries {
            return Ok(ActionResult::Failure {
                error: "Block list is full".to_string(),
            });
        }

        let block_entry = BlockEntry {
            indicator_value: indicator.value.clone(),
            indicator_type: indicator.indicator_type,
            blocked_at: Utc::now(),
            expires_at: Utc::now()
                + chrono::Duration::seconds(config.default_block_duration_secs as i64),
            reason: format!("Threat level: {:?}", assessment.threat_level),
            severity: assessment.threat_level.clone(),
        };

        block_list.insert(indicator.value.clone(), block_entry);

        info!(
            "Blocked {} (type: {:?}, severity: {:?})",
            indicator.value, indicator.indicator_type, assessment.threat_level
        );

        Ok(ActionResult::Success {
            message: format!("Successfully blocked {}", indicator.value),
        })
    }

    /// アラートアクションを実行
    async fn execute_alert(
        &self,
        indicator: &ThreatIndicator,
        assessment: &ThreatAssessment,
    ) -> Result<ActionResult, McpError> {
        warn!(
            "THREAT ALERT: {} (type: {:?}, severity: {:?}, confidence: {:.2})",
            indicator.value,
            indicator.indicator_type,
            assessment.threat_level,
            assessment.confidence_score
        );

        Ok(ActionResult::Success {
            message: format!("Alert generated for {}", indicator.value),
        })
    }

    /// インシデント記録アクションを実行
    async fn execute_log_incident(
        &self,
        indicator: &ThreatIndicator,
        assessment: &ThreatAssessment,
    ) -> Result<ActionResult, McpError> {
        info!(
            "INCIDENT LOGGED: {} (type: {:?}, severity: {:?}, threats: {})",
            indicator.value,
            indicator.indicator_type,
            assessment.threat_level,
            assessment.matched_threats.len()
        );

        Ok(ActionResult::Success {
            message: format!("Incident logged for {}", indicator.value),
        })
    }

    /// 通知アクションを実行
    async fn execute_notify(
        &self,
        indicator: &ThreatIndicator,
        assessment: &ThreatAssessment,
    ) -> Result<ActionResult, McpError> {
        debug!(
            "NOTIFICATION: {} (severity: {:?})",
            indicator.value, assessment.threat_level
        );

        Ok(ActionResult::Success {
            message: format!("Notification sent for {}", indicator.value),
        })
    }

    /// カスタムアクションを実行
    async fn execute_custom(
        &self,
        _name: &str,
        indicator: &ThreatIndicator,
        _assessment: &ThreatAssessment,
    ) -> Result<ActionResult, McpError> {
        Ok(ActionResult::Partial {
            message: format!("Custom action executed for {}", indicator.value),
            warnings: vec!["Custom action not fully implemented".to_string()],
        })
    }

    /// 統計を更新
    async fn update_stats(&self, action: &ResponseAction) {
        let mut stats = self.stats.write().await;
        stats.total_actions += 1;

        match action.action_type {
            ActionType::Block => stats.blocks_executed += 1,
            ActionType::Alert => stats.alerts_generated += 1,
            ActionType::LogIncident => stats.incidents_logged += 1,
            _ => {}
        }

        let action_key = format!("{:?}", action.action_type);
        *stats.action_type_counts.entry(action_key).or_insert(0) += 1;

        // アクティブブロック数を更新
        let block_list = self.block_list.read().await;
        stats.active_blocks = block_list.len();
    }

    /// ブロックされているかチェック
    pub async fn is_blocked(&self, indicator_value: &str) -> bool {
        let block_list = self.block_list.read().await;

        if let Some(entry) = block_list.get(indicator_value) {
            // 期限切れチェック
            if entry.expires_at > Utc::now() {
                return true;
            }
        }

        false
    }

    /// 期限切れブロックをクリーンアップ
    pub async fn cleanup_expired_blocks(&self) -> Result<usize, McpError> {
        let mut block_list = self.block_list.write().await;
        let now = Utc::now();

        let initial_count = block_list.len();
        block_list.retain(|_, entry| entry.expires_at > now);

        let removed_count = initial_count - block_list.len();

        if removed_count > 0 {
            info!("Cleaned up {} expired blocks", removed_count);
        }

        Ok(removed_count)
    }

    /// アクション履歴を取得
    pub async fn get_action_history(&self, limit: Option<usize>) -> Vec<ResponseAction> {
        let history = self.action_history.read().await;

        if let Some(limit) = limit {
            history.iter().rev().take(limit).cloned().collect()
        } else {
            history.iter().rev().cloned().collect()
        }
    }

    /// ブロックリストを取得
    pub async fn get_block_list(&self) -> HashMap<String, BlockEntry> {
        let block_list = self.block_list.read().await;
        block_list.clone()
    }

    /// 統計情報を取得
    pub async fn get_stats(&self) -> ResponseStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// 手動でブロック解除
    pub async fn unblock(&self, indicator_value: &str) -> Result<(), McpError> {
        let mut block_list = self.block_list.write().await;

        if block_list.remove(indicator_value).is_some() {
            info!("Manually unblocked {}", indicator_value);
            Ok(())
        } else {
            Err(McpError::InvalidInput(format!(
                "Indicator not found in block list: {}",
                indicator_value
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_auto_block() {
        let config = ResponseConfig::default();
        let manager = ThreatResponseManager::new(config);

        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: "192.0.2.100".to_string(),
            pattern: None,
            tags: vec![],
            context: None,
            first_seen: Utc::now(),
        };

        let assessment = ThreatAssessment {
            indicator: indicator.clone(),
            is_threat: true,
            threat_level: SeverityLevel::Critical,
            confidence_score: 0.95,
            matched_threats: vec![],
            assessed_at: Utc::now(),
            assessment_duration_ms: 100,
            context: Default::default(),
        };

        let actions = manager.respond_to_threat(assessment).await.unwrap();

        assert!(!actions.is_empty());
        assert!(manager.is_blocked("192.0.2.100").await);

        let stats = manager.get_stats().await;
        assert_eq!(stats.blocks_executed, 1);
        assert!(stats.alerts_generated >= 1);
    }

    #[tokio::test]
    async fn test_unblock() {
        let config = ResponseConfig::default();
        let manager = ThreatResponseManager::new(config);

        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: "198.51.100.50".to_string(),
            pattern: None,
            tags: vec![],
            context: None,
            first_seen: Utc::now(),
        };

        let assessment = ThreatAssessment {
            indicator: indicator.clone(),
            is_threat: true,
            threat_level: SeverityLevel::High,
            confidence_score: 0.9,
            matched_threats: vec![],
            assessed_at: Utc::now(),
            assessment_duration_ms: 100,
            context: Default::default(),
        };

        manager.respond_to_threat(assessment).await.unwrap();
        assert!(manager.is_blocked("198.51.100.50").await);

        manager.unblock("198.51.100.50").await.unwrap();
        assert!(!manager.is_blocked("198.51.100.50").await);
    }
}
