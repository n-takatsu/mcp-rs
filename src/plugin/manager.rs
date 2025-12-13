//! プラグインマネージャー
//!
//! プラグインのライフサイクル管理、ロード、アンロード、実行を提供します。

use crate::error::Result;
use crate::plugin::isolation::{IsolationConfig, IsolationEnvironment};
use crate::plugin::resource::{ResourceLimits, ResourceMonitor, ResourceUsage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use uuid::Uuid;

/// プラグインの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// 初期化中
    Initializing,
    /// 起動中
    Starting,
    /// 実行中
    Running,
    /// 停止中
    Stopping,
    /// 停止済み
    Stopped,
    /// エラー
    Error,
}

/// プラグインのステータス情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatus {
    /// プラグインID
    pub plugin_id: String,
    /// 状態
    pub state: PluginState,
    /// 起動時刻
    pub started_at: Option<SystemTime>,
    /// 停止時刻
    pub stopped_at: Option<SystemTime>,
    /// リソース使用状況
    pub resource_usage: ResourceUsage,
    /// エラーメッセージ
    pub error_message: Option<String>,
}

/// プラグイン情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// プラグインID（UUID）
    pub id: String,
    /// プラグイン名
    pub name: String,
    /// バージョン
    pub version: String,
    /// 説明
    pub description: Option<String>,
    /// プラグインパス
    pub path: PathBuf,
    /// 隔離設定
    pub isolation_config: IsolationConfig,
    /// リソース制限
    pub resource_limits: ResourceLimits,
    /// 作成日時
    pub created_at: SystemTime,
    /// 最終更新日時
    pub updated_at: SystemTime,
    /// 有効/無効
    pub enabled: bool,
}

impl Plugin {
    /// 新しいプラグインを作成
    ///
    /// # 引数
    ///
    /// * `name` - プラグイン名
    /// * `version` - バージョン
    /// * `path` - プラグインパス
    /// * `isolation_config` - 隔離設定
    /// * `resource_limits` - リソース制限
    pub fn new(
        name: String,
        version: String,
        path: PathBuf,
        isolation_config: IsolationConfig,
        resource_limits: ResourceLimits,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            version,
            description: None,
            path,
            isolation_config,
            resource_limits,
            created_at: now,
            updated_at: now,
            enabled: true,
        }
    }
}

/// プラグインマネージャー
///
/// プラグインのライフサイクル管理を提供します。
pub struct PluginManager {
    /// プラグイン一覧
    plugins: Arc<RwLock<HashMap<String, Plugin>>>,
    /// プラグイン状態
    plugin_states: Arc<RwLock<HashMap<String, PluginState>>>,
    /// 隔離環境
    isolation_environments: Arc<RwLock<HashMap<String, IsolationEnvironment>>>,
    /// リソース監視
    resource_monitors: Arc<RwLock<HashMap<String, ResourceMonitor>>>,
    /// 起動タイムアウト
    startup_timeout: Duration,
}

impl PluginManager {
    /// 新しいプラグインマネージャーを作成
    ///
    /// # 引数
    ///
    /// * `startup_timeout` - 起動タイムアウト（デフォルト: 10秒）
    pub fn new(startup_timeout: Option<Duration>) -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            plugin_states: Arc::new(RwLock::new(HashMap::new())),
            isolation_environments: Arc::new(RwLock::new(HashMap::new())),
            resource_monitors: Arc::new(RwLock::new(HashMap::new())),
            startup_timeout: startup_timeout.unwrap_or(Duration::from_secs(10)),
        }
    }

    /// プラグインを登録
    ///
    /// # 引数
    ///
    /// * `plugin` - 登録するプラグイン
    pub async fn register_plugin(&self, plugin: Plugin) -> Result<String> {
        let plugin_id = plugin.id.clone();

        // プラグインを登録
        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id.clone(), plugin);

        // 初期状態を設定
        let mut states = self.plugin_states.write().await;
        states.insert(plugin_id.clone(), PluginState::Stopped);

        Ok(plugin_id)
    }

    /// プラグインを削除
    ///
    /// # 引数
    ///
    /// * `plugin_id` - 削除するプラグインID
    pub async fn unregister_plugin(&self, plugin_id: &str) -> Result<()> {
        // プラグインが実行中の場合は停止
        if let Some(state) = self.plugin_states.read().await.get(plugin_id) {
            if *state == PluginState::Running {
                self.stop_plugin(plugin_id).await?;
            }
        }

        // プラグインを削除
        let mut plugins = self.plugins.write().await;
        plugins.remove(plugin_id);

        let mut states = self.plugin_states.write().await;
        states.remove(plugin_id);

        Ok(())
    }

    /// プラグインを起動
    ///
    /// # 引数
    ///
    /// * `plugin_id` - 起動するプラグインID
    pub async fn start_plugin(&self, plugin_id: &str) -> Result<()> {
        let start_time = SystemTime::now();

        // プラグイン情報を取得
        let plugin = {
            let plugins = self.plugins.read().await;
            plugins
                .get(plugin_id)
                .ok_or_else(|| {
                    crate::error::Error::InvalidInput(format!("Plugin not found: {}", plugin_id))
                })?
                .clone()
        };

        // 状態をStartingに更新
        {
            let mut states = self.plugin_states.write().await;
            states.insert(plugin_id.to_string(), PluginState::Starting);
        }

        // 隔離環境を作成
        let isolation_env =
            IsolationEnvironment::new(plugin_id.to_string(), plugin.isolation_config.clone());

        // 隔離環境を起動
        isolation_env.start().await?;

        // リソース監視を開始
        let resource_monitor =
            ResourceMonitor::new(plugin_id.to_string(), plugin.resource_limits.clone());
        resource_monitor.start().await;

        // 隔離環境とリソース監視を保存
        {
            let mut envs = self.isolation_environments.write().await;
            envs.insert(plugin_id.to_string(), isolation_env);

            let mut monitors = self.resource_monitors.write().await;
            monitors.insert(plugin_id.to_string(), resource_monitor);
        }

        // 起動時間をチェック
        let elapsed = start_time.elapsed().unwrap_or(Duration::from_secs(0));
        if elapsed > self.startup_timeout {
            // タイムアウト
            self.stop_plugin(plugin_id).await?;
            return Err(crate::error::Error::InvalidInput(format!(
                "Plugin startup timeout: {} ({}ms > {}ms)",
                plugin_id,
                elapsed.as_millis(),
                self.startup_timeout.as_millis()
            )));
        }

        // 状態をRunningに更新
        {
            let mut states = self.plugin_states.write().await;
            states.insert(plugin_id.to_string(), PluginState::Running);
        }

        Ok(())
    }

    /// プラグインを停止
    ///
    /// # 引数
    ///
    /// * `plugin_id` - 停止するプラグインID
    pub async fn stop_plugin(&self, plugin_id: &str) -> Result<()> {
        // 状態をStoppingに更新
        {
            let mut states = self.plugin_states.write().await;
            states.insert(plugin_id.to_string(), PluginState::Stopping);
        }

        // リソース監視を停止
        {
            let mut monitors = self.resource_monitors.write().await;
            if let Some(monitor) = monitors.remove(plugin_id) {
                monitor.stop().await;
            }
        }

        // 隔離環境を停止
        {
            let mut envs = self.isolation_environments.write().await;
            if let Some(env) = envs.remove(plugin_id) {
                env.stop().await?;
            }
        }

        // 状態をStoppedに更新
        {
            let mut states = self.plugin_states.write().await;
            states.insert(plugin_id.to_string(), PluginState::Stopped);
        }

        Ok(())
    }

    /// プラグインを再起動
    ///
    /// # 引数
    ///
    /// * `plugin_id` - 再起動するプラグインID
    pub async fn restart_plugin(&self, plugin_id: &str) -> Result<()> {
        self.stop_plugin(plugin_id).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.start_plugin(plugin_id).await?;
        Ok(())
    }

    /// プラグインのステータスを取得
    ///
    /// # 引数
    ///
    /// * `plugin_id` - プラグインID
    pub async fn get_plugin_status(&self, plugin_id: &str) -> Result<PluginStatus> {
        let state = self
            .plugin_states
            .read()
            .await
            .get(plugin_id)
            .copied()
            .ok_or_else(|| {
                crate::error::Error::InvalidInput(format!("Plugin not found: {}", plugin_id))
            })?;

        let resource_usage =
            if let Some(monitor) = self.resource_monitors.read().await.get(plugin_id) {
                monitor.get_current_usage().await
            } else {
                ResourceUsage::default()
            };

        Ok(PluginStatus {
            plugin_id: plugin_id.to_string(),
            state,
            started_at: None, // TODO: 実際の起動時刻を記録
            stopped_at: None, // TODO: 実際の停止時刻を記録
            resource_usage,
            error_message: None,
        })
    }

    /// 全プラグインのステータスを取得
    pub async fn get_all_plugin_statuses(&self) -> Vec<PluginStatus> {
        let mut statuses = Vec::new();
        let plugins = self.plugins.read().await;

        for plugin_id in plugins.keys() {
            if let Ok(status) = self.get_plugin_status(plugin_id).await {
                statuses.push(status);
            }
        }

        statuses
    }

    /// 全プラグインを取得
    pub async fn get_all_plugins(&self) -> Vec<Plugin> {
        self.plugins.read().await.values().cloned().collect()
    }

    /// プラグインを取得
    ///
    /// # 引数
    ///
    /// * `plugin_id` - プラグインID
    pub async fn get_plugin(&self, plugin_id: &str) -> Option<Plugin> {
        self.plugins.read().await.get(plugin_id).cloned()
    }

    /// 実行中のプラグイン数を取得
    pub async fn get_running_count(&self) -> usize {
        self.plugin_states
            .read()
            .await
            .values()
            .filter(|s| **s == PluginState::Running)
            .count()
    }

    /// 全プラグインを停止
    pub async fn stop_all_plugins(&self) -> Result<()> {
        let plugin_ids: Vec<String> = self.plugins.read().await.keys().cloned().collect();

        for plugin_id in plugin_ids {
            if let Err(e) = self.stop_plugin(&plugin_id).await {
                eprintln!("Failed to stop plugin {}: {}", plugin_id, e);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::isolation::IsolationLevel;

    fn create_test_plugin() -> Plugin {
        Plugin::new(
            "test-plugin".to_string(),
            "1.0.0".to_string(),
            PathBuf::from("/tmp/test-plugin"),
            IsolationConfig {
                level: IsolationLevel::Container,
                network_isolation: true,
                filesystem_isolation: true,
                process_isolation: true,
            },
            ResourceLimits {
                max_cpu_percent: 50.0,
                max_memory_mb: 512,
                max_disk_io_mbps: 100,
            },
        )
    }

    #[tokio::test]
    async fn test_register_plugin() {
        let manager = PluginManager::new(None);
        let plugin = create_test_plugin();
        let plugin_id = manager.register_plugin(plugin).await.unwrap();

        assert!(!plugin_id.is_empty());
        assert_eq!(manager.get_all_plugins().await.len(), 1);
    }

    #[tokio::test]
    async fn test_unregister_plugin() {
        let manager = PluginManager::new(None);
        let plugin = create_test_plugin();
        let plugin_id = manager.register_plugin(plugin).await.unwrap();

        manager.unregister_plugin(&plugin_id).await.unwrap();
        assert_eq!(manager.get_all_plugins().await.len(), 0);
    }

    #[tokio::test]
    async fn test_get_plugin_status() {
        let manager = PluginManager::new(None);
        let plugin = create_test_plugin();
        let plugin_id = manager.register_plugin(plugin).await.unwrap();

        let status = manager.get_plugin_status(&plugin_id).await.unwrap();
        assert_eq!(status.state, PluginState::Stopped);
    }

    #[tokio::test]
    async fn test_get_running_count() {
        let manager = PluginManager::new(None);
        assert_eq!(manager.get_running_count().await, 0);

        let plugin = create_test_plugin();
        manager.register_plugin(plugin).await.unwrap();
        assert_eq!(manager.get_running_count().await, 0);
    }

    #[tokio::test]
    async fn test_get_all_plugin_statuses() {
        let manager = PluginManager::new(None);
        let plugin1 = create_test_plugin();
        let plugin2 = create_test_plugin();

        manager.register_plugin(plugin1).await.unwrap();
        manager.register_plugin(plugin2).await.unwrap();

        let statuses = manager.get_all_plugin_statuses().await;
        assert_eq!(statuses.len(), 2);
    }
}
