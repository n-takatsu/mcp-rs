use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::canary_deployment::{
    CanaryDeploymentManager, DeploymentState, 
    TrafficSplit
};
use crate::policy_config::PolicyConfig;
use crate::error::McpError;

/// ロールバック管理システムのメイン構造体
#[derive(Debug)]
pub struct RollbackManager {
    /// デプロイメント履歴
    pub deployment_history: Arc<RwLock<VecDeque<DeploymentSnapshot>>>,
    /// ロールバック設定
    pub rollback_config: Arc<RwLock<RollbackConfig>>,
    /// メトリクス監視
    pub metrics_monitor: Arc<RwLock<MetricsMonitor>>,
    /// イベント通知
    pub event_sender: broadcast::Sender<RollbackEvent>,
    /// ロールバック実行器
    pub executor: Arc<RollbackExecutor>,
    /// ロールバックメトリクス
    pub rollback_metrics: Arc<RwLock<RollbackMetrics>>,
}

/// デプロイメントの完全なスナップショット
#[derive(Debug, Clone)]
pub struct DeploymentSnapshot {
    /// 一意のスナップショットID
    pub id: String,
    /// 作成時刻
    pub timestamp: DateTime<Utc>,
    /// 安定版ポリシー
    pub stable_policy: PolicyConfig,
    /// カナリア版ポリシー（存在する場合）
    pub canary_policy: Option<PolicyConfig>,
    /// トラフィック分散状態
    pub traffic_split: TrafficSplit,
    /// メトリクス状態
    pub metrics: MetricsSnapshot,
    /// デプロイメント状態
    pub deployment_state: DeploymentState,
    /// 追加メタデータ
    pub metadata: HashMap<String, String>,
    /// スナップショット作成理由
    pub creation_reason: SnapshotCreationReason,
}

/// スナップショット作成理由
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotCreationReason {
    /// デプロイメント開始時
    DeploymentStart,
    /// 定期的なバックアップ
    ScheduledBackup,
    /// 手動作成
    Manual { created_by: String },
    /// ロールバック前
    PreRollback,
    /// 異常検知時
    AnomalyDetected { reason: String },
}

/// ロールバック設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackConfig {
    /// 自動ロールバック有効化
    pub auto_rollback_enabled: bool,
    /// エラー率閾値（パーセント）
    pub error_rate_threshold: f64,
    /// レスポンス時間閾値（ミリ秒）
    pub response_time_threshold_ms: u64,
    /// 評価期間（分）
    pub evaluation_window_minutes: u32,
    /// 段階的ロールバック設定
    pub staged_rollback: StagedRollbackConfig,
    /// 保存するスナップショット数の上限
    pub max_snapshots: usize,
    /// ロールバック実行前の確認時間（秒）
    pub confirmation_timeout_seconds: u32,
    /// カスタムロールバック条件
    pub custom_conditions: Vec<CustomRollbackCondition>,
}

/// 段階的ロールバック設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedRollbackConfig {
    /// 段階設定
    pub stages: Vec<RollbackStage>,
    /// 各段階間の待機時間（秒）
    pub stage_interval_seconds: u32,
    /// 段階間での評価を有効化
    pub evaluate_between_stages: bool,
    /// 段階的ロールバックの最大時間（秒）
    pub max_total_duration_seconds: u32,
}

/// ロールバック段階
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackStage {
    /// ステージ名
    pub name: String,
    /// 目標トラフィック割合
    pub target_percentage: f32,
    /// このステージの最大時間（秒）
    pub max_duration_seconds: u32,
    /// 成功条件
    pub success_criteria: Vec<SuccessCriteria>,
}

/// 成功条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriteria {
    /// 条件名
    pub name: String,
    /// メトリクス名
    pub metric_name: String,
    /// 比較演算子
    pub operator: ComparisonOperator,
    /// 閾値
    pub threshold: f64,
    /// 必須条件かどうか
    pub required: bool,
}

/// 比較演算子
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
    NotEqual,
}

/// カスタムロールバック条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRollbackCondition {
    /// 条件名
    pub name: String,
    /// 条件の説明
    pub description: String,
    /// メトリクス名
    pub metric_name: String,
    /// 条件式
    pub condition_expression: String,
    /// 有効化フラグ
    pub enabled: bool,
}

/// メトリクススナップショット
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// スナップショット時刻
    pub timestamp: DateTime<Utc>,
    /// 安定版メトリクス
    pub stable_metrics: PolicyMetrics,
    /// カナリア版メトリクス
    pub canary_metrics: PolicyMetrics,
    /// システム全体のメトリクス
    pub system_metrics: SystemMetrics,
    /// カスタムメトリクス
    pub custom_metrics: HashMap<String, f64>,
}

/// ポリシーメトリクス
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub error_requests: u64,
    pub avg_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub p99_response_time_ms: f64,
    pub error_rate: f64,
    pub throughput_rps: f64,
}

/// システムメトリクス
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub disk_usage_percent: f64,
    pub network_io_bytes_per_sec: f64,
    pub active_connections: u64,
}

/// メトリクス監視システム
#[derive(Debug)]
pub struct MetricsMonitor {
    /// 現在のメトリクス
    pub current_metrics: MetricsSnapshot,
    /// メトリクス履歴
    pub metrics_history: VecDeque<MetricsSnapshot>,
    /// 監視間隔（秒）
    pub monitoring_interval_seconds: u32,
    /// 異常検知設定
    pub anomaly_detection_config: AnomalyDetectionConfig,
}

/// 異常検知設定
#[derive(Debug, Clone)]
pub struct AnomalyDetectionConfig {
    /// 異常検知有効化
    pub enabled: bool,
    /// 統計的異常検知の閾値（標準偏差の倍数）
    pub statistical_threshold: f64,
    /// 移動平均ウィンドウサイズ
    pub moving_average_window: usize,
    /// 季節性調整
    pub seasonal_adjustment: bool,
}

/// ロールバック実行器
#[derive(Debug)]
pub struct RollbackExecutor {
    /// 実行中のロールバック
    pub active_rollbacks: Arc<RwLock<HashMap<String, ActiveRollback>>>,
    /// 実行設定
    pub execution_config: RollbackExecutionConfig,
}

/// アクティブなロールバック
#[derive(Debug, Clone)]
pub struct ActiveRollback {
    /// ロールバックID
    pub id: String,
    /// 対象スナップショット
    pub target_snapshot: DeploymentSnapshot,
    /// 開始時刻
    pub start_time: DateTime<Utc>,
    /// 現在の段階
    pub current_stage: usize,
    /// 進行状況（0.0-1.0）
    pub progress: f32,
    /// ロールバック種類
    pub rollback_type: RollbackType,
    /// 実行者
    pub executor: String,
}

/// ロールバック種類
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackType {
    /// 自動ロールバック
    Automatic { trigger_reason: String },
    /// 手動ロールバック
    Manual { initiated_by: String },
    /// 緊急ロールバック
    Emergency { reason: String },
    /// 段階的ロールバック
    Staged { stages: Vec<RollbackStage> },
}

/// ロールバック実行設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackExecutionConfig {
    /// 並行実行の最大数
    pub max_concurrent_rollbacks: usize,
    /// デフォルトタイムアウト（秒）
    pub default_timeout_seconds: u32,
    /// 再試行回数
    pub retry_attempts: u32,
    /// 再試行間隔（秒）
    pub retry_interval_seconds: u32,
    /// ドライラン有効化
    pub dry_run_enabled: bool,
}

/// ロールバックイベント
#[derive(Debug, Clone)]
pub enum RollbackEvent {
    /// 自動ロールバック開始
    AutoRollbackTriggered {
        rollback_id: String,
        reason: String,
        snapshot_id: String,
        trigger_metrics: MetricsSnapshot,
    },
    /// 手動ロールバック開始
    ManualRollbackInitiated {
        rollback_id: String,
        initiated_by: String,
        target_snapshot_id: String,
        reason: String,
    },
    /// ロールバック進行状況
    RollbackProgress {
        rollback_id: String,
        stage_name: String,
        progress_percentage: f32,
        current_metrics: MetricsSnapshot,
    },
    /// ロールバック完了
    RollbackCompleted {
        rollback_id: String,
        snapshot_id: String,
        duration_ms: u64,
        final_state: DeploymentState,
        success_metrics: MetricsSnapshot,
    },
    /// ロールバック失敗
    RollbackFailed {
        rollback_id: String,
        snapshot_id: String,
        error_message: String,
        partial_completion_percentage: f32,
        error_metrics: Option<MetricsSnapshot>,
    },
    /// スナップショット作成
    SnapshotCreated {
        snapshot_id: String,
        creation_reason: SnapshotCreationReason,
        metrics: MetricsSnapshot,
    },
    /// 異常検知
    AnomalyDetected {
        anomaly_type: String,
        severity: AnomalySeverity,
        metrics: MetricsSnapshot,
        recommended_action: String,
    },
}

/// 異常の深刻度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// ロールバックメトリクス
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RollbackMetrics {
    /// 総ロールバック回数
    pub total_rollbacks: u64,
    /// 自動ロールバック回数
    pub auto_rollbacks: u64,
    /// 手動ロールバック回数
    pub manual_rollbacks: u64,
    /// 成功したロールバック回数
    pub successful_rollbacks: u64,
    /// 失敗したロールバック回数
    pub failed_rollbacks: u64,
    /// 平均ロールバック時間（ミリ秒）
    pub avg_rollback_duration_ms: f64,
    /// 最後のロールバック時刻
    pub last_rollback_time: Option<DateTime<Utc>>,
    /// ロールバック成功率
    pub success_rate: f64,
    /// 平均異常検知時間（秒）
    pub avg_detection_time_seconds: f64,
}