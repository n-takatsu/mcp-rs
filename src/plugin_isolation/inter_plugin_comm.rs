//! プラグイン間通信制御
//!
//! プラグイン間の安全な通信を制御し、不正なデータ交換を防止

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::McpError;

/// プラグイン間通信コントローラー
#[derive(Debug)]
pub struct InterPluginCommunicationController {
    /// 通信ルール
    rules: Arc<RwLock<HashMap<CommunicationRule, RuleStatus>>>,
    /// メッセージキュー
    message_queue: Arc<Mutex<MessageQueue>>,
    /// 通信履歴
    communication_history: Arc<Mutex<Vec<CommunicationEvent>>>,
    /// レート制限
    rate_limiters: Arc<RwLock<HashMap<Uuid, PluginRateLimiter>>>,
    /// 設定
    config: InterPluginCommConfig,
}

/// 通信ルール
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommunicationRule {
    /// 送信元プラグインID
    pub source_plugin: Uuid,
    /// 送信先プラグインID
    pub target_plugin: Uuid,
    /// 許可されるメッセージタイプ
    pub allowed_message_types: Vec<String>,
    /// 優先度
    pub priority: u32,
}

/// ルールステータス
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleStatus {
    /// 有効
    Active,
    /// 無効
    Disabled,
    /// 一時停止
    Suspended,
}

/// メッセージキュー
#[derive(Debug, Default)]
struct MessageQueue {
    /// キューに入ったメッセージ
    messages: Vec<QueuedMessage>,
    /// 最大キューサイズ
    max_size: usize,
}

/// キューに入ったメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessage {
    /// メッセージID
    pub message_id: Uuid,
    /// 送信元プラグイン
    pub from_plugin: Uuid,
    /// 送信先プラグイン
    pub to_plugin: Uuid,
    /// メッセージタイプ
    pub message_type: String,
    /// ペイロード
    pub payload: Vec<u8>,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 優先度
    pub priority: u32,
    /// 再試行回数
    pub retry_count: u32,
}

/// 通信イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationEvent {
    /// イベントID
    pub event_id: Uuid,
    /// 送信元プラグイン
    pub from_plugin: Uuid,
    /// 送信先プラグイン
    pub to_plugin: Uuid,
    /// イベントタイプ
    pub event_type: CommEventType,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 結果
    pub result: CommResult,
    /// エラー情報（ある場合）
    pub error: Option<String>,
}

/// 通信イベントタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommEventType {
    /// メッセージ送信
    MessageSent,
    /// メッセージ受信
    MessageReceived,
    /// メッセージ拒否
    MessageRejected,
    /// タイムアウト
    Timeout,
}

/// 通信結果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommResult {
    /// 成功
    Success,
    /// 失敗
    Failed,
    /// 保留中
    Pending,
}

/// プラグインレート制限
#[derive(Debug)]
struct PluginRateLimiter {
    /// 最大メッセージレート (メッセージ/秒)
    max_rate: u32,
    /// ウィンドウサイズ
    window_seconds: u64,
    /// 最近のメッセージタイムスタンプ
    recent_messages: Vec<chrono::DateTime<chrono::Utc>>,
}

/// プラグイン間通信設定
#[derive(Debug, Clone)]
pub struct InterPluginCommConfig {
    /// デフォルトレート制限 (メッセージ/秒)
    pub default_rate_limit: u32,
    /// 最大キューサイズ
    pub max_queue_size: usize,
    /// メッセージタイムアウト (秒)
    pub message_timeout_seconds: u64,
    /// 最大再試行回数
    pub max_retries: u32,
    /// 履歴保持期間 (秒)
    pub history_retention_seconds: u64,
}

impl Default for InterPluginCommConfig {
    fn default() -> Self {
        Self {
            default_rate_limit: 100,
            max_queue_size: 10000,
            message_timeout_seconds: 30,
            max_retries: 3,
            history_retention_seconds: 86400, // 24時間
        }
    }
}

impl InterPluginCommunicationController {
    /// 新しいコントローラーを作成
    pub async fn new(config: InterPluginCommConfig) -> Result<Self, McpError> {
        info!("Initializing inter-plugin communication controller");

        Ok(Self {
            rules: Arc::new(RwLock::new(HashMap::new())),
            message_queue: Arc::new(Mutex::new(MessageQueue {
                messages: Vec::new(),
                max_size: config.max_queue_size,
            })),
            communication_history: Arc::new(Mutex::new(Vec::new())),
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    /// 通信ルールを追加
    pub async fn add_rule(&self, rule: CommunicationRule) -> Result<(), McpError> {
        debug!(
            "Adding communication rule: {:?} -> {:?}",
            rule.source_plugin, rule.target_plugin
        );

        let mut rules = self.rules.write().await;
        rules.insert(rule, RuleStatus::Active);

        Ok(())
    }

    /// 通信ルールを削除
    pub async fn remove_rule(&self, rule: &CommunicationRule) -> Result<(), McpError> {
        debug!(
            "Removing communication rule: {:?} -> {:?}",
            rule.source_plugin, rule.target_plugin
        );

        let mut rules = self.rules.write().await;
        rules.remove(rule);

        Ok(())
    }

    /// メッセージを送信
    pub async fn send_message(
        &self,
        from_plugin: Uuid,
        to_plugin: Uuid,
        message_type: String,
        payload: Vec<u8>,
        priority: u32,
    ) -> Result<Uuid, McpError> {
        debug!("Sending message from {:?} to {:?}", from_plugin, to_plugin);

        // レート制限チェック
        self.check_rate_limit(from_plugin).await?;

        // 通信ルールチェック
        self.check_communication_allowed(from_plugin, to_plugin, &message_type)
            .await?;

        // メッセージをキューに追加
        let message_id = Uuid::new_v4();
        let message = QueuedMessage {
            message_id,
            from_plugin,
            to_plugin,
            message_type: message_type.clone(),
            payload,
            timestamp: chrono::Utc::now(),
            priority,
            retry_count: 0,
        };

        let mut queue = self.message_queue.lock().await;
        if queue.messages.len() >= queue.max_size {
            return Err(McpError::Plugin("Message queue is full".to_string()));
        }

        queue.messages.push(message);
        // 優先度でソート（高い優先度が先）
        queue.messages.sort_by(|a, b| b.priority.cmp(&a.priority));

        // 通信イベントを記録
        self.record_communication_event(CommunicationEvent {
            event_id: Uuid::new_v4(),
            from_plugin,
            to_plugin,
            event_type: CommEventType::MessageSent,
            timestamp: chrono::Utc::now(),
            result: CommResult::Pending,
            error: None,
        })
        .await?;

        info!("Message queued successfully: {}", message_id);
        Ok(message_id)
    }

    /// メッセージを受信
    pub async fn receive_message(
        &self,
        plugin_id: Uuid,
    ) -> Result<Option<QueuedMessage>, McpError> {
        debug!("Receiving message for plugin: {:?}", plugin_id);

        let mut queue = self.message_queue.lock().await;

        // 該当プラグイン宛のメッセージを検索
        if let Some(index) = queue
            .messages
            .iter()
            .position(|msg| msg.to_plugin == plugin_id)
        {
            let message = queue.messages.remove(index);

            // 通信イベントを記録
            self.record_communication_event(CommunicationEvent {
                event_id: Uuid::new_v4(),
                from_plugin: message.from_plugin,
                to_plugin: message.to_plugin,
                event_type: CommEventType::MessageReceived,
                timestamp: chrono::Utc::now(),
                result: CommResult::Success,
                error: None,
            })
            .await?;

            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// レート制限をチェック
    async fn check_rate_limit(&self, plugin_id: Uuid) -> Result<(), McpError> {
        let mut limiters = self.rate_limiters.write().await;

        let limiter = limiters
            .entry(plugin_id)
            .or_insert_with(|| PluginRateLimiter {
                max_rate: self.config.default_rate_limit,
                window_seconds: 1,
                recent_messages: Vec::new(),
            });

        let now = chrono::Utc::now();
        let window_start = now - chrono::Duration::seconds(limiter.window_seconds as i64);

        // 古いメッセージを削除
        limiter
            .recent_messages
            .retain(|&timestamp| timestamp > window_start);

        // レート制限チェック
        if limiter.recent_messages.len() >= limiter.max_rate as usize {
            warn!("Rate limit exceeded for plugin: {:?}", plugin_id);
            return Err(McpError::Plugin("Rate limit exceeded".to_string()));
        }

        // タイムスタンプを記録
        limiter.recent_messages.push(now);

        Ok(())
    }

    /// 通信が許可されているかチェック
    async fn check_communication_allowed(
        &self,
        from_plugin: Uuid,
        to_plugin: Uuid,
        message_type: &str,
    ) -> Result<(), McpError> {
        let rules = self.rules.read().await;

        // マッチするルールを検索
        for (rule, status) in rules.iter() {
            if rule.source_plugin == from_plugin && rule.target_plugin == to_plugin {
                if *status != RuleStatus::Active {
                    return Err(McpError::Plugin(
                        "Communication rule is not active".to_string(),
                    ));
                }

                if !rule.allowed_message_types.is_empty()
                    && !rule
                        .allowed_message_types
                        .contains(&message_type.to_string())
                {
                    return Err(McpError::Plugin(format!(
                        "Message type '{}' not allowed",
                        message_type
                    )));
                }

                return Ok(());
            }
        }

        // デフォルトでは通信を拒否
        warn!(
            "No communication rule found for {:?} -> {:?}",
            from_plugin, to_plugin
        );
        Err(McpError::Plugin(
            "Communication not allowed".to_string(),
        ))
    }

    /// 通信イベントを記録
    async fn record_communication_event(&self, event: CommunicationEvent) -> Result<(), McpError> {
        let mut history = self.communication_history.lock().await;
        history.push(event);

        // 古い履歴を削除
        let cutoff = chrono::Utc::now()
            - chrono::Duration::seconds(self.config.history_retention_seconds as i64);
        history.retain(|event| event.timestamp > cutoff);

        Ok(())
    }

    /// 通信履歴を取得
    pub async fn get_communication_history(
        &self,
        plugin_id: Option<Uuid>,
        limit: Option<usize>,
    ) -> Result<Vec<CommunicationEvent>, McpError> {
        let history = self.communication_history.lock().await;

        let mut filtered: Vec<CommunicationEvent> = if let Some(id) = plugin_id {
            history
                .iter()
                .filter(|event| event.from_plugin == id || event.to_plugin == id)
                .cloned()
                .collect()
        } else {
            history.clone()
        };

        if let Some(limit) = limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }

    /// 統計情報を取得
    pub async fn get_stats(&self) -> Result<InterPluginCommStats, McpError> {
        let queue = self.message_queue.lock().await;
        let history = self.communication_history.lock().await;
        let rules = self.rules.read().await;

        let successful_messages = history
            .iter()
            .filter(|event| event.result == CommResult::Success)
            .count();

        let failed_messages = history
            .iter()
            .filter(|event| event.result == CommResult::Failed)
            .count();

        Ok(InterPluginCommStats {
            total_rules: rules.len(),
            active_rules: rules
                .iter()
                .filter(|(_, status)| **status == RuleStatus::Active)
                .count(),
            queued_messages: queue.messages.len(),
            successful_messages,
            failed_messages,
            total_events: history.len(),
        })
    }

    /// シャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down inter-plugin communication controller");

        // キューをクリア
        let mut queue = self.message_queue.lock().await;
        queue.messages.clear();

        // ルールをクリア
        let mut rules = self.rules.write().await;
        rules.clear();

        Ok(())
    }
}

/// プラグイン間通信統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterPluginCommStats {
    /// 総ルール数
    pub total_rules: usize,
    /// アクティブルール数
    pub active_rules: usize,
    /// キューに入っているメッセージ数
    pub queued_messages: usize,
    /// 成功したメッセージ数
    pub successful_messages: usize,
    /// 失敗したメッセージ数
    pub failed_messages: usize,
    /// 総イベント数
    pub total_events: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inter_plugin_communication_controller_creation() {
        let config = InterPluginCommConfig::default();
        let controller = InterPluginCommunicationController::new(config).await;
        assert!(controller.is_ok());
    }

    #[tokio::test]
    async fn test_add_and_remove_rule() {
        let config = InterPluginCommConfig::default();
        let controller = InterPluginCommunicationController::new(config)
            .await
            .unwrap();

        let rule = CommunicationRule {
            source_plugin: Uuid::new_v4(),
            target_plugin: Uuid::new_v4(),
            allowed_message_types: vec!["test".to_string()],
            priority: 1,
        };

        // ルール追加
        controller.add_rule(rule.clone()).await.unwrap();

        // ルール削除
        controller.remove_rule(&rule).await.unwrap();
    }

    #[tokio::test]
    async fn test_send_and_receive_message() {
        let config = InterPluginCommConfig::default();
        let controller = InterPluginCommunicationController::new(config)
            .await
            .unwrap();

        let from_plugin = Uuid::new_v4();
        let to_plugin = Uuid::new_v4();

        // 通信ルールを追加
        let rule = CommunicationRule {
            source_plugin: from_plugin,
            target_plugin: to_plugin,
            allowed_message_types: vec!["test".to_string()],
            priority: 1,
        };
        controller.add_rule(rule).await.unwrap();

        // メッセージ送信
        let message_id = controller
            .send_message(from_plugin, to_plugin, "test".to_string(), vec![1, 2, 3], 1)
            .await
            .unwrap();

        assert_ne!(message_id, Uuid::nil());

        // メッセージ受信
        let received = controller.receive_message(to_plugin).await.unwrap();
        assert!(received.is_some());

        let message = received.unwrap();
        assert_eq!(message.from_plugin, from_plugin);
        assert_eq!(message.to_plugin, to_plugin);
        assert_eq!(message.payload, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let mut config = InterPluginCommConfig::default();
        config.default_rate_limit = 2; // 1秒に2メッセージまで

        let controller = InterPluginCommunicationController::new(config)
            .await
            .unwrap();

        let from_plugin = Uuid::new_v4();
        let to_plugin = Uuid::new_v4();

        // 通信ルールを追加
        let rule = CommunicationRule {
            source_plugin: from_plugin,
            target_plugin: to_plugin,
            allowed_message_types: vec!["test".to_string()],
            priority: 1,
        };
        controller.add_rule(rule).await.unwrap();

        // 2つのメッセージは成功するはず
        controller
            .send_message(from_plugin, to_plugin, "test".to_string(), vec![], 1)
            .await
            .unwrap();

        controller
            .send_message(from_plugin, to_plugin, "test".to_string(), vec![], 1)
            .await
            .unwrap();

        // 3つ目はレート制限でエラーになるはず
        let result = controller
            .send_message(from_plugin, to_plugin, "test".to_string(), vec![], 1)
            .await;

        assert!(result.is_err());
    }
}
