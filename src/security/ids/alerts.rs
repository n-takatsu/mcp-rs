//! Alert Management
//!
//! アラート生成、重要度分類、通知システムを管理します。

use super::{DetectionType, RecommendedAction};
use crate::error::McpError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// アラート管理システム
pub struct AlertManager {
    /// アラート履歴
    alert_history: Arc<RwLock<VecDeque<Alert>>>,
    /// アラート集約マップ
    alert_aggregation: Arc<RwLock<HashMap<String, AggregatedAlert>>>,
    /// 通知チャネル
    notification_channels: Arc<RwLock<Vec<NotificationChannel>>>,
    /// 設定
    config: AlertConfig,
}

/// アラート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// アラートID
    pub id: String,
    /// アラートレベル
    pub level: AlertLevel,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// 信頼度
    pub confidence: f64,
    /// 送信元IP
    pub source_ip: Option<IpAddr>,
    /// 説明
    pub description: String,
    /// 推奨アクション
    pub recommended_action: RecommendedAction,
    /// 作成時刻
    pub created_at: DateTime<Utc>,
}

/// アラートレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AlertLevel {
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
    /// 緊急
    Critical,
}

/// 集約されたアラート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedAlert {
    /// 集約キー
    pub key: String,
    /// アラートレベル
    pub level: AlertLevel,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// 発生回数
    pub count: u64,
    /// 最初の発生時刻
    pub first_occurrence: DateTime<Utc>,
    /// 最後の発生時刻
    pub last_occurrence: DateTime<Utc>,
    /// 影響を受けたIP
    pub affected_ips: Vec<IpAddr>,
}

/// 通知チャネル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationChannel {
    /// メール
    Email {
        /// 宛先メールアドレス
        recipients: Vec<String>,
        /// 最小レベル
        min_level: AlertLevel,
    },
    /// Slack
    Slack {
        /// WebhookURL
        webhook_url: String,
        /// 最小レベル
        min_level: AlertLevel,
    },
    /// ログ
    Log {
        /// 最小レベル
        min_level: AlertLevel,
    },
    /// カスタムWebhook
    CustomWebhook {
        /// WebhookURL
        url: String,
        /// 最小レベル
        min_level: AlertLevel,
    },
}

/// アラート設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// アラート履歴の最大保持数
    pub max_history_size: usize,
    /// アラート集約ウィンドウ（秒）
    pub aggregation_window_seconds: i64,
    /// 集約しきい値（この回数以上で集約アラートを送信）
    pub aggregation_threshold: u64,
    /// 通知レート制限（秒）
    pub notification_rate_limit_seconds: i64,
    /// デフォルト通知レベル
    pub default_notification_level: AlertLevel,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            max_history_size: 10000,
            aggregation_window_seconds: 300, // 5分
            aggregation_threshold: 10,
            notification_rate_limit_seconds: 60,
            default_notification_level: AlertLevel::Medium,
        }
    }
}

/// 通知結果
#[derive(Debug)]
pub struct NotificationResult {
    /// 成功フラグ
    pub success: bool,
    /// チャネル数
    pub channels_notified: usize,
    /// エラーメッセージ
    pub errors: Vec<String>,
}

impl AlertManager {
    /// 新しいアラート管理システムを作成
    pub async fn new() -> Result<Self, McpError> {
        Self::with_config(AlertConfig::default()).await
    }

    /// 設定付きでアラート管理システムを作成
    pub async fn with_config(config: AlertConfig) -> Result<Self, McpError> {
        info!("Initializing alert manager");

        Ok(Self {
            alert_history: Arc::new(RwLock::new(VecDeque::new())),
            alert_aggregation: Arc::new(RwLock::new(HashMap::new())),
            notification_channels: Arc::new(RwLock::new(vec![
                // デフォルトでログ通知を有効化
                NotificationChannel::Log {
                    min_level: AlertLevel::Low,
                },
            ])),
            config,
        })
    }

    /// アラートを送信
    pub async fn send_alert(&self, alert: Alert) -> Result<(), McpError> {
        info!(
            "Sending alert: level={:?}, type={:?}, confidence={:.2}",
            alert.level, alert.detection_type, alert.confidence
        );

        // アラート履歴に追加
        self.add_to_history(alert.clone()).await;

        // アラート集約をチェック
        let should_notify = self.check_aggregation(&alert).await;

        if should_notify {
            // 通知を送信
            let result = self.send_notifications(&alert).await?;

            if !result.success {
                warn!("Some notification channels failed: {:?}", result.errors);
            }
        }

        Ok(())
    }

    /// 通知チャネルを追加
    pub async fn add_notification_channel(&self, channel: NotificationChannel) {
        let mut channels = self.notification_channels.write().await;
        channels.push(channel);
        info!("Added notification channel");
    }

    /// アラート履歴を取得
    pub async fn get_alert_history(&self, limit: Option<usize>) -> Vec<Alert> {
        let history = self.alert_history.read().await;

        if let Some(limit) = limit {
            history.iter().rev().take(limit).cloned().collect()
        } else {
            history.iter().rev().cloned().collect()
        }
    }

    /// レベル別アラート統計を取得
    pub async fn get_alert_stats(&self) -> HashMap<AlertLevel, u64> {
        let history = self.alert_history.read().await;
        let mut stats = HashMap::new();

        for alert in history.iter() {
            *stats.entry(alert.level).or_insert(0) += 1;
        }

        stats
    }

    /// 集約されたアラートを取得
    pub async fn get_aggregated_alerts(&self) -> Vec<AggregatedAlert> {
        self.alert_aggregation
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// アラート履歴に追加
    async fn add_to_history(&self, alert: Alert) {
        let mut history = self.alert_history.write().await;
        history.push_back(alert);

        // 履歴サイズ制限
        while history.len() > self.config.max_history_size {
            history.pop_front();
        }
    }

    /// アラート集約をチェック
    async fn check_aggregation(&self, alert: &Alert) -> bool {
        let aggregation_key = format!(
            "{}:{:?}",
            alert
                .source_ip
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            alert.detection_type
        );

        let mut aggregation = self.alert_aggregation.write().await;

        let aggregated = aggregation
            .entry(aggregation_key.clone())
            .or_insert_with(|| AggregatedAlert {
                key: aggregation_key.clone(),
                level: alert.level,
                detection_type: alert.detection_type,
                count: 0,
                first_occurrence: alert.created_at,
                last_occurrence: alert.created_at,
                affected_ips: Vec::new(),
            });

        aggregated.count += 1;
        aggregated.last_occurrence = alert.created_at;

        if let Some(ip) = alert.source_ip {
            if !aggregated.affected_ips.contains(&ip) {
                aggregated.affected_ips.push(ip);
            }
        }

        // 集約しきい値を超えた場合のみ通知
        if aggregated.count >= self.config.aggregation_threshold {
            // カウンターをリセット
            aggregated.count = 0;
            true
        } else {
            // 単独でも Critical レベルは即座に通知
            alert.level == AlertLevel::Critical
        }
    }

    /// 通知を送信
    async fn send_notifications(&self, alert: &Alert) -> Result<NotificationResult, McpError> {
        let channels = self.notification_channels.read().await;
        let mut channels_notified = 0;
        let mut errors = Vec::new();

        for channel in channels.iter() {
            if self.should_notify(alert.level, channel) {
                match self.send_to_channel(alert, channel).await {
                    Ok(_) => channels_notified += 1,
                    Err(e) => errors.push(format!("Channel error: {}", e)),
                }
            }
        }

        Ok(NotificationResult {
            success: errors.is_empty(),
            channels_notified,
            errors,
        })
    }

    /// チャネルに通知すべきか判定
    fn should_notify(&self, alert_level: AlertLevel, channel: &NotificationChannel) -> bool {
        let min_level = match channel {
            NotificationChannel::Email { min_level, .. } => *min_level,
            NotificationChannel::Slack { min_level, .. } => *min_level,
            NotificationChannel::Log { min_level } => *min_level,
            NotificationChannel::CustomWebhook { min_level, .. } => *min_level,
        };

        alert_level >= min_level
    }

    /// チャネルに通知を送信
    async fn send_to_channel(
        &self,
        alert: &Alert,
        channel: &NotificationChannel,
    ) -> Result<(), McpError> {
        match channel {
            NotificationChannel::Email { recipients, .. } => {
                self.send_email_notification(alert, recipients).await
            }
            NotificationChannel::Slack { webhook_url, .. } => {
                self.send_slack_notification(alert, webhook_url).await
            }
            NotificationChannel::Log { .. } => self.send_log_notification(alert).await,
            NotificationChannel::CustomWebhook { url, .. } => {
                self.send_webhook_notification(alert, url).await
            }
        }
    }

    /// メール通知を送信
    async fn send_email_notification(
        &self,
        alert: &Alert,
        _recipients: &[String],
    ) -> Result<(), McpError> {
        // 実装例（実際にはlettre crateなどを使用）
        info!("Would send email notification for alert: {}", alert.id);
        // TODO: 実装
        Ok(())
    }

    /// Slack通知を送信
    async fn send_slack_notification(
        &self,
        alert: &Alert,
        webhook_url: &str,
    ) -> Result<(), McpError> {
        let payload = serde_json::json!({
            "text": format!("🚨 Security Alert: {:?}", alert.level),
            "attachments": [{
                "color": self.get_color_for_level(alert.level),
                "fields": [
                    {
                        "title": "Alert Level",
                        "value": format!("{:?}", alert.level),
                        "short": true
                    },
                    {
                        "title": "Detection Type",
                        "value": format!("{:?}", alert.detection_type),
                        "short": true
                    },
                    {
                        "title": "Confidence",
                        "value": format!("{:.1}%", alert.confidence * 100.0),
                        "short": true
                    },
                    {
                        "title": "Source IP",
                        "value": alert.source_ip.map(|ip| ip.to_string()).unwrap_or_else(|| "Unknown".to_string()),
                        "short": true
                    },
                    {
                        "title": "Description",
                        "value": alert.description.clone(),
                        "short": false
                    },
                    {
                        "title": "Recommended Action",
                        "value": format!("{:?}", alert.recommended_action),
                        "short": false
                    }
                ],
                "footer": "MCP-RS Intrusion Detection System",
                "ts": alert.created_at.timestamp()
            }]
        });

        // 実際のHTTPリクエスト（reqwestを使用）
        let client = reqwest::Client::new();
        client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| McpError::Config(format!("Slack notification failed: {}", e)))?;

        info!("Sent Slack notification for alert: {}", alert.id);
        Ok(())
    }

    /// ログ通知を送信
    async fn send_log_notification(&self, alert: &Alert) -> Result<(), McpError> {
        match alert.level {
            AlertLevel::Critical => {
                error!(
                    "🚨 CRITICAL ALERT: {:?} - {} (confidence: {:.1}%)",
                    alert.detection_type,
                    alert.description,
                    alert.confidence * 100.0
                );
            }
            AlertLevel::High => {
                error!(
                    "⚠️  HIGH ALERT: {:?} - {} (confidence: {:.1}%)",
                    alert.detection_type,
                    alert.description,
                    alert.confidence * 100.0
                );
            }
            AlertLevel::Medium => {
                warn!(
                    "⚠  MEDIUM ALERT: {:?} - {} (confidence: {:.1}%)",
                    alert.detection_type,
                    alert.description,
                    alert.confidence * 100.0
                );
            }
            AlertLevel::Low => {
                info!(
                    "ℹ️  LOW ALERT: {:?} - {} (confidence: {:.1}%)",
                    alert.detection_type,
                    alert.description,
                    alert.confidence * 100.0
                );
            }
        }

        Ok(())
    }

    /// Webhook通知を送信
    async fn send_webhook_notification(&self, alert: &Alert, url: &str) -> Result<(), McpError> {
        let payload = serde_json::to_value(alert)
            .map_err(|e| McpError::Config(format!("Failed to serialize alert: {}", e)))?;

        let client = reqwest::Client::new();
        client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| McpError::Config(format!("Webhook notification failed: {}", e)))?;

        info!("Sent webhook notification for alert: {}", alert.id);
        Ok(())
    }

    /// レベルに応じた色を取得（Slack用）
    fn get_color_for_level(&self, level: AlertLevel) -> &'static str {
        match level {
            AlertLevel::Critical => "danger",
            AlertLevel::High => "warning",
            AlertLevel::Medium => "#FFA500",
            AlertLevel::Low => "good",
        }
    }

    /// 定期的なクリーンアップ
    pub async fn cleanup_old_alerts(&self) {
        use chrono::Duration;

        let cutoff = Utc::now() - Duration::hours(24);

        let mut aggregation = self.alert_aggregation.write().await;
        aggregation.retain(|_, agg| agg.last_occurrence > cutoff);

        info!("Cleaned up old aggregated alerts");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alert_manager_initialization() {
        let manager = AlertManager::new().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_send_alert() {
        let manager = AlertManager::new().await.unwrap();

        let alert = Alert {
            id: uuid::Uuid::new_v4().to_string(),
            level: AlertLevel::High,
            detection_type: DetectionType::SqlInjection,
            confidence: 0.95,
            source_ip: Some("192.168.1.100".parse().unwrap()),
            description: "SQL injection detected".to_string(),
            recommended_action: RecommendedAction::Block,
            created_at: Utc::now(),
        };

        let result = manager.send_alert(alert).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_alert_history() {
        let manager = AlertManager::new().await.unwrap();

        for i in 0..5 {
            let alert = Alert {
                id: format!("alert-{}", i),
                level: AlertLevel::Medium,
                detection_type: DetectionType::XssAttack,
                confidence: 0.8,
                source_ip: Some("192.168.1.100".parse().unwrap()),
                description: format!("Test alert {}", i),
                recommended_action: RecommendedAction::Warn,
                created_at: Utc::now(),
            };

            let _ = manager.send_alert(alert).await;
        }

        let history = manager.get_alert_history(Some(3)).await;
        assert_eq!(history.len(), 3);
    }

    #[tokio::test]
    async fn test_alert_level_ordering() {
        assert!(AlertLevel::Critical > AlertLevel::High);
        assert!(AlertLevel::High > AlertLevel::Medium);
        assert!(AlertLevel::Medium > AlertLevel::Low);
    }

    #[tokio::test]
    async fn test_notification_channel_addition() {
        let manager = AlertManager::new().await.unwrap();

        let channel = NotificationChannel::Email {
            recipients: vec!["security@example.com".to_string()],
            min_level: AlertLevel::High,
        };

        manager.add_notification_channel(channel).await;

        let channels = manager.notification_channels.read().await;
        assert_eq!(channels.len(), 2); // デフォルトのLog + 追加したEmail
    }
}
