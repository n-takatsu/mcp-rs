//! Plugin Isolation Types
//!
//! プラグイン隔離システム用の型定義

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::plugin_isolation::security_validation::SecurityLevel;

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
