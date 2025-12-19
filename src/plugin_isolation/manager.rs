//! Isolated Plugin Manager
//!
//! プラグインマネージャーの実装

use super::config::PluginManagerConfig;
use super::health::PluginManagerHealth;
use super::types::{PluginInstance, PluginMetadata, PluginMetrics, PluginState};
use super::{communication_broker, isolation_engine, lifecycle_manager, monitoring, sandbox};
use crate::error::McpError;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// 隔離プラグインマネージャー
#[derive(Debug)]
pub struct IsolatedPluginManager {
    /// プラグインインスタンス管理
    plugins: Arc<RwLock<HashMap<Uuid, Arc<Mutex<PluginInstance>>>>>,
    /// 隔離エンジン
    isolation_engine: Arc<isolation_engine::IsolationEngine>,
    /// ライフサイクル管理
    lifecycle_manager: Arc<lifecycle_manager::LifecycleManager>,
    /// セキュリティサンドボックス
    sandbox: Arc<sandbox::SecuritySandbox>,
    /// 通信ブローカー
    communication_broker: Arc<communication_broker::CommunicationBroker>,
    /// 監視システム
    monitoring: Arc<monitoring::MonitoringSystem>,
    /// 設定
    config: PluginManagerConfig,
}

impl IsolatedPluginManager {
    /// 新しいプラグインマネージャーを作成
    pub async fn new(config: PluginManagerConfig) -> Result<Self, McpError> {
        info!("Initializing isolated plugin manager");

        let isolation_engine = Arc::new(
            isolation_engine::IsolationEngine::new(config.isolation_config.clone()).await?,
        );

        let lifecycle_manager = Arc::new(lifecycle_manager::LifecycleManager::new().await?);

        let sandbox =
            Arc::new(sandbox::SecuritySandbox::new(config.security_policy.clone()).await?);

        let communication_broker =
            Arc::new(communication_broker::CommunicationBroker::new().await?);

        // MonitoringConfigのデフォルト値を使用
        let monitoring = Arc::new(
            monitoring::MonitoringSystem::new_with_config(monitoring::MonitoringConfig::default())
                .await?,
        );

        Ok(Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            isolation_engine,
            lifecycle_manager,
            sandbox,
            communication_broker,
            monitoring,
            config,
        })
    }

    /// プラグインを登録
    pub async fn register_plugin(&self, metadata: PluginMetadata) -> Result<Uuid, McpError> {
        info!("Registering plugin: {}", metadata.name);

        // プラグイン数制限チェック
        let plugins = self.plugins.read().await;
        if plugins.len() >= self.config.max_plugins as usize {
            return Err(McpError::Plugin(
                "Maximum number of plugins reached".to_string(),
            ));
        }
        drop(plugins);

        // セキュリティ検証
        self.sandbox.validate_plugin_metadata(&metadata).await?;

        // プラグインインスタンス作成
        let instance = PluginInstance {
            metadata: metadata.clone(),
            state: PluginState::Uninitialized,
            container_id: None,
            process_id: None,
            started_at: None,
            last_activity: chrono::Utc::now(),
            error_info: None,
            metrics: PluginMetrics::default(),
            security_violations: 0,
        };

        // 登録
        let mut plugins = self.plugins.write().await;
        plugins.insert(metadata.id, Arc::new(Mutex::new(instance)));

        // ライフサイクル管理に登録
        self.lifecycle_manager.register_plugin(metadata.id).await?;

        info!("Plugin registered successfully: {}", metadata.id);
        Ok(metadata.id)
    }

    /// プラグインを起動
    pub async fn start_plugin(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!("Starting plugin: {}", plugin_id);

        let plugins = self.plugins.read().await;
        let plugin_arc = plugins
            .get(&plugin_id)
            .ok_or_else(|| McpError::Plugin("Plugin not found".to_string()))?
            .clone();
        drop(plugins);

        let mut plugin = plugin_arc.lock().await;

        // 状態チェック
        if plugin.state != PluginState::Uninitialized && plugin.state != PluginState::Stopped {
            return Err(McpError::Plugin(format!(
                "Plugin is not in a startable state: {:?}",
                plugin.state
            )));
        }

        plugin.state = PluginState::Starting;
        plugin.last_activity = chrono::Utc::now();
        drop(plugin);

        // 隔離環境でプラグインを起動
        let container_id = self.isolation_engine.start_plugin(plugin_id).await?;

        // サンドボックス適用
        self.sandbox
            .apply_sandbox_to_plugin(plugin_id, &container_id)
            .await?;

        // 通信ブローカーに登録
        self.communication_broker
            .register_plugin(plugin_id, communication_broker::ChannelType::Http)
            .await?;

        // プラグイン状態更新
        let mut plugin = plugin_arc.lock().await;
        plugin.state = PluginState::Running;
        plugin.container_id = Some(container_id);
        plugin.started_at = Some(chrono::Utc::now());
        plugin.last_activity = chrono::Utc::now();

        info!("Plugin started successfully: {}", plugin_id);
        Ok(())
    }

    /// プラグインを停止
    pub async fn stop_plugin(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!("Stopping plugin: {}", plugin_id);

        let plugins = self.plugins.read().await;
        let plugin_arc = plugins
            .get(&plugin_id)
            .ok_or_else(|| McpError::Plugin("Plugin not found".to_string()))?
            .clone();
        drop(plugins);

        let mut plugin = plugin_arc.lock().await;

        if plugin.state != PluginState::Running && plugin.state != PluginState::Paused {
            return Err(McpError::Plugin(format!(
                "Plugin is not in a stoppable state: {:?}",
                plugin.state
            )));
        }

        plugin.state = PluginState::Stopping;
        let container_id = plugin.container_id.clone();
        drop(plugin);

        // 隔離環境からプラグインを停止
        if let Some(container_id) = container_id {
            self.isolation_engine
                .stop_plugin(plugin_id, &container_id)
                .await?;
        }

        // 通信ブローカーから登録解除
        self.communication_broker
            .unregister_plugin(plugin_id)
            .await?;

        // プラグイン状態更新
        let mut plugin = plugin_arc.lock().await;
        plugin.state = PluginState::Stopped;
        plugin.container_id = None;
        plugin.process_id = None;
        plugin.last_activity = chrono::Utc::now();

        info!("Plugin stopped successfully: {}", plugin_id);
        Ok(())
    }

    /// プラグインを隔離状態にする（セキュリティ違反時）
    pub async fn quarantine_plugin(&self, plugin_id: Uuid, reason: String) -> Result<(), McpError> {
        warn!("Quarantining plugin: {} - Reason: {}", plugin_id, reason);

        let plugins = self.plugins.read().await;
        let plugin_arc = plugins
            .get(&plugin_id)
            .ok_or_else(|| McpError::Plugin("Plugin not found".to_string()))?
            .clone();
        drop(plugins);

        let mut plugin = plugin_arc.lock().await;

        // セキュリティ違反カウント増加
        plugin.security_violations += 1;
        plugin.state = PluginState::Quarantined;
        plugin.error_info = Some(reason.clone());
        plugin.last_activity = chrono::Utc::now();

        let container_id = plugin.container_id.clone();
        drop(plugin);

        // プラグインを強制停止
        if let Some(container_id) = container_id {
            self.isolation_engine
                .force_stop_plugin(plugin_id, &container_id)
                .await?;
        }

        // セキュリティイベントをログ記録
        error!(
            "Plugin {} quarantined due to security violation: {}",
            plugin_id, reason
        );

        warn!("Security alert for plugin {}: {}", plugin_id, reason);

        Ok(())
    }

    /// プラグインの状態を取得
    pub async fn get_plugin_state(&self, plugin_id: Uuid) -> Result<PluginState, McpError> {
        let plugins = self.plugins.read().await;
        let plugin_arc = plugins
            .get(&plugin_id)
            .ok_or_else(|| McpError::Plugin("Plugin not found".to_string()))?;

        let plugin = plugin_arc.lock().await;
        Ok(plugin.state)
    }

    /// プラグインのメトリクスを取得
    pub async fn get_plugin_metrics(&self, plugin_id: Uuid) -> Result<PluginMetrics, McpError> {
        let plugins = self.plugins.read().await;
        let plugin_arc = plugins
            .get(&plugin_id)
            .ok_or_else(|| McpError::Plugin("Plugin not found".to_string()))?;

        let plugin = plugin_arc.lock().await;
        Ok(plugin.metrics.clone())
    }

    /// 全プラグインの状態を取得
    pub async fn get_all_plugin_states(&self) -> HashMap<Uuid, PluginState> {
        let plugins = self.plugins.read().await;
        let mut states = HashMap::new();

        for (id, plugin_arc) in plugins.iter() {
            if let Ok(plugin) = plugin_arc.try_lock() {
                states.insert(*id, plugin.state);
            }
        }

        states
    }

    /// プラグインマネージャーをシャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down isolated plugin manager");

        // 全プラグインを停止
        let plugin_ids: Vec<Uuid> = {
            let plugins = self.plugins.read().await;
            plugins.keys().cloned().collect()
        };

        for plugin_id in plugin_ids {
            if let Err(e) = self.stop_plugin(plugin_id).await {
                error!("Failed to stop plugin {}: {}", plugin_id, e);
            }
        }

        // コンポーネントのシャットダウン
        self.monitoring.shutdown().await?;
        self.communication_broker.shutdown().await?;
        self.isolation_engine.shutdown().await?;

        info!("Isolated plugin manager shutdown completed");
        Ok(())
    }

    /// ヘルスチェック
    pub async fn health_check(&self) -> Result<PluginManagerHealth, McpError> {
        let plugins = self.plugins.read().await;
        let total_plugins = plugins.len();

        let mut running_count = 0;
        let mut error_count = 0;
        let mut quarantined_count = 0;

        for plugin_arc in plugins.values() {
            if let Ok(plugin) = plugin_arc.try_lock() {
                match plugin.state {
                    PluginState::Running => running_count += 1,
                    PluginState::Error => error_count += 1,
                    PluginState::Quarantined => quarantined_count += 1,
                    _ => {}
                }
            }
        }

        Ok(PluginManagerHealth {
            total_plugins,
            running_plugins: running_count,
            error_plugins: error_count,
            quarantined_plugins: quarantined_count,
            system_health: if error_count + quarantined_count == 0 {
                "Healthy".to_string()
            } else {
                "Degraded".to_string()
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_isolation::security_validation::SecurityLevel;

    #[tokio::test]
    async fn test_plugin_manager_creation() {
        let config = PluginManagerConfig::default();
        let manager = IsolatedPluginManager::new(config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_plugin_registration() {
        let config = PluginManagerConfig::default();
        let manager = IsolatedPluginManager::new(config).await.unwrap();

        let metadata = PluginMetadata {
            id: Uuid::new_v4(),
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: "test-author".to_string(),
            required_permissions: vec![],
            resource_limits: super::super::types::ResourceLimits::default(),
            security_level: SecurityLevel::Standard,
            dependencies: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let result = manager.register_plugin(metadata).await;
        assert!(result.is_ok());
    }
}
