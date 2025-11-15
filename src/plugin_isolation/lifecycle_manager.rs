//! プラグインライフサイクル管理
//!
//! プラグインの登録、起動、監視、更新、停止、削除を管理する

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::McpError;
use crate::plugin_isolation::{PluginMetadata, PluginMetrics, PluginState};

/// ライフサイクル管理
#[derive(Debug)]
pub struct LifecycleManager {
    /// 管理対象プラグイン
    plugins: Arc<RwLock<HashMap<Uuid, PluginLifecycle>>>,
    /// ヘルスチェッカー
    health_checker: Arc<HealthChecker>,
    /// バージョン管理
    version_manager: Arc<VersionManager>,
    /// 自動復旧システム
    auto_recovery: Arc<AutoRecoverySystem>,
    /// 依存関係管理
    dependency_manager: Arc<DependencyManager>,
}

/// プラグインライフサイクル情報
#[derive(Debug, Clone)]
pub struct PluginLifecycle {
    /// プラグインID
    pub plugin_id: Uuid,
    /// 現在の状態
    pub current_state: PluginState,
    /// 目標状態
    pub target_state: PluginState,
    /// ライフサイクルイベント履歴
    pub lifecycle_events: Vec<LifecycleEvent>,
    /// ヘルスチェック設定
    pub health_check_config: HealthCheckConfig,
    /// 自動復旧設定
    pub auto_recovery_config: AutoRecoveryConfig,
    /// 依存関係
    pub dependencies: Vec<Uuid>,
    /// 逆依存関係（このプラグインに依存しているプラグイン）
    pub dependents: Vec<Uuid>,
    /// バージョン情報
    pub version_info: VersionInfo,
}

/// ライフサイクルイベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    /// イベントID
    pub event_id: Uuid,
    /// イベントタイプ
    pub event_type: LifecycleEventType,
    /// 発生時刻
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 詳細情報
    pub details: String,
    /// 実行者
    pub actor: String,
    /// 成功/失敗
    pub success: bool,
    /// エラー情報
    pub error_info: Option<String>,
}

/// ライフサイクルイベントタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleEventType {
    /// 登録
    Register,
    /// 起動
    Start,
    /// 停止
    Stop,
    /// 再起動
    Restart,
    /// 一時停止
    Pause,
    /// 再開
    Resume,
    /// 更新
    Update,
    /// 削除
    Unregister,
    /// ヘルスチェック
    HealthCheck,
    /// 自動復旧
    AutoRecover,
    /// 隔離
    Quarantine,
}

/// ヘルスチェック設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// 有効/無効
    pub enabled: bool,
    /// チェック間隔（秒）
    pub interval_secs: u64,
    /// タイムアウト（秒）
    pub timeout_secs: u64,
    /// 失敗の閾値
    pub failure_threshold: u32,
    /// 成功の閾値
    pub success_threshold: u32,
    /// ヘルスチェックエンドポイント
    pub endpoint: String,
    /// 期待するHTTPステータスコード
    pub expected_status_codes: Vec<u16>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 30,
            timeout_secs: 10,
            failure_threshold: 3,
            success_threshold: 1,
            endpoint: "/health".to_string(),
            expected_status_codes: vec![200],
        }
    }
}

/// 自動復旧設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRecoveryConfig {
    /// 有効/無効
    pub enabled: bool,
    /// 最大復旧試行回数
    pub max_retry_attempts: u32,
    /// 復旧間隔（秒）
    pub retry_interval_secs: u64,
    /// バックオフ戦略
    pub backoff_strategy: BackoffStrategy,
    /// 復旧方法
    pub recovery_methods: Vec<RecoveryMethod>,
}

impl Default for AutoRecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retry_attempts: 3,
            retry_interval_secs: 60,
            backoff_strategy: BackoffStrategy::Exponential,
            recovery_methods: vec![RecoveryMethod::Restart, RecoveryMethod::Recreate],
        }
    }
}

/// バックオフ戦略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffStrategy {
    /// 固定間隔
    Fixed,
    /// 線形増加
    Linear,
    /// 指数的増加
    Exponential,
}

/// 復旧方法
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryMethod {
    /// 再起動
    Restart,
    /// 再作成
    Recreate,
    /// ロールバック
    Rollback,
    /// 手動介入
    Manual,
}

/// バージョン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// 現在のバージョン
    pub current_version: String,
    /// 利用可能なバージョン
    pub available_versions: Vec<String>,
    /// 更新履歴
    pub update_history: Vec<UpdateRecord>,
}

/// 更新記録
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecord {
    /// 更新前バージョン
    pub from_version: String,
    /// 更新後バージョン
    pub to_version: String,
    /// 更新時刻
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// 更新理由
    pub reason: String,
    /// 成功/失敗
    pub success: bool,
}

/// ヘルスチェッカー
#[derive(Debug)]
pub struct HealthChecker {
    /// ヘルス状態追跡
    health_states: Arc<RwLock<HashMap<Uuid, HealthState>>>,
    /// HTTPクライアント
    http_client: reqwest::Client,
}

/// ヘルス状態
#[derive(Debug, Clone)]
pub struct HealthState {
    /// 現在の状態
    pub status: HealthStatus,
    /// 連続失敗回数
    pub consecutive_failures: u32,
    /// 連続成功回数
    pub consecutive_successes: u32,
    /// 最終チェック時刻
    pub last_check: chrono::DateTime<chrono::Utc>,
    /// 最終成功時刻
    pub last_success: Option<chrono::DateTime<chrono::Utc>>,
    /// 最終失敗時刻
    pub last_failure: Option<chrono::DateTime<chrono::Utc>>,
}

/// ヘルス状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// 健全
    Healthy,
    /// 不健全
    Unhealthy,
    /// 不明
    Unknown,
}

/// バージョン管理
#[derive(Debug)]
pub struct VersionManager {
    /// バージョン情報
    version_info: Arc<RwLock<HashMap<Uuid, VersionInfo>>>,
}

/// 自動復旧システム
#[derive(Debug)]
pub struct AutoRecoverySystem {
    /// 復旧状態追跡
    recovery_states: Arc<RwLock<HashMap<Uuid, RecoveryState>>>,
}

/// 復旧状態
#[derive(Debug, Clone)]
pub struct RecoveryState {
    /// 復旧試行回数
    pub attempt_count: u32,
    /// 最終復旧試行時刻
    pub last_attempt: chrono::DateTime<chrono::Utc>,
    /// 次回復旧予定時刻
    pub next_attempt: chrono::DateTime<chrono::Utc>,
    /// 現在の復旧方法
    pub current_method: RecoveryMethod,
}

/// 依存関係管理
#[derive(Debug)]
pub struct DependencyManager {
    /// 依存関係グラフ
    dependency_graph: Arc<RwLock<HashMap<Uuid, Vec<Uuid>>>>,
}

impl LifecycleManager {
    /// 新しいライフサイクル管理を作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing lifecycle manager");

        let health_checker = Arc::new(HealthChecker::new().await?);
        let version_manager = Arc::new(VersionManager::new().await?);
        let auto_recovery = Arc::new(AutoRecoverySystem::new().await?);
        let dependency_manager = Arc::new(DependencyManager::new().await?);

        Ok(Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            health_checker,
            version_manager,
            auto_recovery,
            dependency_manager,
        })
    }

    /// プラグインを登録
    pub async fn register_plugin(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!("Registering plugin in lifecycle manager: {}", plugin_id);

        let lifecycle = PluginLifecycle {
            plugin_id,
            current_state: PluginState::Uninitialized,
            target_state: PluginState::Uninitialized,
            lifecycle_events: vec![],
            health_check_config: HealthCheckConfig::default(),
            auto_recovery_config: AutoRecoveryConfig::default(),
            dependencies: vec![],
            dependents: vec![],
            version_info: VersionInfo {
                current_version: "1.0.0".to_string(),
                available_versions: vec!["1.0.0".to_string()],
                update_history: vec![],
            },
        };

        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id, lifecycle);

        // ライフサイクルイベントを記録
        self.record_lifecycle_event(
            plugin_id,
            LifecycleEventType::Register,
            "Plugin registered".to_string(),
            "system".to_string(),
            true,
            None,
        )
        .await?;

        info!("Plugin registered in lifecycle manager: {}", plugin_id);
        Ok(())
    }

    /// プラグインの状態を変更
    pub async fn transition_state(
        &self,
        plugin_id: Uuid,
        target_state: PluginState,
    ) -> Result<(), McpError> {
        info!(
            "Transitioning plugin {} to state: {:?}",
            plugin_id, target_state
        );

        let mut plugins = self.plugins.write().await;
        let lifecycle = plugins
            .get_mut(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin not found".to_string()))?;

        let current_state = lifecycle.current_state;
        lifecycle.target_state = target_state;

        // 状態遷移の妥当性をチェック
        self.validate_state_transition(current_state, target_state)?;

        // 依存関係チェック
        self.check_dependencies(plugin_id, target_state).await?;

        // 状態遷移を実行
        match target_state {
            PluginState::Running => {
                self.start_plugin_lifecycle(plugin_id).await?;
            }
            PluginState::Stopped => {
                self.stop_plugin_lifecycle(plugin_id).await?;
            }
            PluginState::Paused => {
                self.pause_plugin_lifecycle(plugin_id).await?;
            }
            _ => {}
        }

        lifecycle.current_state = target_state;

        info!(
            "Plugin {} transitioned to state: {:?}",
            plugin_id, target_state
        );
        Ok(())
    }

    /// 状態遷移の妥当性をチェック
    fn validate_state_transition(
        &self,
        current: PluginState,
        target: PluginState,
    ) -> Result<(), McpError> {
        let valid_transitions = match current {
            PluginState::Uninitialized => vec![PluginState::Starting],
            PluginState::Starting => vec![PluginState::Running, PluginState::Error],
            PluginState::Running => vec![
                PluginState::Paused,
                PluginState::Stopping,
                PluginState::Error,
                PluginState::Quarantined,
            ],
            PluginState::Paused => vec![PluginState::Running, PluginState::Stopping],
            PluginState::Stopping => vec![PluginState::Stopped],
            PluginState::Stopped => vec![PluginState::Starting],
            PluginState::Error => vec![PluginState::Starting, PluginState::Stopped],
            PluginState::Quarantined => vec![PluginState::Stopped],
        };

        if !valid_transitions.contains(&target) {
            return Err(McpError::PluginError(format!(
                "Invalid state transition from {:?} to {:?}",
                current, target
            )));
        }

        Ok(())
    }

    /// 依存関係をチェック
    async fn check_dependencies(
        &self,
        plugin_id: Uuid,
        target_state: PluginState,
    ) -> Result<(), McpError> {
        let plugins = self.plugins.read().await;
        let lifecycle = plugins
            .get(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin not found".to_string()))?;

        match target_state {
            PluginState::Running => {
                // 依存関係がすべて実行中かチェック
                for dep_id in &lifecycle.dependencies {
                    if let Some(dep_lifecycle) = plugins.get(dep_id) {
                        if dep_lifecycle.current_state != PluginState::Running {
                            return Err(McpError::PluginError(format!(
                                "Dependency {} is not running",
                                dep_id
                            )));
                        }
                    }
                }
            }
            PluginState::Stopped => {
                // 逆依存関係がすべて停止済みかチェック
                for dependent_id in &lifecycle.dependents {
                    if let Some(dependent_lifecycle) = plugins.get(dependent_id) {
                        if dependent_lifecycle.current_state == PluginState::Running {
                            return Err(McpError::PluginError(format!(
                                "Dependent {} is still running",
                                dependent_id
                            )));
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// プラグインライフサイクルを開始
    async fn start_plugin_lifecycle(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Starting plugin lifecycle: {}", plugin_id);

        // ヘルスチェックを開始
        self.health_checker.start_health_check(plugin_id).await?;

        // ライフサイクルイベントを記録
        self.record_lifecycle_event(
            plugin_id,
            LifecycleEventType::Start,
            "Plugin lifecycle started".to_string(),
            "system".to_string(),
            true,
            None,
        )
        .await?;

        Ok(())
    }

    /// プラグインライフサイクルを停止
    async fn stop_plugin_lifecycle(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Stopping plugin lifecycle: {}", plugin_id);

        // ヘルスチェックを停止
        self.health_checker.stop_health_check(plugin_id).await?;

        // ライフサイクルイベントを記録
        self.record_lifecycle_event(
            plugin_id,
            LifecycleEventType::Stop,
            "Plugin lifecycle stopped".to_string(),
            "system".to_string(),
            true,
            None,
        )
        .await?;

        Ok(())
    }

    /// プラグインライフサイクルを一時停止
    async fn pause_plugin_lifecycle(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Pausing plugin lifecycle: {}", plugin_id);

        // ヘルスチェックを一時停止
        self.health_checker.pause_health_check(plugin_id).await?;

        // ライフサイクルイベントを記録
        self.record_lifecycle_event(
            plugin_id,
            LifecycleEventType::Pause,
            "Plugin lifecycle paused".to_string(),
            "system".to_string(),
            true,
            None,
        )
        .await?;

        Ok(())
    }

    /// ライフサイクルイベントを記録
    async fn record_lifecycle_event(
        &self,
        plugin_id: Uuid,
        event_type: LifecycleEventType,
        details: String,
        actor: String,
        success: bool,
        error_info: Option<String>,
    ) -> Result<(), McpError> {
        let event = LifecycleEvent {
            event_id: Uuid::new_v4(),
            event_type,
            timestamp: chrono::Utc::now(),
            details,
            actor,
            success,
            error_info,
        };

        let mut plugins = self.plugins.write().await;
        if let Some(lifecycle) = plugins.get_mut(&plugin_id) {
            lifecycle.lifecycle_events.push(event);
        }

        Ok(())
    }

    /// プラグインのヘルス状態を取得
    pub async fn get_plugin_health(&self, plugin_id: Uuid) -> Result<HealthStatus, McpError> {
        self.health_checker.get_health_status(plugin_id).await
    }

    /// プラグインの自動復旧を実行
    pub async fn trigger_auto_recovery(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!("Triggering auto recovery for plugin: {}", plugin_id);

        self.auto_recovery.attempt_recovery(plugin_id).await?;

        // ライフサイクルイベントを記録
        self.record_lifecycle_event(
            plugin_id,
            LifecycleEventType::AutoRecover,
            "Auto recovery triggered".to_string(),
            "system".to_string(),
            true,
            None,
        )
        .await?;

        Ok(())
    }

    /// プラグインの依存関係を設定
    pub async fn set_dependencies(
        &self,
        plugin_id: Uuid,
        dependencies: Vec<Uuid>,
    ) -> Result<(), McpError> {
        info!(
            "Setting dependencies for plugin {}: {:?}",
            plugin_id, dependencies
        );

        self.dependency_manager
            .set_dependencies(plugin_id, dependencies.clone())
            .await?;

        let mut plugins = self.plugins.write().await;
        if let Some(lifecycle) = plugins.get_mut(&plugin_id) {
            lifecycle.dependencies = dependencies;
        }

        Ok(())
    }

    /// ライフサイクル管理をシャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down lifecycle manager");

        // 全プラグインのヘルスチェックを停止
        let plugin_ids: Vec<Uuid> = {
            let plugins = self.plugins.read().await;
            plugins.keys().cloned().collect()
        };

        for plugin_id in plugin_ids {
            if let Err(e) = self.health_checker.stop_health_check(plugin_id).await {
                error!(
                    "Failed to stop health check for plugin {}: {}",
                    plugin_id, e
                );
            }
        }

        info!("Lifecycle manager shutdown completed");
        Ok(())
    }
}

impl HealthChecker {
    /// 新しいヘルスチェッカーを作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing health checker");

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| McpError::PluginError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            health_states: Arc::new(RwLock::new(HashMap::new())),
            http_client,
        })
    }

    /// ヘルスチェックを開始
    pub async fn start_health_check(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Starting health check for plugin: {}", plugin_id);

        let health_state = HealthState {
            status: HealthStatus::Unknown,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check: chrono::Utc::now(),
            last_success: None,
            last_failure: None,
        };

        let mut states = self.health_states.write().await;
        states.insert(plugin_id, health_state);

        // 定期的なヘルスチェックを開始
        let health_states = self.health_states.clone();
        let http_client = self.http_client.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            loop {
                interval.tick().await;

                if let Err(e) = Self::perform_health_check(
                    plugin_id,
                    health_states.clone(),
                    http_client.clone(),
                )
                .await
                {
                    error!("Health check failed for plugin {}: {}", plugin_id, e);
                }
            }
        });

        Ok(())
    }

    /// ヘルスチェックを実行
    async fn perform_health_check(
        plugin_id: Uuid,
        health_states: Arc<RwLock<HashMap<Uuid, HealthState>>>,
        http_client: reqwest::Client,
    ) -> Result<(), McpError> {
        // TODO: 実際のヘルスチェックロジックを実装
        // 今はダミー実装
        let is_healthy = true; // プラグインの実際の状態をチェック

        let mut states = health_states.write().await;
        if let Some(state) = states.get_mut(&plugin_id) {
            state.last_check = chrono::Utc::now();

            if is_healthy {
                state.status = HealthStatus::Healthy;
                state.consecutive_successes += 1;
                state.consecutive_failures = 0;
                state.last_success = Some(chrono::Utc::now());
            } else {
                state.status = HealthStatus::Unhealthy;
                state.consecutive_failures += 1;
                state.consecutive_successes = 0;
                state.last_failure = Some(chrono::Utc::now());
            }
        }

        Ok(())
    }

    /// ヘルスチェックを停止
    pub async fn stop_health_check(&self, plugin_id: Uuid) -> Result<(), McpError> {
        debug!("Stopping health check for plugin: {}", plugin_id);

        let mut states = self.health_states.write().await;
        states.remove(&plugin_id);

        Ok(())
    }

    /// ヘルスチェックを一時停止
    pub async fn pause_health_check(&self, _plugin_id: Uuid) -> Result<(), McpError> {
        // TODO: 一時停止ロジックを実装
        Ok(())
    }

    /// ヘルス状態を取得
    pub async fn get_health_status(&self, plugin_id: Uuid) -> Result<HealthStatus, McpError> {
        let states = self.health_states.read().await;
        let state = states
            .get(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin health state not found".to_string()))?;
        Ok(state.status.clone())
    }
}

impl VersionManager {
    /// 新しいバージョン管理を作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing version manager");
        Ok(Self {
            version_info: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

impl AutoRecoverySystem {
    /// 新しい自動復旧システムを作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing auto recovery system");
        Ok(Self {
            recovery_states: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 復旧を試行
    pub async fn attempt_recovery(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!("Attempting recovery for plugin: {}", plugin_id);
        // TODO: 実際の復旧ロジックを実装
        Ok(())
    }
}

impl DependencyManager {
    /// 新しい依存関係管理を作成
    pub async fn new() -> Result<Self, McpError> {
        info!("Initializing dependency manager");
        Ok(Self {
            dependency_graph: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 依存関係を設定
    pub async fn set_dependencies(
        &self,
        plugin_id: Uuid,
        dependencies: Vec<Uuid>,
    ) -> Result<(), McpError> {
        let mut graph = self.dependency_graph.write().await;
        graph.insert(plugin_id, dependencies);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lifecycle_manager_creation() {
        let manager = LifecycleManager::new().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_plugin_registration() {
        let manager = LifecycleManager::new().await.unwrap();
        let plugin_id = Uuid::new_v4();

        let result = manager.register_plugin(plugin_id).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.interval_secs, 30);
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.failure_threshold, 3);
    }
}
