//! 隔離プラグインマネージャー
//! 
//! セキュアコアサーバーから完全に分離されたプラグインサーバー群の管理システム
//! 各プラグインは独立したコンテナ環境で実行され、厳格なセキュリティ制約下で動作する

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

pub mod isolation_engine;
pub mod lifecycle_manager;
pub mod sandbox;
pub mod communication_broker;
pub mod monitoring;
pub mod security_validation;

pub use communication_broker::{
    CommunicationBroker, CommunicationChannel, BrokerMessage, MessageType,
    ChannelType, EncryptionAlgorithm, RateLimitConfig, BrokerConfig,
    ChannelStats, AuthenticationInfo, MessageFilter, FilterType, FilterAction
};

pub use monitoring::{
    MonitoringSystem, PluginMetrics, SystemMetrics, LogEntry, LogLevel,
    Alert, AlertSeverity, AlertStatus, MonitoringEvent, MonitoringEventType,
    EventSeverity, MonitoringConfig, DetailedMetrics, ProcessStats,
    SecurityStats, PerformanceStats, MetricValue
};

pub use security_validation::{
    SecurityValidationSystem, ValidationResult, ValidationType, SecurityLevel,
    ValidationStatus, StaticAnalysisResult, DynamicAnalysisResult,
    VulnerabilityResult, PermissionValidationResult, SecurityFinding,
    FindingType, IssueSeverity, SecurityIssue, SecurityIssueType,
    SecurityValidationConfig
};

use crate::error::McpError;

/// プラグインの実行状態
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginState {
    /// 未初期化状態
    Uninitialized,
    /// 起動中
    Starting,
    /// 実行中
    Running,
    /// 一時停止中
    Paused,
    /// 停止中
    Stopping,
    /// 停止済み
    Stopped,
    /// エラー状態
    Error,
    /// 隔離状態（セキュリティ違反時）
    Quarantined,
}

/// プラグインメタデータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// プラグインID
    pub id: Uuid,
    /// プラグイン名
    pub name: String,
    /// バージョン
    pub version: String,
    /// 説明
    pub description: String,
    /// 作成者
    pub author: String,
    /// 必要な権限
    pub required_permissions: Vec<String>,
    /// リソース制限
    pub resource_limits: ResourceLimits,
    /// セキュリティレベル
    pub security_level: SecurityLevel,
    /// 依存関係
    pub dependencies: Vec<String>,
    /// 作成日時
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// 更新日時
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// リソース制限設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// 最大CPU使用率（0.0-1.0）
    pub max_cpu_usage: f64,
    /// 最大メモリ使用量（MB）
    pub max_memory_mb: u64,
    /// 最大ディスク使用量（MB）
    pub max_disk_mb: u64,
    /// 最大ネットワーク帯域幅（Mbps）
    pub max_network_mbps: u64,
    /// 最大同時接続数
    pub max_connections: u32,
    /// 実行時間制限（秒）
    pub max_execution_time_secs: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_usage: 0.5,
            max_memory_mb: 512,
            max_disk_mb: 1024,
            max_network_mbps: 10,
            max_connections: 100,
            max_execution_time_secs: 3600,
        }
    }
}

/// セキュリティレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    /// 最小限の制限
    Minimal,
    /// 標準的な制限
    Standard,
    /// 厳格な制限
    Strict,
    /// 最大限の制限
    Maximum,
}

/// プラグインインスタンス
#[derive(Debug)]
pub struct PluginInstance {
    /// メタデータ
    pub metadata: PluginMetadata,
    /// 現在の状態
    pub state: PluginState,
    /// コンテナID
    pub container_id: Option<String>,
    /// プロセスID
    pub process_id: Option<u32>,
    /// 起動時刻
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 最終アクティビティ時刻
    pub last_activity: chrono::DateTime<chrono::Utc>,
    /// エラー情報
    pub error_info: Option<String>,
    /// パフォーマンスメトリクス
    pub metrics: PluginMetrics,
    /// セキュリティ違反カウント
    pub security_violations: u32,
}

/// プラグインパフォーマンスメトリクス
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PluginMetrics {
    /// CPU使用率
    pub cpu_usage: f64,
    /// メモリ使用量（MB）
    pub memory_usage_mb: u64,
    /// ディスク使用量（MB）
    pub disk_usage_mb: u64,
    /// ネットワーク送信量（MB）
    pub network_tx_mb: u64,
    /// ネットワーク受信量（MB）
    pub network_rx_mb: u64,
    /// リクエスト処理数
    pub requests_processed: u64,
    /// エラー数
    pub error_count: u64,
    /// 平均レスポンス時間（ms）
    pub avg_response_time_ms: f64,
}

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

/// プラグインマネージャー設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerConfig {
    /// 最大プラグイン数
    pub max_plugins: u32,
    /// デフォルトリソース制限
    pub default_resource_limits: ResourceLimits,
    /// セキュリティポリシー
    pub security_policy: SecurityPolicy,
    /// 監視設定
    pub monitoring_config: MonitoringConfig,
    /// 隔離設定
    pub isolation_config: IsolationConfig,
}

/// セキュリティポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// 許可されたネットワークアクセス
    pub allowed_network_access: Vec<String>,
    /// 禁止されたシステムコール
    pub blocked_syscalls: Vec<String>,
    /// ファイルアクセス制限
    pub file_access_restrictions: Vec<String>,
    /// セキュリティ違反時の自動対応
    pub auto_quarantine_enabled: bool,
    /// 最大セキュリティ違反回数
    pub max_security_violations: u32,
}

/// 監視設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// メトリクス収集間隔（秒）
    pub metrics_collection_interval_secs: u64,
    /// ログレベル
    pub log_level: String,
    /// アラート閾値
    pub alert_thresholds: AlertThresholds,
}

/// アラート閾値
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// CPU使用率アラート閾値
    pub cpu_usage_threshold: f64,
    /// メモリ使用率アラート閾値
    pub memory_usage_threshold: f64,
    /// エラー率アラート閾値
    pub error_rate_threshold: f64,
}

/// 隔離設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationConfig {
    /// コンテナランタイム
    pub container_runtime: String,
    /// ネットワーク名前空間の使用
    pub use_network_namespace: bool,
    /// ファイルシステム隔離
    pub filesystem_isolation: bool,
    /// プロセス隔離
    pub process_isolation: bool,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            max_plugins: 100,
            default_resource_limits: ResourceLimits::default(),
            security_policy: SecurityPolicy {
                allowed_network_access: vec!["api.example.com".to_string()],
                blocked_syscalls: vec!["execve".to_string(), "fork".to_string()],
                file_access_restrictions: vec!["/etc".to_string(), "/sys".to_string()],
                auto_quarantine_enabled: true,
                max_security_violations: 3,
            },
            monitoring_config: MonitoringConfig {
                metrics_collection_interval_secs: 30,
                log_level: "INFO".to_string(),
                alert_thresholds: AlertThresholds {
                    cpu_usage_threshold: 0.8,
                    memory_usage_threshold: 0.9,
                    error_rate_threshold: 0.1,
                },
            },
            isolation_config: IsolationConfig {
                container_runtime: "docker".to_string(),
                use_network_namespace: true,
                filesystem_isolation: true,
                process_isolation: true,
            },
        }
    }
}

impl IsolatedPluginManager {
    /// 新しいプラグインマネージャーを作成
    pub async fn new(config: PluginManagerConfig) -> Result<Self, McpError> {
        info!("Initializing isolated plugin manager");

        let isolation_engine = Arc::new(
            isolation_engine::IsolationEngine::new(config.isolation_config.clone()).await?
        );
        
        let lifecycle_manager = Arc::new(
            lifecycle_manager::LifecycleManager::new().await?
        );
        
        let sandbox = Arc::new(
            sandbox::SecuritySandbox::new(config.security_policy.clone()).await?
        );
        
        let communication_broker = Arc::new(
            communication_broker::CommunicationBroker::new().await?
        );
        
        let monitoring = Arc::new(
            monitoring::MonitoringSystem::new(config.monitoring_config.clone()).await?
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
            return Err(McpError::PluginError(
                "Maximum number of plugins reached".to_string()
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

        // 監視開始
        self.monitoring.start_plugin_monitoring(metadata.id).await?;

        info!("Plugin registered successfully: {}", metadata.id);
        Ok(metadata.id)
    }

    /// プラグインを起動
    pub async fn start_plugin(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!("Starting plugin: {}", plugin_id);

        let plugins = self.plugins.read().await;
        let plugin_arc = plugins.get(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin not found".to_string()))?
            .clone();
        drop(plugins);

        let mut plugin = plugin_arc.lock().await;

        // 状態チェック
        if plugin.state != PluginState::Uninitialized && plugin.state != PluginState::Stopped {
            return Err(McpError::PluginError(
                format!("Plugin is not in a startable state: {:?}", plugin.state)
            ));
        }

        plugin.state = PluginState::Starting;
        plugin.last_activity = chrono::Utc::now();
        drop(plugin);

        // 隔離環境でプラグインを起動
        let container_id = self.isolation_engine.start_plugin(plugin_id).await?;

        // サンドボックス適用
        self.sandbox.apply_sandbox_to_plugin(plugin_id, &container_id).await?;

        // 通信ブローカーに登録
        self.communication_broker.register_plugin(plugin_id, &container_id).await?;

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
        let plugin_arc = plugins.get(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin not found".to_string()))?
            .clone();
        drop(plugins);

        let mut plugin = plugin_arc.lock().await;

        if plugin.state != PluginState::Running && plugin.state != PluginState::Paused {
            return Err(McpError::PluginError(
                format!("Plugin is not in a stoppable state: {:?}", plugin.state)
            ));
        }

        plugin.state = PluginState::Stopping;
        let container_id = plugin.container_id.clone();
        drop(plugin);

        // 隔離環境からプラグインを停止
        if let Some(container_id) = container_id {
            self.isolation_engine.stop_plugin(plugin_id, &container_id).await?;
        }

        // 通信ブローカーから登録解除
        self.communication_broker.unregister_plugin(plugin_id).await?;

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
        let plugin_arc = plugins.get(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin not found".to_string()))?
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
            self.isolation_engine.force_stop_plugin(plugin_id, &container_id).await?;
        }

        // セキュリティイベントをログ記録
        error!("Plugin {} quarantined due to security violation: {}", plugin_id, reason);
        
        // 監視システムにアラート送信
        self.monitoring.send_security_alert(plugin_id, &reason).await?;

        Ok(())
    }

    /// プラグインの状態を取得
    pub async fn get_plugin_state(&self, plugin_id: Uuid) -> Result<PluginState, McpError> {
        let plugins = self.plugins.read().await;
        let plugin_arc = plugins.get(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin not found".to_string()))?;
        
        let plugin = plugin_arc.lock().await;
        Ok(plugin.state)
    }

    /// プラグインのメトリクスを取得
    pub async fn get_plugin_metrics(&self, plugin_id: Uuid) -> Result<PluginMetrics, McpError> {
        let plugins = self.plugins.read().await;
        let plugin_arc = plugins.get(&plugin_id)
            .ok_or_else(|| McpError::PluginError("Plugin not found".to_string()))?;
        
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

/// プラグインマネージャーのヘルス状態
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginManagerHealth {
    /// 総プラグイン数
    pub total_plugins: usize,
    /// 実行中プラグイン数
    pub running_plugins: usize,
    /// エラー状態プラグイン数
    pub error_plugins: usize,
    /// 隔離状態プラグイン数
    pub quarantined_plugins: usize,
    /// システム全体の健全性
    pub system_health: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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
            resource_limits: ResourceLimits::default(),
            security_level: SecurityLevel::Standard,
            dependencies: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let result = manager.register_plugin(metadata).await;
        assert!(result.is_ok());
    }
}