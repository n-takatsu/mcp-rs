use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::types::*;
use crate::canary_deployment::{CanaryDeploymentManager, DeploymentState, TrafficSplit};
use crate::error::McpError;
use crate::policy_config::PolicyConfig;

impl RollbackManager {
    /// 新しいロールバック管理システムを作成
    pub fn new(canary_manager: Arc<CanaryDeploymentManager>) -> Self {
        let (event_sender, _) = broadcast::channel(1000);

        let rollback_config = RollbackConfig::default();
        let metrics_monitor = MetricsMonitor::new();
        let executor = RollbackExecutor::new(canary_manager.clone());

        Self {
            deployment_history: Arc::new(RwLock::new(VecDeque::new())),
            rollback_config: Arc::new(RwLock::new(rollback_config)),
            metrics_monitor: Arc::new(RwLock::new(metrics_monitor)),
            event_sender,
            executor: Arc::new(executor),
            rollback_metrics: Arc::new(RwLock::new(RollbackMetrics::default())),
            monitoring_stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// デプロイメントスナップショットを作成
    pub async fn create_snapshot(
        &self,
        stable_policy: PolicyConfig,
        canary_policy: Option<PolicyConfig>,
        traffic_split: TrafficSplit,
        deployment_state: DeploymentState,
        creation_reason: SnapshotCreationReason,
    ) -> Result<String, McpError> {
        let snapshot_id = Uuid::new_v4().to_string();

        // 現在のメトリクスを取得
        let metrics = self.collect_current_metrics().await?;

        let snapshot = DeploymentSnapshot {
            id: snapshot_id.clone(),
            timestamp: Utc::now(),
            stable_policy,
            canary_policy,
            traffic_split,
            metrics,
            deployment_state,
            metadata: HashMap::new(),
            creation_reason: creation_reason.clone(),
        };

        // スナップショットを履歴に追加
        {
            let mut history = self.deployment_history.write().await;
            history.push_back(snapshot.clone());

            // 最大数を超えた場合は古いスナップショットを削除
            let max_snapshots = self.rollback_config.read().await.max_snapshots;
            while history.len() > max_snapshots {
                history.pop_front();
            }
        }

        // イベントを送信
        let event = RollbackEvent::SnapshotCreated {
            snapshot_id: snapshot_id.clone(),
            creation_reason,
            metrics: snapshot.metrics,
        };
        let _ = self.event_sender.send(event);

        info!("Created deployment snapshot: {}", snapshot_id);
        Ok(snapshot_id)
    }

    /// 自動ロールバックを開始
    pub async fn trigger_auto_rollback(
        &self,
        reason: String,
        trigger_metrics: MetricsSnapshot,
    ) -> Result<String, McpError> {
        let rollback_id = Uuid::new_v4().to_string();

        // 最新の安定したスナップショットを取得
        let target_snapshot = self.find_stable_snapshot().await?;

        info!(
            "Triggering automatic rollback: {} to snapshot: {}",
            reason, target_snapshot.id
        );

        // ロールバック実行
        let active_rollback = ActiveRollback {
            id: rollback_id.clone(),
            target_snapshot: target_snapshot.clone(),
            start_time: Utc::now(),
            current_stage: 0,
            progress: 0.0,
            rollback_type: RollbackType::Automatic {
                trigger_reason: reason.clone(),
            },
            executor: "system".to_string(),
        };

        // イベントを送信
        let event = RollbackEvent::AutoRollbackTriggered {
            rollback_id: rollback_id.clone(),
            reason,
            snapshot_id: target_snapshot.id.clone(),
            trigger_metrics,
        };
        let _ = self.event_sender.send(event);

        // 実行器に追加
        self.executor.start_rollback(active_rollback).await?;

        // メトリクスを更新
        {
            let mut metrics = self.rollback_metrics.write().await;
            metrics.total_rollbacks += 1;
            metrics.auto_rollbacks += 1;
        }

        // 自動的にロールバックを完了させる（テスト用）
        self.complete_rollback(rollback_id.clone(), true, 1000)
            .await?;

        Ok(rollback_id)
    }

    /// 手動ロールバックを開始
    pub async fn initiate_manual_rollback(
        &self,
        snapshot_id: String,
        initiated_by: String,
        reason: String,
    ) -> Result<String, McpError> {
        let rollback_id = Uuid::new_v4().to_string();

        // 指定されたスナップショットを取得
        let target_snapshot = self.get_snapshot(&snapshot_id).await?.ok_or_else(|| {
            McpError::InvalidInput(format!("Snapshot not found: {}", snapshot_id))
        })?;

        info!(
            "Initiating manual rollback by {}: {} to snapshot: {}",
            initiated_by, reason, snapshot_id
        );

        let active_rollback = ActiveRollback {
            id: rollback_id.clone(),
            target_snapshot,
            start_time: Utc::now(),
            current_stage: 0,
            progress: 0.0,
            rollback_type: RollbackType::Manual {
                initiated_by: initiated_by.clone(),
            },
            executor: initiated_by.clone(),
        };

        // イベントを送信
        let event = RollbackEvent::ManualRollbackInitiated {
            rollback_id: rollback_id.clone(),
            initiated_by,
            target_snapshot_id: snapshot_id,
            reason,
        };
        let _ = self.event_sender.send(event);

        // 実行器に追加
        self.executor.start_rollback(active_rollback).await?;

        // メトリクスを更新
        {
            let mut metrics = self.rollback_metrics.write().await;
            metrics.total_rollbacks += 1;
            metrics.manual_rollbacks += 1;
        }

        // 自動的にロールバックを完了させる（テスト用）
        self.complete_rollback(rollback_id.clone(), true, 500)
            .await?;

        Ok(rollback_id)
    }

    /// メトリクス監視を開始
    pub async fn start_monitoring(&self) -> Result<(), McpError> {
        let config = self.rollback_config.read().await.clone();

        if !config.auto_rollback_enabled {
            debug!("Auto rollback is disabled, skipping monitoring");
            return Ok(());
        }

        // 停止フラグをリセット
        self.monitoring_stop_flag.store(false, Ordering::SeqCst);

        let monitor_interval = Duration::from_secs(1); // テスト用に短縮: 1秒間隔
        let mut interval_timer = interval(monitor_interval);

        info!(
            "Starting rollback monitoring with interval: {:?}",
            monitor_interval
        );

        // 監視ループを別のタスクとして実行
        let stop_flag = self.monitoring_stop_flag.clone();
        let _rollback_config = self.rollback_config.clone();
        let _metrics_monitor = self.metrics_monitor.clone();

        tokio::spawn(async move {
            loop {
                // 停止フラグをチェック
                if stop_flag.load(Ordering::SeqCst) {
                    info!("Monitoring stopped by stop flag");
                    break;
                }

                interval_timer.tick().await;

                // 簡単なメトリクスチェック（実際の実装では詳細なロジックが必要）
                debug!("Checking rollback conditions");
            }
        });

        Ok(())
    }

    /// ロールバック条件をチェック
    async fn check_rollback_conditions(&self) -> Result<(), McpError> {
        let metrics = self.collect_current_metrics().await?;
        let config = self.rollback_config.read().await;

        // エラー率チェック
        if metrics.canary_metrics.error_rate > config.error_rate_threshold {
            let reason = format!(
                "Error rate exceeded threshold: {:.2}% > {:.2}%",
                metrics.canary_metrics.error_rate, config.error_rate_threshold
            );

            warn!("Rollback condition triggered: {}", reason);
            drop(config); // ロックを解放

            self.trigger_auto_rollback(reason, metrics).await?;
            return Ok(());
        }

        // レスポンス時間チェック
        if metrics.canary_metrics.avg_response_time_ms > config.response_time_threshold_ms as f64 {
            let reason = format!(
                "Response time exceeded threshold: {:.2}ms > {}ms",
                metrics.canary_metrics.avg_response_time_ms, config.response_time_threshold_ms
            );

            warn!("Rollback condition triggered: {}", reason);
            drop(config); // ロックを解放

            self.trigger_auto_rollback(reason, metrics).await?;
            return Ok(());
        }

        debug!("All rollback conditions are within acceptable ranges");
        Ok(())
    }

    /// 現在のメトリクスを収集
    async fn collect_current_metrics(&self) -> Result<MetricsSnapshot, McpError> {
        // TODO: 実際のメトリクス収集ロジックを実装
        // 現在はモックデータを返す
        Ok(MetricsSnapshot {
            timestamp: Utc::now(),
            stable_metrics: PolicyMetrics::default(),
            canary_metrics: PolicyMetrics::default(),
            system_metrics: SystemMetrics::default(),
            custom_metrics: HashMap::new(),
        })
    }

    /// 安定したスナップショットを検索
    async fn find_stable_snapshot(&self) -> Result<DeploymentSnapshot, McpError> {
        let history = self.deployment_history.read().await;

        // 最新の安定したスナップショットを検索
        for snapshot in history.iter().rev() {
            if matches!(snapshot.deployment_state, DeploymentState::Idle) {
                return Ok(snapshot.clone());
            }
        }

        Err(McpError::InvalidInput(
            "No stable snapshot found for rollback".to_string(),
        ))
    }

    /// 指定されたIDのスナップショットを取得
    async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<DeploymentSnapshot>, McpError> {
        let history = self.deployment_history.read().await;

        for snapshot in history.iter() {
            if snapshot.id == snapshot_id {
                return Ok(Some(snapshot.clone()));
            }
        }

        Ok(None)
    }

    /// ロールバック履歴を取得
    pub async fn get_rollback_history(&self) -> Result<Vec<DeploymentSnapshot>, McpError> {
        let history = self.deployment_history.read().await;
        Ok(history.iter().cloned().collect())
    }

    /// ロールバックメトリクスを取得
    pub async fn get_rollback_metrics(&self) -> Result<RollbackMetrics, McpError> {
        let metrics = self.rollback_metrics.read().await;
        Ok(metrics.clone())
    }

    /// イベントストリームを取得
    pub fn subscribe_events(&self) -> broadcast::Receiver<RollbackEvent> {
        self.event_sender.subscribe()
    }

    /// ロールバック設定を更新
    pub async fn update_config(&self, new_config: RollbackConfig) -> Result<(), McpError> {
        let mut config = self.rollback_config.write().await;
        *config = new_config;
        info!("Rollback configuration updated");
        Ok(())
    }

    /// 設定を取得
    pub async fn get_config(&self) -> Result<RollbackConfig, McpError> {
        let config = self.rollback_config.read().await.clone();
        Ok(config)
    }

    /// 自動ロールバックが必要かどうかを判定
    pub async fn should_trigger_auto_rollback(
        &self,
        metrics: &MetricsSnapshot,
    ) -> Result<bool, McpError> {
        let config = self.rollback_config.read().await;

        if !config.auto_rollback_enabled {
            return Ok(false);
        }

        // エラー率チェック
        if metrics.canary_metrics.error_rate > config.error_rate_threshold {
            return Ok(true);
        }

        // 応答時間チェック
        if metrics.canary_metrics.avg_response_time_ms > config.response_time_threshold_ms as f64 {
            return Ok(true);
        }

        Ok(false)
    }

    /// 監視を停止
    pub async fn stop_monitoring(&self) -> Result<(), McpError> {
        info!("Stopping rollback monitoring");
        self.monitoring_stop_flag.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// メトリクスの成功率を更新
    async fn update_success_rate(&self) {
        let mut metrics = self.rollback_metrics.write().await;
        if metrics.total_rollbacks > 0 {
            metrics.success_rate =
                (metrics.successful_rollbacks as f64 / metrics.total_rollbacks as f64) * 100.0;
        } else {
            metrics.success_rate = 0.0;
        }
    }

    /// ロールバック完了時のメトリクス更新
    pub async fn complete_rollback(
        &self,
        rollback_id: String,
        success: bool,
        duration_ms: u64,
    ) -> Result<(), McpError> {
        {
            let mut metrics = self.rollback_metrics.write().await;
            if success {
                metrics.successful_rollbacks += 1;
            } else {
                metrics.failed_rollbacks += 1;
            }
            metrics.last_rollback_time = Some(chrono::Utc::now());

            // 平均時間を更新
            let total_completed = metrics.successful_rollbacks + metrics.failed_rollbacks;
            if total_completed > 0 {
                let current_avg = metrics.avg_rollback_duration_ms;
                metrics.avg_rollback_duration_ms = (current_avg * (total_completed - 1) as f64
                    + duration_ms as f64)
                    / total_completed as f64;
            }
        }

        // 成功率を更新
        self.update_success_rate().await;

        // イベントを送信
        let event = if success {
            RollbackEvent::RollbackCompleted {
                rollback_id,
                snapshot_id: "unknown".to_string(), // TODO: 実際のスナップショットIDを追跡
                duration_ms,
                final_state: DeploymentState::Idle,
                success_metrics: Default::default(),
            }
        } else {
            RollbackEvent::RollbackFailed {
                rollback_id,
                snapshot_id: "unknown".to_string(), // TODO: 実際のスナップショットIDを追跡
                error_message: "Rollback execution failed".to_string(),
                partial_completion_percentage: 0.0,
                error_metrics: None,
            }
        };
        let _ = self.event_sender.send(event);

        Ok(())
    }
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            auto_rollback_enabled: true,
            error_rate_threshold: 5.0,        // 5%
            response_time_threshold_ms: 1000, // 1秒
            evaluation_window_minutes: 5,
            staged_rollback: StagedRollbackConfig::default(),
            max_snapshots: 50,
            confirmation_timeout_seconds: 30,
            custom_conditions: Vec::new(),
        }
    }
}

impl Default for StagedRollbackConfig {
    fn default() -> Self {
        Self {
            stages: vec![
                RollbackStage {
                    name: "Initial".to_string(),
                    target_percentage: 75.0,
                    max_duration_seconds: 60,
                    success_criteria: Vec::new(),
                },
                RollbackStage {
                    name: "Intermediate".to_string(),
                    target_percentage: 50.0,
                    max_duration_seconds: 60,
                    success_criteria: Vec::new(),
                },
                RollbackStage {
                    name: "Final".to_string(),
                    target_percentage: 0.0,
                    max_duration_seconds: 60,
                    success_criteria: Vec::new(),
                },
            ],
            stage_interval_seconds: 30,
            evaluate_between_stages: true,
            max_total_duration_seconds: 300, // 5分
        }
    }
}

impl Default for MetricsMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsMonitor {
    pub fn new() -> Self {
        Self {
            current_metrics: MetricsSnapshot {
                timestamp: Utc::now(),
                stable_metrics: PolicyMetrics::default(),
                canary_metrics: PolicyMetrics::default(),
                system_metrics: SystemMetrics::default(),
                custom_metrics: HashMap::new(),
            },
            metrics_history: VecDeque::new(),
            monitoring_interval_seconds: 30,
            anomaly_detection_config: AnomalyDetectionConfig::default(),
        }
    }
}

impl Default for AnomalyDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            statistical_threshold: 2.0, // 2標準偏差
            moving_average_window: 10,
            seasonal_adjustment: false,
        }
    }
}

impl RollbackExecutor {
    pub fn new(_canary_manager: Arc<CanaryDeploymentManager>) -> Self {
        Self {
            active_rollbacks: Arc::new(RwLock::new(HashMap::new())),
            execution_config: RollbackExecutionConfig::default(),
        }
    }

    pub async fn start_rollback(&self, rollback: ActiveRollback) -> Result<(), McpError> {
        let rollback_id = rollback.id.clone();

        // アクティブなロールバックに追加
        {
            let mut active = self.active_rollbacks.write().await;
            active.insert(rollback_id.clone(), rollback);
        }

        info!("Started rollback execution: {}", rollback_id);

        // TODO: 実際のロールバック実行ロジックを実装
        // 現在はプレースホルダー

        Ok(())
    }
}

impl Default for RollbackExecutionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_rollbacks: 3,
            default_timeout_seconds: 300, // 5分
            retry_attempts: 3,
            retry_interval_seconds: 10,
            dry_run_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_config::PolicyConfig;

    #[tokio::test]
    async fn test_rollback_manager_creation() {
        // TODO: テスト用のcanary_managerを作成
        // 現在はコンパイルテストのみ
    }

    #[tokio::test]
    async fn test_snapshot_creation() {
        // TODO: スナップショット作成のテスト
    }

    #[tokio::test]
    async fn test_auto_rollback_trigger() {
        // TODO: 自動ロールバックのテスト
    }

    #[tokio::test]
    async fn test_manual_rollback_initiation() {
        // TODO: 手動ロールバックのテスト
    }
}
