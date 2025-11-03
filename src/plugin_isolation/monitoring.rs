//! プラグイン監視システム
//! 
//! プラグインのリアルタイム監視、ログ収集、アラート機能を提供
//! パフォーマンス指標、セキュリティイベント、リソース使用量を追跡

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex, watch, broadcast};
use tokio::time::{Duration, Instant, interval};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::error::McpError;
use crate::plugin_isolation::PluginState;

/// 監視システム
#[derive(Debug)]
pub struct MonitoringSystem {
    /// プラグインメトリクス
    plugin_metrics: Arc<RwLock<HashMap<Uuid, PluginMetrics>>>,
    /// システムメトリクス
    system_metrics: Arc<RwLock<SystemMetrics>>,
    /// ログコレクター
    log_collector: Arc<LogCollector>,
    /// アラートマネージャー
    alert_manager: Arc<AlertManager>,
    /// イベントストア
    event_store: Arc<EventStore>,
    /// 設定
    config: MonitoringConfig,
    /// 監視タスクハンドル
    monitoring_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    /// イベントブロードキャスター
    event_broadcaster: broadcast::Sender<MonitoringEvent>,
}

/// プラグインメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetrics {
    /// プラグインID
    pub plugin_id: Uuid,
    /// プラグイン名
    pub plugin_name: String,
    /// 現在のステート
    pub current_state: PluginState,
    /// CPU使用率（%）
    pub cpu_usage_percent: f64,
    /// メモリ使用量（MB）
    pub memory_usage_mb: f64,
    /// ディスク使用量（MB）
    pub disk_usage_mb: f64,
    /// ネットワークI/O（bytes/sec）
    pub network_io_bytes_per_sec: f64,
    /// 処理されたメッセージ数
    pub messages_processed: u64,
    /// エラー数
    pub error_count: u64,
    /// 平均レスポンス時間（ms）
    pub avg_response_time_ms: f64,
    /// アクティブ接続数
    pub active_connections: u32,
    /// 最終ハートビート時刻
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    /// 稼働時間（秒）
    pub uptime_seconds: u64,
    /// 詳細メトリクス
    pub detailed_metrics: DetailedMetrics,
}

/// 詳細メトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedMetrics {
    /// プロセス統計
    pub process_stats: ProcessStats,
    /// セキュリティ統計
    pub security_stats: SecurityStats,
    /// パフォーマンス統計
    pub performance_stats: PerformanceStats,
    /// カスタムメトリクス
    pub custom_metrics: HashMap<String, MetricValue>,
}

/// プロセス統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStats {
    /// プロセスID
    pub pid: u32,
    /// スレッド数
    pub thread_count: u32,
    /// ファイルディスクリプタ数
    pub file_descriptor_count: u32,
    /// 仮想メモリサイズ（MB）
    pub virtual_memory_mb: f64,
    /// 物理メモリサイズ（MB）
    pub physical_memory_mb: f64,
    /// スワップ使用量（MB）
    pub swap_usage_mb: f64,
    /// システムコール数
    pub syscall_count: u64,
    /// 最後のGC実行時間
    pub last_gc_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// セキュリティ統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityStats {
    /// 認証試行回数
    pub auth_attempts: u64,
    /// 認証失敗回数
    pub auth_failures: u64,
    /// 権限違反数
    pub permission_violations: u64,
    /// 不審なアクティビティ数
    pub suspicious_activities: u64,
    /// ファイルアクセス試行数
    pub file_access_attempts: u64,
    /// ネットワーク接続試行数
    pub network_connection_attempts: u64,
    /// セキュリティポリシー違反数
    pub policy_violations: u64,
    /// 最後のセキュリティイベント時刻
    pub last_security_event: Option<chrono::DateTime<chrono::Utc>>,
}

/// パフォーマンス統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    /// リクエスト処理統計
    pub request_stats: RequestStats,
    /// データベース統計
    pub database_stats: DatabaseStats,
    /// キャッシュ統計
    pub cache_stats: CacheStats,
    /// I/O統計
    pub io_stats: IoStats,
}

/// リクエスト処理統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStats {
    /// 総リクエスト数
    pub total_requests: u64,
    /// 成功リクエスト数
    pub successful_requests: u64,
    /// 失敗リクエスト数
    pub failed_requests: u64,
    /// 平均処理時間（ms）
    pub avg_processing_time_ms: f64,
    /// 95パーセンタイル処理時間（ms）
    pub p95_processing_time_ms: f64,
    /// 99パーセンタイル処理時間（ms）
    pub p99_processing_time_ms: f64,
    /// スループット（req/sec）
    pub throughput_per_sec: f64,
}

/// データベース統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    /// 総クエリ数
    pub total_queries: u64,
    /// 読み取りクエリ数
    pub read_queries: u64,
    /// 書き込みクエリ数
    pub write_queries: u64,
    /// 平均クエリ時間（ms）
    pub avg_query_time_ms: f64,
    /// 接続プールサイズ
    pub connection_pool_size: u32,
    /// アクティブ接続数
    pub active_connections: u32,
}

/// キャッシュ統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// キャッシュヒット数
    pub cache_hits: u64,
    /// キャッシュミス数
    pub cache_misses: u64,
    /// キャッシュヒット率（%）
    pub hit_ratio_percent: f64,
    /// キャッシュサイズ（MB）
    pub cache_size_mb: f64,
    /// エビクション数
    pub evictions: u64,
}

/// I/O統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoStats {
    /// 読み取りバイト数
    pub bytes_read: u64,
    /// 書き込みバイト数
    pub bytes_written: u64,
    /// 読み取り操作数
    pub read_operations: u64,
    /// 書き込み操作数
    pub write_operations: u64,
    /// 平均読み取り時間（ms）
    pub avg_read_time_ms: f64,
    /// 平均書き込み時間（ms）
    pub avg_write_time_ms: f64,
}

/// メトリクス値
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    /// 整数値
    Integer(i64),
    /// 浮動小数点値
    Float(f64),
    /// 文字列値
    String(String),
    /// ブール値
    Boolean(bool),
    /// 配列値
    Array(Vec<MetricValue>),
    /// オブジェクト値
    Object(HashMap<String, MetricValue>),
}

/// システムメトリクス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// 総CPU使用率（%）
    pub total_cpu_usage_percent: f64,
    /// 総メモリ使用量（MB）
    pub total_memory_usage_mb: f64,
    /// 総ディスク使用量（MB）
    pub total_disk_usage_mb: f64,
    /// 総ネットワークI/O（bytes/sec）
    pub total_network_io_bytes_per_sec: f64,
    /// アクティブプラグイン数
    pub active_plugin_count: u32,
    /// システム稼働時間（秒）
    pub system_uptime_seconds: u64,
    /// システム負荷平均
    pub load_average: [f64; 3], // 1分、5分、15分
    /// 使用可能メモリ（MB）
    pub available_memory_mb: f64,
    /// 使用可能ディスク容量（MB）
    pub available_disk_mb: f64,
}

/// ログコレクター
#[derive(Debug)]
pub struct LogCollector {
    /// ログエントリ
    log_entries: Arc<RwLock<VecDeque<LogEntry>>>,
    /// ログ送信者
    log_sender: Arc<Mutex<watch::Sender<LogEntry>>>,
    /// ログレベルフィルター
    level_filter: LogLevel,
    /// 最大ログエントリ数
    max_entries: usize,
    /// ログローテーション設定
    rotation_config: LogRotationConfig,
}

/// ログエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// ログID
    pub log_id: Uuid,
    /// プラグインID
    pub plugin_id: Option<Uuid>,
    /// ログレベル
    pub level: LogLevel,
    /// メッセージ
    pub message: String,
    /// ソースファイル
    pub source_file: Option<String>,
    /// 行番号
    pub line_number: Option<u32>,
    /// スレッドID
    pub thread_id: Option<String>,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 追加フィールド
    pub fields: HashMap<String, String>,
    /// スタックトレース
    pub stack_trace: Option<String>,
}

/// ログレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    /// トレース
    Trace = 0,
    /// デバッグ
    Debug = 1,
    /// 情報
    Info = 2,
    /// 警告
    Warn = 3,
    /// エラー
    Error = 4,
    /// クリティカル
    Critical = 5,
}

/// ログローテーション設定
#[derive(Debug, Clone)]
pub struct LogRotationConfig {
    /// 最大ファイルサイズ（MB）
    pub max_file_size_mb: u64,
    /// 最大ファイル数
    pub max_file_count: u32,
    /// ローテーション間隔（時間）
    pub rotation_interval_hours: u64,
    /// 圧縮有効/無効
    pub compression_enabled: bool,
}

/// アラートマネージャー
#[derive(Debug)]
pub struct AlertManager {
    /// アラートルール
    alert_rules: Arc<RwLock<Vec<AlertRule>>>,
    /// アクティブアラート
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
    /// アラート送信者
    alert_senders: Arc<RwLock<Vec<Box<dyn AlertSender>>>>,
    /// アラート履歴
    alert_history: Arc<RwLock<VecDeque<Alert>>>,
    /// 設定
    config: AlertConfig,
}

/// アラートルール
#[derive(Debug, Clone)]
pub struct AlertRule {
    /// ルールID
    pub rule_id: String,
    /// ルール名
    pub name: String,
    /// 説明
    pub description: String,
    /// 条件
    pub condition: AlertCondition,
    /// 重要度
    pub severity: AlertSeverity,
    /// アクション
    pub actions: Vec<AlertAction>,
    /// 有効/無効
    pub enabled: bool,
    /// 冷却時間（秒）
    pub cooldown_seconds: u64,
}

/// アラート条件
#[derive(Debug, Clone)]
pub enum AlertCondition {
    /// メトリクスしきい値
    MetricThreshold {
        metric_name: String,
        operator: ComparisonOperator,
        threshold: f64,
        duration_seconds: u64,
    },
    /// ログパターンマッチ
    LogPattern {
        pattern: String,
        level: LogLevel,
        count_threshold: u32,
        time_window_seconds: u64,
    },
    /// プラグイン状態変更
    PluginStateChange {
        plugin_id: Option<Uuid>,
        from_state: Option<PluginState>,
        to_state: PluginState,
    },
    /// カスタム条件
    Custom {
        condition_script: String,
        parameters: HashMap<String, String>,
    },
}

/// 比較演算子
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    /// より大きい
    GreaterThan,
    /// 以上
    GreaterThanOrEqual,
    /// より小さい
    LessThan,
    /// 以下
    LessThanOrEqual,
    /// 等しい
    Equal,
    /// 等しくない
    NotEqual,
}

/// アラート重要度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// 情報
    Info = 1,
    /// 警告
    Warning = 2,
    /// エラー
    Error = 3,
    /// クリティカル
    Critical = 4,
    /// 緊急
    Emergency = 5,
}

/// アラートアクション
#[derive(Debug, Clone)]
pub enum AlertAction {
    /// メール送信
    SendEmail {
        recipients: Vec<String>,
        subject_template: String,
        body_template: String,
    },
    /// Slack通知
    SendSlack {
        webhook_url: String,
        channel: String,
        message_template: String,
    },
    /// Webhook呼び出し
    CallWebhook {
        url: String,
        method: String,
        headers: HashMap<String, String>,
        body_template: String,
    },
    /// プラグイン停止
    StopPlugin {
        plugin_id: Uuid,
        force: bool,
    },
    /// プラグイン再起動
    RestartPlugin {
        plugin_id: Uuid,
        delay_seconds: u64,
    },
    /// カスタムアクション
    Custom {
        action_script: String,
        parameters: HashMap<String, String>,
    },
}

/// アラート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// アラートID
    pub alert_id: Uuid,
    /// ルールID
    pub rule_id: String,
    /// アラート名
    pub name: String,
    /// 説明
    pub description: String,
    /// 重要度
    pub severity: AlertSeverity,
    /// ステータス
    pub status: AlertStatus,
    /// 発生時刻
    pub triggered_at: chrono::DateTime<chrono::Utc>,
    /// 解決時刻
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 関連プラグインID
    pub plugin_id: Option<Uuid>,
    /// 詳細情報
    pub details: HashMap<String, String>,
    /// 通知履歴
    pub notification_history: Vec<NotificationRecord>,
}

/// アラートステータス
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertStatus {
    /// アクティブ
    Active,
    /// 確認済み
    Acknowledged,
    /// 解決済み
    Resolved,
    /// 抑制中
    Suppressed,
}

/// 通知記録
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    /// 通知タイプ
    pub notification_type: String,
    /// 送信先
    pub recipient: String,
    /// 送信時刻
    pub sent_at: chrono::DateTime<chrono::Utc>,
    /// 送信結果
    pub success: bool,
    /// エラーメッセージ
    pub error_message: Option<String>,
}

/// アラート送信者トレイト
pub trait AlertSender: Send + Sync + std::fmt::Debug {
    fn send_alert(&self, alert: &Alert, action: &AlertAction) -> Result<(), McpError>;
    fn get_sender_type(&self) -> String;
}

/// アラート設定
#[derive(Debug, Clone)]
pub struct AlertConfig {
    /// 最大アクティブアラート数
    pub max_active_alerts: usize,
    /// アラート履歴保持日数
    pub alert_history_days: u32,
    /// デフォルト冷却時間（秒）
    pub default_cooldown_seconds: u64,
    /// バッチ送信有効/無効
    pub batch_notifications: bool,
    /// バッチサイズ
    pub batch_size: usize,
    /// バッチ間隔（秒）
    pub batch_interval_seconds: u64,
}

/// イベントストア
#[derive(Debug)]
pub struct EventStore {
    /// イベント
    events: Arc<RwLock<VecDeque<MonitoringEvent>>>,
    /// 最大イベント数
    max_events: usize,
    /// インデックス
    indices: Arc<RwLock<HashMap<String, Vec<usize>>>>,
}

/// 監視イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringEvent {
    /// イベントID
    pub event_id: Uuid,
    /// イベントタイプ
    pub event_type: MonitoringEventType,
    /// プラグインID
    pub plugin_id: Option<Uuid>,
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// データ
    pub data: HashMap<String, MetricValue>,
    /// 重要度
    pub severity: EventSeverity,
    /// 説明
    pub description: String,
    /// タグ
    pub tags: Vec<String>,
}

/// 監視イベントタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitoringEventType {
    /// プラグイン状態変更
    PluginStateChange,
    /// メトリクス更新
    MetricsUpdate,
    /// セキュリティイベント
    SecurityEvent,
    /// パフォーマンス警告
    PerformanceWarning,
    /// リソース不足
    ResourceExhaustion,
    /// システムイベント
    SystemEvent,
    /// カスタムイベント
    Custom(String),
}

/// イベント重要度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventSeverity {
    /// 低
    Low = 1,
    /// 中
    Medium = 2,
    /// 高
    High = 3,
    /// クリティカル
    Critical = 4,
}

/// 監視設定
#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    /// メトリクス収集間隔（秒）
    pub metrics_collection_interval_secs: u64,
    /// ログ収集設定
    pub log_collection: LogCollectionConfig,
    /// アラート設定
    pub alert_config: AlertConfig,
    /// イベントストア設定
    pub event_store_config: EventStoreConfig,
    /// リソース監視設定
    pub resource_monitoring: ResourceMonitoringConfig,
}

/// ログ収集設定
#[derive(Debug, Clone)]
pub struct LogCollectionConfig {
    /// 最大ログエントリ数
    pub max_log_entries: usize,
    /// ログレベルフィルター
    pub level_filter: LogLevel,
    /// ローテーション設定
    pub rotation_config: LogRotationConfig,
    /// リアルタイム送信有効/無効
    pub realtime_streaming: bool,
}

/// イベントストア設定
#[derive(Debug, Clone)]
pub struct EventStoreConfig {
    /// 最大イベント数
    pub max_events: usize,
    /// インデックス有効/無効
    pub indexing_enabled: bool,
    /// 圧縮有効/無効
    pub compression_enabled: bool,
    /// 保持期間（日）
    pub retention_days: u32,
}

/// リソース監視設定
#[derive(Debug, Clone)]
pub struct ResourceMonitoringConfig {
    /// CPU監視有効/無効
    pub cpu_monitoring_enabled: bool,
    /// メモリ監視有効/無効
    pub memory_monitoring_enabled: bool,
    /// ディスク監視有効/無効
    pub disk_monitoring_enabled: bool,
    /// ネットワーク監視有効/無効
    pub network_monitoring_enabled: bool,
    /// プロセス監視有効/無効
    pub process_monitoring_enabled: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            metrics_collection_interval_secs: 30,
            log_collection: LogCollectionConfig {
                max_log_entries: 100000,
                level_filter: LogLevel::Info,
                rotation_config: LogRotationConfig {
                    max_file_size_mb: 100,
                    max_file_count: 10,
                    rotation_interval_hours: 24,
                    compression_enabled: true,
                },
                realtime_streaming: true,
            },
            alert_config: AlertConfig {
                max_active_alerts: 1000,
                alert_history_days: 30,
                default_cooldown_seconds: 300,
                batch_notifications: true,
                batch_size: 10,
                batch_interval_seconds: 60,
            },
            event_store_config: EventStoreConfig {
                max_events: 1000000,
                indexing_enabled: true,
                compression_enabled: true,
                retention_days: 90,
            },
            resource_monitoring: ResourceMonitoringConfig {
                cpu_monitoring_enabled: true,
                memory_monitoring_enabled: true,
                disk_monitoring_enabled: true,
                network_monitoring_enabled: true,
                process_monitoring_enabled: true,
            },
        }
    }
}

impl MonitoringSystem {
    /// 新しい監視システムを作成
    pub async fn new() -> Result<Self, McpError> {
        Self::new_with_config(MonitoringConfig::default()).await
    }

    /// 設定付きで監視システムを作成
    pub async fn new_with_config(config: MonitoringConfig) -> Result<Self, McpError> {
        info!("Initializing monitoring system");

        let log_collector = Arc::new(LogCollector::new(config.log_collection.clone()).await?);
        let alert_manager = Arc::new(AlertManager::new(config.alert_config.clone()).await?);
        let event_store = Arc::new(EventStore::new(config.event_store_config.clone()).await?);

        let (event_broadcaster, _) = broadcast::channel(1000);

        Ok(Self {
            plugin_metrics: Arc::new(RwLock::new(HashMap::new())),
            system_metrics: Arc::new(RwLock::new(SystemMetrics::default())),
            log_collector,
            alert_manager,
            event_store,
            config,
            monitoring_handles: Arc::new(Mutex::new(Vec::new())),
            event_broadcaster,
        })
    }

    /// 監視を開始
    pub async fn start_monitoring(&self) -> Result<(), McpError> {
        info!("Starting monitoring system");

        let mut handles = self.monitoring_handles.lock().await;

        // メトリクス収集タスクを開始
        let metrics_handle = self.start_metrics_collection().await?;
        handles.push(metrics_handle);

        // アラート処理タスクを開始
        let alert_handle = self.start_alert_processing().await?;
        handles.push(alert_handle);

        // イベント処理タスクを開始
        let event_handle = self.start_event_processing().await?;
        handles.push(event_handle);

        info!("Monitoring system started");
        Ok(())
    }

    /// プラグインを監視対象に追加
    pub async fn add_plugin(&self, plugin_id: Uuid, plugin_name: String) -> Result<(), McpError> {
        info!("Adding plugin to monitoring: {} ({})", plugin_name, plugin_id);

        let metrics = PluginMetrics {
            plugin_id,
            plugin_name: plugin_name.clone(),
            current_state: PluginState::Uninitialized,
            cpu_usage_percent: 0.0,
            memory_usage_mb: 0.0,
            disk_usage_mb: 0.0,
            network_io_bytes_per_sec: 0.0,
            messages_processed: 0,
            error_count: 0,
            avg_response_time_ms: 0.0,
            active_connections: 0,
            last_heartbeat: chrono::Utc::now(),
            uptime_seconds: 0,
            detailed_metrics: DetailedMetrics::default(),
        };

        let mut plugin_metrics = self.plugin_metrics.write().await;
        plugin_metrics.insert(plugin_id, metrics);

        // 監視イベントを送信
        let event = MonitoringEvent {
            event_id: Uuid::new_v4(),
            event_type: MonitoringEventType::PluginStateChange,
            plugin_id: Some(plugin_id),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
            severity: EventSeverity::Low,
            description: format!("Plugin {} added to monitoring", plugin_name),
            tags: vec!["plugin".to_string(), "monitoring".to_string()],
        };

        self.send_event(event).await?;

        info!("Plugin added to monitoring: {}", plugin_name);
        Ok(())
    }

    /// プラグインを監視対象から削除
    pub async fn remove_plugin(&self, plugin_id: Uuid) -> Result<(), McpError> {
        info!("Removing plugin from monitoring: {}", plugin_id);

        let mut plugin_metrics = self.plugin_metrics.write().await;
        let plugin_name = plugin_metrics
            .get(&plugin_id)
            .map(|m| m.plugin_name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        
        plugin_metrics.remove(&plugin_id);

        // 監視イベントを送信
        let event = MonitoringEvent {
            event_id: Uuid::new_v4(),
            event_type: MonitoringEventType::PluginStateChange,
            plugin_id: Some(plugin_id),
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
            severity: EventSeverity::Low,
            description: format!("Plugin {} removed from monitoring", plugin_name),
            tags: vec!["plugin".to_string(), "monitoring".to_string()],
        };

        self.send_event(event).await?;

        info!("Plugin removed from monitoring: {}", plugin_name);
        Ok(())
    }

    /// プラグインメトリクスを更新
    pub async fn update_plugin_metrics(&self, plugin_id: Uuid, metrics: PluginMetrics) -> Result<(), McpError> {
        let mut plugin_metrics = self.plugin_metrics.write().await;
        if let Some(existing_metrics) = plugin_metrics.get_mut(&plugin_id) {
            *existing_metrics = metrics;
            
            // メトリクス更新イベントを送信
            let event = MonitoringEvent {
                event_id: Uuid::new_v4(),
                event_type: MonitoringEventType::MetricsUpdate,
                plugin_id: Some(plugin_id),
                timestamp: chrono::Utc::now(),
                data: HashMap::new(),
                severity: EventSeverity::Low,
                description: "Plugin metrics updated".to_string(),
                tags: vec!["metrics".to_string(), "update".to_string()],
            };

            self.send_event(event).await?;
        }

        Ok(())
    }

    /// プラグインメトリクスを取得
    pub async fn get_plugin_metrics(&self, plugin_id: Uuid) -> Result<Option<PluginMetrics>, McpError> {
        let plugin_metrics = self.plugin_metrics.read().await;
        Ok(plugin_metrics.get(&plugin_id).cloned())
    }

    /// 全プラグインメトリクスを取得
    pub async fn get_all_plugin_metrics(&self) -> Result<HashMap<Uuid, PluginMetrics>, McpError> {
        let plugin_metrics = self.plugin_metrics.read().await;
        Ok(plugin_metrics.clone())
    }

    /// システムメトリクスを更新
    pub async fn update_system_metrics(&self, metrics: SystemMetrics) -> Result<(), McpError> {
        let mut system_metrics = self.system_metrics.write().await;
        *system_metrics = metrics;

        // システムメトリクス更新イベントを送信
        let event = MonitoringEvent {
            event_id: Uuid::new_v4(),
            event_type: MonitoringEventType::SystemEvent,
            plugin_id: None,
            timestamp: chrono::Utc::now(),
            data: HashMap::new(),
            severity: EventSeverity::Low,
            description: "System metrics updated".to_string(),
            tags: vec!["system".to_string(), "metrics".to_string()],
        };

        self.send_event(event).await?;
        Ok(())
    }

    /// システムメトリクスを取得
    pub async fn get_system_metrics(&self) -> Result<SystemMetrics, McpError> {
        let system_metrics = self.system_metrics.read().await;
        Ok(system_metrics.clone())
    }

    /// ログエントリを追加
    pub async fn log(&self, entry: LogEntry) -> Result<(), McpError> {
        self.log_collector.add_log_entry(entry).await
    }

    /// イベントを送信
    pub async fn send_event(&self, event: MonitoringEvent) -> Result<(), McpError> {
        // イベントストアに保存
        self.event_store.add_event(event.clone()).await?;

        // イベントをブロードキャスト
        if let Err(e) = self.event_broadcaster.send(event) {
            debug!("Failed to broadcast event: {}", e);
        }

        Ok(())
    }

    /// イベント購読者を取得
    pub fn subscribe_events(&self) -> broadcast::Receiver<MonitoringEvent> {
        self.event_broadcaster.subscribe()
    }

    /// 監視システムをシャットダウン
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down monitoring system");

        // 監視タスクを停止
        let mut handles = self.monitoring_handles.lock().await;
        for handle in handles.drain(..) {
            handle.abort();
        }

        // アラートマネージャーをシャットダウン
        self.alert_manager.shutdown().await?;

        // ログコレクターをシャットダウン
        self.log_collector.shutdown().await?;

        info!("Monitoring system shutdown completed");
        Ok(())
    }

    /// メトリクス収集タスクを開始
    async fn start_metrics_collection(&self) -> Result<tokio::task::JoinHandle<()>, McpError> {
        let collection_interval = Duration::from_secs(self.config.metrics_collection_interval_secs);
        let plugin_metrics = Arc::clone(&self.plugin_metrics);
        let system_metrics = Arc::clone(&self.system_metrics);

        let handle = tokio::spawn(async move {
            let mut interval = interval(collection_interval);
            
            loop {
                interval.tick().await;
                
                // システムメトリクスを収集
                if let Ok(metrics) = Self::collect_system_metrics().await {
                    let mut system_metrics = system_metrics.write().await;
                    *system_metrics = metrics;
                }

                // プラグインメトリクスを収集
                let mut plugin_metrics = plugin_metrics.write().await;
                for (plugin_id, metrics) in plugin_metrics.iter_mut() {
                    if let Ok(updated_metrics) = Self::collect_plugin_metrics(*plugin_id).await {
                        *metrics = updated_metrics;
                    }
                }
            }
        });

        Ok(handle)
    }

    /// アラート処理タスクを開始
    async fn start_alert_processing(&self) -> Result<tokio::task::JoinHandle<()>, McpError> {
        let alert_manager = Arc::clone(&self.alert_manager);
        let plugin_metrics = Arc::clone(&self.plugin_metrics);

        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                // アラート条件をチェック
                let plugin_metrics = plugin_metrics.read().await;
                for (plugin_id, metrics) in plugin_metrics.iter() {
                    if let Err(e) = alert_manager.check_alert_conditions(*plugin_id, metrics).await {
                        error!("Failed to check alert conditions for plugin {}: {}", plugin_id, e);
                    }
                }
            }
        });

        Ok(handle)
    }

    /// イベント処理タスクを開始
    async fn start_event_processing(&self) -> Result<tokio::task::JoinHandle<()>, McpError> {
        let event_store = Arc::clone(&self.event_store);

        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                // 古いイベントをクリーンアップ
                if let Err(e) = event_store.cleanup_old_events().await {
                    error!("Failed to cleanup old events: {}", e);
                }
            }
        });

        Ok(handle)
    }

    /// システムメトリクスを収集
    async fn collect_system_metrics() -> Result<SystemMetrics, McpError> {
        // TODO: 実際のシステムメトリクス収集を実装
        Ok(SystemMetrics::default())
    }

    /// プラグインメトリクスを収集
    async fn collect_plugin_metrics(_plugin_id: Uuid) -> Result<PluginMetrics, McpError> {
        // TODO: 実際のプラグインメトリクス収集を実装
        Err(McpError::NotImplemented("Plugin metrics collection not implemented".to_string()))
    }
}

// 他のコンポーネントの実装

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            total_cpu_usage_percent: 0.0,
            total_memory_usage_mb: 0.0,
            total_disk_usage_mb: 0.0,
            total_network_io_bytes_per_sec: 0.0,
            active_plugin_count: 0,
            system_uptime_seconds: 0,
            load_average: [0.0, 0.0, 0.0],
            available_memory_mb: 0.0,
            available_disk_mb: 0.0,
        }
    }
}

impl Default for DetailedMetrics {
    fn default() -> Self {
        Self {
            process_stats: ProcessStats::default(),
            security_stats: SecurityStats::default(),
            performance_stats: PerformanceStats::default(),
            custom_metrics: HashMap::new(),
        }
    }
}

impl Default for ProcessStats {
    fn default() -> Self {
        Self {
            pid: 0,
            thread_count: 0,
            file_descriptor_count: 0,
            virtual_memory_mb: 0.0,
            physical_memory_mb: 0.0,
            swap_usage_mb: 0.0,
            syscall_count: 0,
            last_gc_time: None,
        }
    }
}

impl Default for SecurityStats {
    fn default() -> Self {
        Self {
            auth_attempts: 0,
            auth_failures: 0,
            permission_violations: 0,
            suspicious_activities: 0,
            file_access_attempts: 0,
            network_connection_attempts: 0,
            policy_violations: 0,
            last_security_event: None,
        }
    }
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            request_stats: RequestStats::default(),
            database_stats: DatabaseStats::default(),
            cache_stats: CacheStats::default(),
            io_stats: IoStats::default(),
        }
    }
}

impl Default for RequestStats {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            avg_processing_time_ms: 0.0,
            p95_processing_time_ms: 0.0,
            p99_processing_time_ms: 0.0,
            throughput_per_sec: 0.0,
        }
    }
}

impl Default for DatabaseStats {
    fn default() -> Self {
        Self {
            total_queries: 0,
            read_queries: 0,
            write_queries: 0,
            avg_query_time_ms: 0.0,
            connection_pool_size: 0,
            active_connections: 0,
        }
    }
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            cache_hits: 0,
            cache_misses: 0,
            hit_ratio_percent: 0.0,
            cache_size_mb: 0.0,
            evictions: 0,
        }
    }
}

impl Default for IoStats {
    fn default() -> Self {
        Self {
            bytes_read: 0,
            bytes_written: 0,
            read_operations: 0,
            write_operations: 0,
            avg_read_time_ms: 0.0,
            avg_write_time_ms: 0.0,
        }
    }
}

impl LogCollector {
    async fn new(_config: LogCollectionConfig) -> Result<Self, McpError> {
        let (sender, _) = watch::channel(LogEntry {
            log_id: Uuid::new_v4(),
            plugin_id: None,
            level: LogLevel::Info,
            message: "Log collector initialized".to_string(),
            source_file: None,
            line_number: None,
            thread_id: None,
            timestamp: chrono::Utc::now(),
            fields: HashMap::new(),
            stack_trace: None,
        });

        Ok(Self {
            log_entries: Arc::new(RwLock::new(VecDeque::new())),
            log_sender: Arc::new(Mutex::new(sender)),
            level_filter: LogLevel::Info,
            max_entries: 100000,
            rotation_config: LogRotationConfig {
                max_file_size_mb: 100,
                max_file_count: 10,
                rotation_interval_hours: 24,
                compression_enabled: true,
            },
        })
    }

    async fn add_log_entry(&self, entry: LogEntry) -> Result<(), McpError> {
        if entry.level >= self.level_filter {
            let mut entries = self.log_entries.write().await;
            
            // 最大エントリ数をチェック
            if entries.len() >= self.max_entries {
                entries.pop_front();
            }
            
            entries.push_back(entry.clone());
            
            // リアルタイム送信
            let sender = self.log_sender.lock().await;
            let _ = sender.send(entry);
        }
        
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        // TODO: ログファイルのフラッシュなど
        Ok(())
    }
}

impl AlertManager {
    async fn new(_config: AlertConfig) -> Result<Self, McpError> {
        Ok(Self {
            alert_rules: Arc::new(RwLock::new(Vec::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_senders: Arc::new(RwLock::new(Vec::new())),
            alert_history: Arc::new(RwLock::new(VecDeque::new())),
            config: _config,
        })
    }

    async fn check_alert_conditions(&self, _plugin_id: Uuid, _metrics: &PluginMetrics) -> Result<(), McpError> {
        // TODO: アラート条件のチェック実装
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        // TODO: 実装
        Ok(())
    }
}

impl EventStore {
    async fn new(config: EventStoreConfig) -> Result<Self, McpError> {
        Ok(Self {
            events: Arc::new(RwLock::new(VecDeque::new())),
            max_events: config.max_events,
            indices: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    async fn add_event(&self, event: MonitoringEvent) -> Result<(), McpError> {
        let mut events = self.events.write().await;
        
        // 最大イベント数をチェック
        if events.len() >= self.max_events {
            events.pop_front();
        }
        
        events.push_back(event);
        Ok(())
    }

    async fn cleanup_old_events(&self) -> Result<(), McpError> {
        // TODO: 古いイベントのクリーンアップ実装
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitoring_system_creation() {
        let monitoring = MonitoringSystem::new().await;
        assert!(monitoring.is_ok());
    }

    #[tokio::test]
    async fn test_plugin_monitoring() {
        let monitoring = MonitoringSystem::new().await.unwrap();
        let plugin_id = Uuid::new_v4();
        
        let result = monitoring.add_plugin(plugin_id, "test_plugin".to_string()).await;
        assert!(result.is_ok());
        
        let metrics = monitoring.get_plugin_metrics(plugin_id).await.unwrap();
        assert!(metrics.is_some());
    }

    #[test]
    fn test_log_levels() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Critical > LogLevel::Error);
    }

    #[test]
    fn test_alert_severity() {
        assert!(AlertSeverity::Critical > AlertSeverity::Warning);
        assert!(AlertSeverity::Emergency > AlertSeverity::Critical);
    }
}