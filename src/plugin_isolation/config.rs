//! Plugin Manager Configuration
//!
//! プラグインマネージャーの設定

use super::types::ResourceLimits;
use serde::{Deserialize, Serialize};

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
