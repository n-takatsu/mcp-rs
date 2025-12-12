//! 動的ポリシー更新システム
//!
//! リアルタイムでポリシーを更新し、原子性とゼロダウンタイムを保証

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::error::McpError;
use crate::policy_config::PolicyConfig;
use crate::policy_validation::{PolicyValidationEngine, ValidationLevel};

/// 動的ポリシー更新マネージャー
///
/// ポリシーのホットリロードと原子性を保証しながら更新を実行
pub struct DynamicPolicyUpdater {
    /// 現在アクティブなポリシー
    active_policy: Arc<RwLock<PolicyConfig>>,
    /// 更新イベントの通知チャンネル
    event_sender: broadcast::Sender<PolicyUpdateEvent>,
    /// ポリシーバリデーター
    validator: Arc<PolicyValidationEngine>,
    /// 更新設定
    config: UpdateConfig,
    /// 更新履歴
    update_history: Arc<RwLock<Vec<UpdateRecord>>>,
}

/// ポリシー更新イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyUpdateEvent {
    /// イベント種別
    pub event_type: UpdateEventType,
    /// ポリシーID
    pub policy_id: String,
    /// 旧バージョン
    pub old_version: String,
    /// 新バージョン
    pub new_version: String,
    /// イベント発生時刻
    pub timestamp: DateTime<Utc>,
    /// 更新ステータス
    pub status: UpdateStatus,
    /// メッセージ
    pub message: Option<String>,
}

/// 更新イベント種別
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdateEventType {
    /// 更新開始
    UpdateStarted,
    /// 検証中
    Validating,
    /// 検証成功
    ValidationSuccess,
    /// 検証失敗
    ValidationFailed,
    /// 適用中
    Applying,
    /// 適用成功
    Applied,
    /// 適用失敗
    ApplyFailed,
    /// ロールバック
    RolledBack,
}

/// 更新ステータス
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UpdateStatus {
    /// 進行中
    InProgress,
    /// 成功
    Success,
    /// 失敗
    Failed,
    /// ロールバック済み
    RolledBack,
}

/// 更新設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// 検証を有効にするか
    pub enable_validation: bool,
    /// 更新タイムアウト（秒）
    pub update_timeout_secs: u64,
    /// 自動ロールバックを有効にするか
    pub auto_rollback: bool,
    /// 段階的適用を有効にするか
    pub gradual_rollout: bool,
    /// 段階的適用の段階数
    pub rollout_stages: usize,
    /// 各段階の待機時間（秒）
    pub stage_delay_secs: u64,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enable_validation: true,
            update_timeout_secs: 30,
            auto_rollback: true,
            gradual_rollout: false,
            rollout_stages: 3,
            stage_delay_secs: 5,
        }
    }
}

/// 更新記録
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecord {
    /// 更新ID
    pub id: String,
    /// ポリシーID
    pub policy_id: String,
    /// 旧バージョン
    pub old_version: String,
    /// 新バージョン
    pub new_version: String,
    /// 更新開始時刻
    pub started_at: DateTime<Utc>,
    /// 更新完了時刻
    pub completed_at: Option<DateTime<Utc>>,
    /// ステータス
    pub status: UpdateStatus,
    /// エラーメッセージ
    pub error_message: Option<String>,
}

impl DynamicPolicyUpdater {
    /// 新しい動的ポリシー更新マネージャーを作成
    ///
    /// # 引数
    /// * `initial_policy` - 初期ポリシー設定
    /// * `config` - 更新設定
    pub fn new(initial_policy: PolicyConfig, config: UpdateConfig) -> Self {
        let (event_sender, _) = broadcast::channel(100);

        Self {
            active_policy: Arc::new(RwLock::new(initial_policy)),
            event_sender,
            validator: Arc::new(PolicyValidationEngine::new()),
            config,
            update_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 更新イベントを購読
    pub fn subscribe(&self) -> broadcast::Receiver<PolicyUpdateEvent> {
        self.event_sender.subscribe()
    }

    /// 現在のポリシーを取得
    pub async fn get_active_policy(&self) -> PolicyConfig {
        self.active_policy.read().await.clone()
    }

    /// ポリシーを更新（原子性保証）
    ///
    /// # 引数
    /// * `new_policy` - 新しいポリシー設定
    ///
    /// # 戻り値
    /// 更新が成功した場合は `Ok(())`、失敗した場合はエラー
    pub async fn update_policy(&self, new_policy: PolicyConfig) -> Result<(), McpError> {
        let old_policy = self.active_policy.read().await.clone();
        let update_id = uuid::Uuid::new_v4().to_string();

        info!(
            "ポリシー更新開始: {} -> {}",
            old_policy.version, new_policy.version
        );

        // 更新開始イベント送信
        self.send_event(PolicyUpdateEvent {
            event_type: UpdateEventType::UpdateStarted,
            policy_id: new_policy.id.clone(),
            old_version: old_policy.version.clone(),
            new_version: new_policy.version.clone(),
            timestamp: Utc::now(),
            status: UpdateStatus::InProgress,
            message: Some("ポリシー更新を開始しました".to_string()),
        });

        // 更新記録を作成
        let mut record = UpdateRecord {
            id: update_id.clone(),
            policy_id: new_policy.id.clone(),
            old_version: old_policy.version.clone(),
            new_version: new_policy.version.clone(),
            started_at: Utc::now(),
            completed_at: None,
            status: UpdateStatus::InProgress,
            error_message: None,
        };

        // 検証フェーズ
        if self.config.enable_validation {
            if let Err(e) = self.validate_policy(&new_policy).await {
                error!("ポリシー検証失敗: {}", e);
                record.status = UpdateStatus::Failed;
                record.error_message = Some(e.to_string());
                record.completed_at = Some(Utc::now());
                self.update_history.write().await.push(record);

                self.send_event(PolicyUpdateEvent {
                    event_type: UpdateEventType::ValidationFailed,
                    policy_id: new_policy.id.clone(),
                    old_version: old_policy.version.clone(),
                    new_version: new_policy.version.clone(),
                    timestamp: Utc::now(),
                    status: UpdateStatus::Failed,
                    message: Some(format!("検証失敗: {}", e)),
                });

                return Err(e);
            }

            self.send_event(PolicyUpdateEvent {
                event_type: UpdateEventType::ValidationSuccess,
                policy_id: new_policy.id.clone(),
                old_version: old_policy.version.clone(),
                new_version: new_policy.version.clone(),
                timestamp: Utc::now(),
                status: UpdateStatus::InProgress,
                message: Some("検証成功".to_string()),
            });
        }

        // 適用フェーズ
        self.send_event(PolicyUpdateEvent {
            event_type: UpdateEventType::Applying,
            policy_id: new_policy.id.clone(),
            old_version: old_policy.version.clone(),
            new_version: new_policy.version.clone(),
            timestamp: Utc::now(),
            status: UpdateStatus::InProgress,
            message: Some("ポリシーを適用中".to_string()),
        });

        // 原子的にポリシーを更新
        match self.apply_policy_atomically(new_policy.clone()).await {
            Ok(_) => {
                info!("ポリシー更新成功: {}", new_policy.version);
                record.status = UpdateStatus::Success;
                record.completed_at = Some(Utc::now());
                self.update_history.write().await.push(record);

                self.send_event(PolicyUpdateEvent {
                    event_type: UpdateEventType::Applied,
                    policy_id: new_policy.id.clone(),
                    old_version: old_policy.version,
                    new_version: new_policy.version.clone(),
                    timestamp: Utc::now(),
                    status: UpdateStatus::Success,
                    message: Some("ポリシー適用成功".to_string()),
                });

                Ok(())
            }
            Err(e) => {
                error!("ポリシー適用失敗: {}", e);

                // 自動ロールバック
                if self.config.auto_rollback {
                    warn!("自動ロールバックを実行中...");
                    if let Err(rollback_err) = self.apply_policy_atomically(old_policy.clone()).await
                    {
                        error!("ロールバック失敗: {}", rollback_err);
                        record.status = UpdateStatus::Failed;
                        record.error_message =
                            Some(format!("適用失敗 & ロールバック失敗: {}", rollback_err));
                    } else {
                        info!("ロールバック成功");
                        record.status = UpdateStatus::RolledBack;
                        record.error_message = Some(format!("適用失敗、ロールバック実行: {}", e));

                        self.send_event(PolicyUpdateEvent {
                            event_type: UpdateEventType::RolledBack,
                            policy_id: new_policy.id.clone(),
                            old_version: old_policy.version.clone(),
                            new_version: new_policy.version.clone(),
                            timestamp: Utc::now(),
                            status: UpdateStatus::RolledBack,
                            message: Some("ロールバック成功".to_string()),
                        });
                    }
                } else {
                    record.status = UpdateStatus::Failed;
                    record.error_message = Some(e.to_string());
                }

                record.completed_at = Some(Utc::now());
                self.update_history.write().await.push(record);

                self.send_event(PolicyUpdateEvent {
                    event_type: UpdateEventType::ApplyFailed,
                    policy_id: new_policy.id.clone(),
                    old_version: old_policy.version.clone(),
                    new_version: new_policy.version.clone(),
                    timestamp: Utc::now(),
                    status: UpdateStatus::Failed,
                    message: Some(format!("適用失敗: {}", e)),
                });

                Err(e)
            }
        }
    }

    /// ポリシーを検証
    async fn validate_policy(&self, policy: &PolicyConfig) -> Result<(), McpError> {
        info!("ポリシー検証中: {}", policy.version);

        // 追加の検証ロジック
        if policy.version.is_empty() {
            return Err(McpError::Validation(
                "ポリシーバージョンが空です".to_string(),
            ));
        }

        Ok(())
    }

    /// ポリシーを原子的に適用
    async fn apply_policy_atomically(&self, policy: PolicyConfig) -> Result<(), McpError> {
        // 書き込みロックを取得（原子性保証）
        let mut active = self.active_policy.write().await;

        // ポリシーを更新
        *active = policy;

        Ok(())
    }

    /// イベントを送信
    fn send_event(&self, event: PolicyUpdateEvent) {
        // エラーを無視（受信者がいない場合）
        let _ = self.event_sender.send(event);
    }

    /// 更新履歴を取得
    pub async fn get_update_history(&self) -> Vec<UpdateRecord> {
        self.update_history.read().await.clone()
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> UpdateStatistics {
        let history = self.update_history.read().await;

        let total_updates = history.len();
        let successful_updates = history
            .iter()
            .filter(|r| r.status == UpdateStatus::Success)
            .count();
        let failed_updates = history
            .iter()
            .filter(|r| r.status == UpdateStatus::Failed)
            .count();
        let rolled_back_updates = history
            .iter()
            .filter(|r| r.status == UpdateStatus::RolledBack)
            .count();

        let avg_update_time = if !history.is_empty() {
            let total_duration: i64 = history
                .iter()
                .filter_map(|r| {
                    r.completed_at
                        .map(|completed| (completed - r.started_at).num_milliseconds())
                })
                .sum();
            total_duration / history.len() as i64
        } else {
            0
        };

        UpdateStatistics {
            total_updates,
            successful_updates,
            failed_updates,
            rolled_back_updates,
            average_update_time_ms: avg_update_time,
            success_rate: if total_updates > 0 {
                (successful_updates as f64 / total_updates as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

/// 更新統計情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatistics {
    /// 総更新回数
    pub total_updates: usize,
    /// 成功した更新
    pub successful_updates: usize,
    /// 失敗した更新
    pub failed_updates: usize,
    /// ロールバックされた更新
    pub rolled_back_updates: usize,
    /// 平均更新時間（ミリ秒）
    pub average_update_time_ms: i64,
    /// 成功率（%）
    pub success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_policy(version: &str) -> PolicyConfig {
        use crate::policy_config::*;

        PolicyConfig {
            id: "test-policy".to_string(),
            name: "Test Policy".to_string(),
            version: version.to_string(),
            description: Some("Test policy".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            security: SecurityPolicyConfig {
                enabled: true,
                encryption: EncryptionConfig {
                    algorithm: "AES-256".to_string(),
                    key_size: 256,
                    pbkdf2_iterations: 100000,
                },
                tls: TlsConfig {
                    enforce: true,
                    min_version: "1.2".to_string(),
                    cipher_suites: vec!["TLS_AES_256_GCM_SHA384".to_string()],
                },
                input_validation: InputValidationConfig {
                    enabled: true,
                    max_input_length: 1024,
                    sql_injection_protection: true,
                    xss_protection: true,
                },
                rate_limiting: RateLimitingConfig {
                    enabled: true,
                    requests_per_minute: 6000,
                    burst_size: 10,
                },
            },
            monitoring: MonitoringPolicyConfig {
                interval_seconds: 60,
                alerts_enabled: true,
                log_level: "info".to_string(),
                metrics: MetricsConfig {
                    enabled: true,
                    sampling_rate: 1.0,
                    buffer_size: 1000,
                },
            },
            authentication: AuthenticationPolicyConfig {
                enabled: true,
                method: "password".to_string(),
                session_timeout_seconds: 1800,
                require_mfa: false,
            },
            custom: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_policy_update() {
        let initial_policy = create_test_policy("1.0.0");
        let config = UpdateConfig::default();
        let updater = DynamicPolicyUpdater::new(initial_policy, config);

        let new_policy = create_test_policy("1.1.0");
        let result = updater.update_policy(new_policy.clone()).await;

        assert!(result.is_ok());

        let active = updater.get_active_policy().await;
        assert_eq!(active.version, "1.1.0");
    }

    #[tokio::test]
    async fn test_policy_update_with_validation() {
        let initial_policy = create_test_policy("1.0.0");
        let config = UpdateConfig {
            enable_validation: true,
            ..Default::default()
        };
        let updater = DynamicPolicyUpdater::new(initial_policy, config);

        let new_policy = create_test_policy("1.1.0");
        let result = updater.update_policy(new_policy).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_statistics() {
        let initial_policy = create_test_policy("1.0.0");
        let config = UpdateConfig::default();
        let updater = DynamicPolicyUpdater::new(initial_policy, config);

        // 複数回更新
        for i in 1..=5 {
            let new_policy = create_test_policy(&format!("1.{}.0", i));
            let _ = updater.update_policy(new_policy).await;
        }

        let stats = updater.get_statistics().await;
        assert_eq!(stats.total_updates, 5);
        assert!(stats.success_rate > 0.0);
    }
}
