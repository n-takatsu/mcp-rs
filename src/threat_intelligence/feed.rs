//! Real-time Threat Intelligence Feed
//!
//! リアルタイムで脅威情報を配信するフィードシステム

use crate::error::McpError;
use crate::threat_intelligence::manager::ThreatIntelligenceManager;
use crate::threat_intelligence::types::*;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// リアルタイム脅威フィード
pub struct ThreatFeed {
    /// 脅威マネージャー
    threat_manager: Arc<ThreatIntelligenceManager>,
    /// サブスクリプション管理
    subscriptions: Arc<RwLock<HashMap<Uuid, Subscription>>>,
    /// ブロードキャストチャンネル
    broadcast_tx: broadcast::Sender<ThreatFeedEvent>,
    /// フィード設定
    config: Arc<RwLock<ThreatFeedConfig>>,
    /// フィード統計
    stats: Arc<RwLock<ThreatFeedStats>>,
    /// 自動更新タスクハンドル
    update_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

/// 脅威フィード設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatFeedConfig {
    /// フィード有効化
    pub enabled: bool,
    /// 最大サブスクリプション数
    pub max_subscriptions: usize,
    /// デフォルト更新間隔（秒）
    pub default_update_interval_secs: u64,
    /// バッチサイズ
    pub batch_size: usize,
    /// イベントバッファサイズ
    pub event_buffer_size: usize,
    /// 自動クリーンアップ間隔（秒）
    pub cleanup_interval_secs: u64,
}

impl Default for ThreatFeedConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_subscriptions: 1000,
            default_update_interval_secs: 60,
            batch_size: 100,
            event_buffer_size: 10000,
            cleanup_interval_secs: 300,
        }
    }
}

/// サブスクリプション情報
#[derive(Debug, Clone)]
pub struct Subscription {
    /// サブスクリプションID
    pub id: Uuid,
    /// サブスクライバー識別子
    pub subscriber_id: String,
    /// フィルター設定
    pub filters: ThreatFeedFilters,
    /// 作成時刻
    pub created_at: DateTime<Utc>,
    /// 最終更新時刻
    pub last_updated: DateTime<Utc>,
    /// アクティブ状態
    pub active: bool,
}

/// 脅威フィードフィルター
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatFeedFilters {
    /// 脅威タイプフィルター
    pub threat_types: Option<Vec<ThreatType>>,
    /// 最小深刻度
    pub min_severity: Option<SeverityLevel>,
    /// 指標タイプフィルター
    pub indicator_types: Option<Vec<IndicatorType>>,
    /// プロバイダーフィルター
    pub providers: Option<Vec<String>>,
    /// 最小信頼度スコア
    pub min_confidence: Option<f64>,
    /// タグフィルター
    pub tags: Option<Vec<String>>,
}

impl Default for ThreatFeedFilters {
    fn default() -> Self {
        Self {
            threat_types: None,
            min_severity: Some(SeverityLevel::Low),
            indicator_types: None,
            providers: None,
            min_confidence: Some(0.5),
            tags: None,
        }
    }
}

/// 脅威フィードイベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatFeedEvent {
    /// イベントID
    pub id: Uuid,
    /// イベントタイプ
    pub event_type: ThreatFeedEventType,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// ペイロード
    pub payload: ThreatFeedPayload,
}

/// 脅威フィードイベントタイプ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThreatFeedEventType {
    /// 新規脅威検出
    NewThreat,
    /// 脅威更新
    ThreatUpdated,
    /// 脅威削除
    ThreatRemoved,
    /// 一括更新
    BatchUpdate,
    /// システムイベント
    SystemEvent,
}

/// 脅威フィードペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ThreatFeedPayload {
    /// 単一脅威
    SingleThreat {
        /// 脅威インテリジェンス
        threat: Box<ThreatIntelligence>,
    },
    /// 脅威リスト
    ThreatList {
        /// 脅威のリスト
        threats: Vec<ThreatIntelligence>,
        /// 総数
        total_count: usize,
    },
    /// 脅威評価
    ThreatAssessment {
        /// 評価結果
        assessment: Box<ThreatAssessment>,
    },
    /// システムメッセージ
    SystemMessage {
        /// メッセージ
        message: String,
        /// レベル
        level: String,
    },
}

/// 脅威フィード統計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThreatFeedStats {
    /// 総イベント数
    pub total_events: u64,
    /// アクティブサブスクリプション数
    pub active_subscriptions: usize,
    /// 配信された脅威数
    pub threats_delivered: u64,
    /// 最終イベント時刻
    pub last_event_at: Option<DateTime<Utc>>,
    /// イベントタイプ別カウント
    pub event_type_counts: HashMap<String, u64>,
}

impl ThreatFeed {
    /// 新しい脅威フィードを作成
    pub fn new(threat_manager: Arc<ThreatIntelligenceManager>, config: ThreatFeedConfig) -> Self {
        let (broadcast_tx, _) = broadcast::channel(config.event_buffer_size);

        Self {
            threat_manager,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            config: Arc::new(RwLock::new(config)),
            stats: Arc::new(RwLock::new(ThreatFeedStats::default())),
            update_task: Arc::new(RwLock::new(None)),
        }
    }

    /// サブスクリプションを作成
    pub async fn subscribe(
        &self,
        subscriber_id: String,
        filters: ThreatFeedFilters,
    ) -> Result<Uuid, McpError> {
        let config = self.config.read().await;
        let mut subscriptions = self.subscriptions.write().await;

        // 最大サブスクリプション数チェック
        if subscriptions.len() >= config.max_subscriptions {
            return Err(McpError::InvalidInput(
                "Maximum subscriptions reached".to_string(),
            ));
        }

        let subscription = Subscription {
            id: Uuid::new_v4(),
            subscriber_id,
            filters,
            created_at: Utc::now(),
            last_updated: Utc::now(),
            active: true,
        };

        let subscription_id = subscription.id;
        subscriptions.insert(subscription_id, subscription);

        info!(
            "Created threat feed subscription: id={}, total={}",
            subscription_id,
            subscriptions.len()
        );

        // 統計更新
        let mut stats = self.stats.write().await;
        stats.active_subscriptions = subscriptions.len();

        Ok(subscription_id)
    }

    /// サブスクリプションを解除
    pub async fn unsubscribe(&self, subscription_id: Uuid) -> Result<(), McpError> {
        let mut subscriptions = self.subscriptions.write().await;

        if subscriptions.remove(&subscription_id).is_some() {
            info!("Removed threat feed subscription: id={}", subscription_id);

            // 統計更新
            let mut stats = self.stats.write().await;
            stats.active_subscriptions = subscriptions.len();

            Ok(())
        } else {
            Err(McpError::InvalidInput(format!(
                "Subscription not found: {}",
                subscription_id
            )))
        }
    }

    /// イベントリスナーを作成
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<ThreatFeedEvent> {
        self.broadcast_tx.subscribe()
    }

    /// 脅威イベントを発行
    pub async fn publish_threat(&self, threat: ThreatIntelligence) -> Result<(), McpError> {
        let event = ThreatFeedEvent {
            id: Uuid::new_v4(),
            event_type: ThreatFeedEventType::NewThreat,
            timestamp: Utc::now(),
            payload: ThreatFeedPayload::SingleThreat {
                threat: Box::new(threat),
            },
        };

        self.publish_event(event).await
    }

    /// 脅威評価イベントを発行
    pub async fn publish_assessment(&self, assessment: ThreatAssessment) -> Result<(), McpError> {
        let event = ThreatFeedEvent {
            id: Uuid::new_v4(),
            event_type: ThreatFeedEventType::NewThreat,
            timestamp: Utc::now(),
            payload: ThreatFeedPayload::ThreatAssessment {
                assessment: Box::new(assessment),
            },
        };

        self.publish_event(event).await
    }

    /// 脅威リストイベントを発行
    pub async fn publish_threat_list(
        &self,
        threats: Vec<ThreatIntelligence>,
    ) -> Result<(), McpError> {
        let total_count = threats.len();
        let event = ThreatFeedEvent {
            id: Uuid::new_v4(),
            event_type: ThreatFeedEventType::BatchUpdate,
            timestamp: Utc::now(),
            payload: ThreatFeedPayload::ThreatList {
                threats,
                total_count,
            },
        };

        self.publish_event(event).await
    }

    /// イベントを発行
    async fn publish_event(&self, event: ThreatFeedEvent) -> Result<(), McpError> {
        // フィルタリング適用（サブスクリプションに基づく）
        let filtered_event = self.apply_filters(&event).await?;

        // ブロードキャスト（受信者がいない場合もエラーにしない）
        let receiver_count = self.broadcast_tx.send(filtered_event.clone()).unwrap_or(0);

        debug!(
            "Published threat feed event: id={}, receivers={}",
            filtered_event.id, receiver_count
        );

        // 統計更新
        self.update_stats(&filtered_event).await;

        Ok(())
    }

    /// フィルター適用
    async fn apply_filters(&self, event: &ThreatFeedEvent) -> Result<ThreatFeedEvent, McpError> {
        // 現在の実装では全イベントを配信
        // 将来的にサブスクリプション単位のフィルタリングを実装可能
        Ok(event.clone())
    }

    /// 統計更新
    async fn update_stats(&self, event: &ThreatFeedEvent) {
        let mut stats = self.stats.write().await;
        stats.total_events += 1;
        stats.last_event_at = Some(event.timestamp);

        let event_type_key = format!("{:?}", event.event_type);
        *stats.event_type_counts.entry(event_type_key).or_insert(0) += 1;

        if matches!(
            event.event_type,
            ThreatFeedEventType::NewThreat | ThreatFeedEventType::BatchUpdate
        ) {
            stats.threats_delivered += 1;
        }
    }

    /// サブスクリプション情報を取得
    pub async fn get_subscription(&self, subscription_id: Uuid) -> Option<Subscription> {
        let subscriptions = self.subscriptions.read().await;
        subscriptions.get(&subscription_id).cloned()
    }

    /// 全サブスクリプションを取得
    pub async fn list_subscriptions(&self) -> Vec<Subscription> {
        let subscriptions = self.subscriptions.read().await;
        subscriptions.values().cloned().collect()
    }

    /// 統計情報を取得
    pub async fn get_stats(&self) -> ThreatFeedStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// 非アクティブなサブスクリプションをクリーンアップ
    pub async fn cleanup_inactive_subscriptions(
        &self,
        max_age_hours: i64,
    ) -> Result<usize, McpError> {
        let mut subscriptions = self.subscriptions.write().await;
        let cutoff_time = Utc::now() - Duration::hours(max_age_hours);

        let initial_count = subscriptions.len();
        // アクティブなサブスクリプションのみ保持（古いアクティブなサブスクリプションも削除）
        // 非アクティブなサブスクリプションは時間に関係なく削除
        subscriptions.retain(|_, sub| sub.active && sub.last_updated > cutoff_time);

        let removed_count = initial_count - subscriptions.len();

        if removed_count > 0 {
            info!("Cleaned up {} inactive subscriptions", removed_count);

            // 統計更新
            let mut stats = self.stats.write().await;
            stats.active_subscriptions = subscriptions.len();
        }

        Ok(removed_count)
    }

    /// フィルターを更新
    pub async fn update_filters(
        &self,
        subscription_id: Uuid,
        filters: ThreatFeedFilters,
    ) -> Result<(), McpError> {
        let mut subscriptions = self.subscriptions.write().await;

        if let Some(subscription) = subscriptions.get_mut(&subscription_id) {
            subscription.filters = filters;
            subscription.last_updated = Utc::now();
            info!("Updated filters for subscription: id={}", subscription_id);
            Ok(())
        } else {
            Err(McpError::InvalidInput(format!(
                "Subscription not found: {}",
                subscription_id
            )))
        }
    }

    /// サブスクリプションの状態を切り替え
    pub async fn toggle_subscription(
        &self,
        subscription_id: Uuid,
        active: bool,
    ) -> Result<(), McpError> {
        let mut subscriptions = self.subscriptions.write().await;

        if let Some(subscription) = subscriptions.get_mut(&subscription_id) {
            subscription.active = active;
            subscription.last_updated = Utc::now();
            info!(
                "Toggled subscription: id={}, active={}",
                subscription_id, active
            );
            Ok(())
        } else {
            Err(McpError::InvalidInput(format!(
                "Subscription not found: {}",
                subscription_id
            )))
        }
    }

    /// 自動更新を開始
    pub async fn start_auto_update(&self) -> Result<(), McpError> {
        let mut task_handle = self.update_task.write().await;

        // 既に起動している場合は何もしない
        if task_handle.is_some() {
            return Ok(());
        }

        let config = self.config.read().await;
        let update_interval = config.default_update_interval_secs;
        let cleanup_interval = config.cleanup_interval_secs;
        drop(config);

        // 自己参照のためにArcクローン
        let feed_clone = Arc::new(self.clone_shallow());

        let handle = tokio::spawn(async move {
            let mut update_ticker =
                tokio::time::interval(tokio::time::Duration::from_secs(update_interval));
            let mut cleanup_ticker =
                tokio::time::interval(tokio::time::Duration::from_secs(cleanup_interval));

            loop {
                tokio::select! {
                    _ = update_ticker.tick() => {
                        if let Err(e) = feed_clone.perform_update().await {
                            error!("Auto update failed: {}", e);
                        }
                    }
                    _ = cleanup_ticker.tick() => {
                        if let Err(e) = feed_clone.cleanup_inactive_subscriptions(24).await {
                            error!("Auto cleanup failed: {}", e);
                        }
                    }
                }
            }
        });

        *task_handle = Some(handle);
        info!("Started auto update task (interval: {}s)", update_interval);
        Ok(())
    }

    /// 自動更新を停止
    pub async fn stop_auto_update(&self) -> Result<(), McpError> {
        let mut task_handle = self.update_task.write().await;

        if let Some(handle) = task_handle.take() {
            handle.abort();
            info!("Stopped auto update task");
        }

        Ok(())
    }

    /// 更新処理を実行
    async fn perform_update(&self) -> Result<(), McpError> {
        debug!("Performing threat feed update...");

        // システムイベントを発行
        let event = ThreatFeedEvent {
            id: Uuid::new_v4(),
            event_type: ThreatFeedEventType::SystemEvent,
            timestamp: Utc::now(),
            payload: ThreatFeedPayload::SystemMessage {
                message: "Threat feed auto-update started".to_string(),
                level: "info".to_string(),
            },
        };

        self.publish_event(event).await?;

        Ok(())
    }

    /// シャローコピーを作成（自動更新タスク用）
    fn clone_shallow(&self) -> Self {
        Self {
            threat_manager: Arc::clone(&self.threat_manager),
            subscriptions: Arc::clone(&self.subscriptions),
            broadcast_tx: self.broadcast_tx.clone(),
            config: Arc::clone(&self.config),
            stats: Arc::clone(&self.stats),
            update_task: Arc::new(RwLock::new(None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscription_creation() {
        let manager = Arc::new(ThreatIntelligenceManager::new());
        let config = ThreatFeedConfig::default();
        let feed = ThreatFeed::new(manager, config);

        let subscription_id = feed
            .subscribe("test-subscriber".to_string(), ThreatFeedFilters::default())
            .await
            .unwrap();

        assert!(feed.get_subscription(subscription_id).await.is_some());
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let manager = Arc::new(ThreatIntelligenceManager::new());
        let config = ThreatFeedConfig::default();
        let feed = ThreatFeed::new(manager, config);

        let subscription_id = feed
            .subscribe("test-subscriber".to_string(), ThreatFeedFilters::default())
            .await
            .unwrap();

        feed.unsubscribe(subscription_id).await.unwrap();
        assert!(feed.get_subscription(subscription_id).await.is_none());
    }

    #[tokio::test]
    async fn test_event_publishing() {
        let manager = Arc::new(ThreatIntelligenceManager::new());
        let config = ThreatFeedConfig::default();
        let feed = ThreatFeed::new(manager, config);

        let threat = ThreatIntelligence {
            id: "test-threat".to_string(),
            threat_type: ThreatType::Malware,
            severity: SeverityLevel::High,
            indicators: vec![],
            source: ThreatSource {
                provider: "test-provider".to_string(),
                feed_name: "test-feed".to_string(),
                reliability: 0.9,
                last_updated: Utc::now(),
            },
            confidence_score: 0.95,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            expiration: None,
            metadata: ThreatMetadata::default(),
        };

        feed.publish_threat(threat).await.unwrap();

        let stats = feed.get_stats().await;
        assert_eq!(stats.total_events, 1);
        assert_eq!(stats.threats_delivered, 1);
    }

    #[tokio::test]
    async fn test_max_subscriptions() {
        let manager = Arc::new(ThreatIntelligenceManager::new());
        let config = ThreatFeedConfig {
            max_subscriptions: 2,
            ..Default::default()
        };
        let feed = ThreatFeed::new(manager, config);

        // 最大数までサブスクリプション作成
        feed.subscribe("subscriber1".to_string(), ThreatFeedFilters::default())
            .await
            .unwrap();
        feed.subscribe("subscriber2".to_string(), ThreatFeedFilters::default())
            .await
            .unwrap();

        // 最大数を超えるとエラー
        let result = feed
            .subscribe("subscriber3".to_string(), ThreatFeedFilters::default())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cleanup_inactive_subscriptions() {
        let manager = Arc::new(ThreatIntelligenceManager::new());
        let config = ThreatFeedConfig::default();
        let feed = ThreatFeed::new(manager, config);

        // アクティブなサブスクリプション作成
        let active_sub = feed
            .subscribe(
                "active-subscriber".to_string(),
                ThreatFeedFilters::default(),
            )
            .await
            .unwrap();

        // 非アクティブなサブスクリプション作成
        let inactive_sub = feed
            .subscribe(
                "inactive-subscriber".to_string(),
                ThreatFeedFilters::default(),
            )
            .await
            .unwrap();

        // 非アクティブ化
        feed.toggle_subscription(inactive_sub, false).await.unwrap();

        // クリーンアップ（max_age_hoursより新しいサブスクリプションでも非アクティブなら削除）
        // max_age_hours=100（十分に長い時間）を設定して、新しいサブスクリプションでもテスト
        let removed = feed.cleanup_inactive_subscriptions(100).await.unwrap();

        // 非アクティブなサブスクリプションが削除された
        assert_eq!(removed, 1);

        // アクティブなサブスクリプションは残っている
        assert!(feed.get_subscription(active_sub).await.is_some());
        assert!(feed.get_subscription(inactive_sub).await.is_none());
    }
}
