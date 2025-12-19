//! 高度なエラーハンドリングシステム
//!
//! プラグイン実行時のエラーを検出、分類、回復する

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::McpError;
use crate::plugin_isolation::PluginState;

/// エラーコールバック型（Debugトレイトなし）
type ErrorCallback = Arc<dyn Fn(&PluginError) -> Result<(), McpError> + Send + Sync>;

/// エラーハンドラー
pub struct PluginErrorHandler {
    /// エラー履歴
    error_history: Arc<Mutex<Vec<PluginError>>>,
    /// エラーカウンター（プラグインごと）
    error_counters: Arc<RwLock<HashMap<Uuid, ErrorCounter>>>,
    /// 回復戦略
    recovery_strategies: Arc<RwLock<HashMap<ErrorCategory, RecoveryStrategy>>>,
    /// エラーコールバック
    error_callbacks: Arc<Mutex<Vec<ErrorCallback>>>,
    /// 設定
    config: ErrorHandlingConfig,
}

/// プラグインエラー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginError {
    /// エラーID
    pub error_id: Uuid,
    /// プラグインID
    pub plugin_id: Uuid,
    /// エラーカテゴリ
    pub category: ErrorCategory,
    /// エラーコード
    pub error_code: String,
    /// エラーメッセージ
    pub message: String,
    /// スタックトレース
    pub stack_trace: Option<String>,
    /// 発生時刻
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 重大度
    pub severity: ErrorSeverity,
    /// コンテキスト情報
    pub context: HashMap<String, String>,
    /// 回復可能か
    pub recoverable: bool,
}

/// エラーカテゴリ
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// メモリ不足
    OutOfMemory,
    /// CPU制限超過
    CpuLimitExceeded,
    /// ネットワークエラー
    NetworkError,
    /// ファイルシステムエラー
    FileSystemError,
    /// パーミッションエラー
    PermissionDenied,
    /// タイムアウト
    Timeout,
    /// クラッシュ
    Crash,
    /// 初期化失敗
    InitializationFailed,
    /// 実行エラー
    ExecutionError,
    /// 通信エラー
    CommunicationError,
    /// セキュリティ違反
    SecurityViolation,
    /// 不明なエラー
    Unknown,
}

/// エラー重大度
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
    /// 致命的
    Critical,
}

/// エラーカウンター
#[derive(Debug, Clone)]
struct ErrorCounter {
    /// 総エラー数
    total_errors: u64,
    /// カテゴリ別エラー数
    errors_by_category: HashMap<ErrorCategory, u64>,
    /// 最後のエラー時刻
    last_error_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 連続エラー数
    consecutive_errors: u32,
}

impl Default for ErrorCounter {
    fn default() -> Self {
        Self {
            total_errors: 0,
            errors_by_category: HashMap::new(),
            last_error_time: None,
            consecutive_errors: 0,
        }
    }
}

/// 回復戦略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// 再起動
    Restart {
        /// 最大再起動回数
        max_retries: u32,
        /// 再起動間隔（秒）
        backoff_seconds: u64,
    },
    /// 隔離（セーフモード）
    Quarantine,
    /// ロールバック
    Rollback,
    /// リソースリセット
    ResourceReset,
    /// グレースフルシャットダウン
    GracefulShutdown,
    /// 何もしない
    None,
}

/// エラーハンドリング設定
#[derive(Debug, Clone)]
pub struct ErrorHandlingConfig {
    /// 最大エラー履歴サイズ
    pub max_history_size: usize,
    /// エラー履歴保持期間（秒）
    pub history_retention_seconds: u64,
    /// 自動回復を有効化
    pub auto_recovery_enabled: bool,
    /// 連続エラー閾値（これを超えると隔離）
    pub consecutive_error_threshold: u32,
    /// クリティカルエラー閾値（これを超えると停止）
    pub critical_error_threshold: u32,
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            max_history_size: 10000,
            history_retention_seconds: 86400, // 24時間
            auto_recovery_enabled: true,
            consecutive_error_threshold: 5,
            critical_error_threshold: 3,
        }
    }
}

impl PluginErrorHandler {
    /// 新しいエラーハンドラーを作成
    pub async fn new(config: ErrorHandlingConfig) -> Result<Self, McpError> {
        info!("Initializing plugin error handler");

        let mut recovery_strategies = HashMap::new();

        // デフォルト回復戦略を設定
        recovery_strategies.insert(ErrorCategory::OutOfMemory, RecoveryStrategy::ResourceReset);
        recovery_strategies.insert(
            ErrorCategory::CpuLimitExceeded,
            RecoveryStrategy::ResourceReset,
        );
        recovery_strategies.insert(
            ErrorCategory::NetworkError,
            RecoveryStrategy::Restart {
                max_retries: 3,
                backoff_seconds: 5,
            },
        );
        recovery_strategies.insert(
            ErrorCategory::Timeout,
            RecoveryStrategy::Restart {
                max_retries: 2,
                backoff_seconds: 3,
            },
        );
        recovery_strategies.insert(
            ErrorCategory::Crash,
            RecoveryStrategy::Restart {
                max_retries: 3,
                backoff_seconds: 10,
            },
        );
        recovery_strategies.insert(
            ErrorCategory::SecurityViolation,
            RecoveryStrategy::Quarantine,
        );
        recovery_strategies.insert(
            ErrorCategory::InitializationFailed,
            RecoveryStrategy::GracefulShutdown,
        );

        Ok(Self {
            error_history: Arc::new(Mutex::new(Vec::new())),
            error_counters: Arc::new(RwLock::new(HashMap::new())),
            recovery_strategies: Arc::new(RwLock::new(recovery_strategies)),
            error_callbacks: Arc::new(Mutex::new(Vec::new())),
            config,
        })
    }

    /// エラーを処理
    pub async fn handle_error(
        &self,
        plugin_id: Uuid,
        category: ErrorCategory,
        error_code: String,
        message: String,
        stack_trace: Option<String>,
        context: HashMap<String, String>,
    ) -> Result<RecoveryAction, McpError> {
        let severity = self.determine_severity(&category, &context);

        let error = PluginError {
            error_id: Uuid::new_v4(),
            plugin_id,
            category: category.clone(),
            error_code: error_code.clone(),
            message: message.clone(),
            stack_trace,
            timestamp: chrono::Utc::now(),
            severity,
            context: context.clone(),
            recoverable: self.is_recoverable(&category),
        };

        error!(
            "Plugin error detected: plugin_id={:?}, category={:?}, severity={:?}, message={}",
            plugin_id, category, severity, message
        );

        // エラーを履歴に追加
        self.add_to_history(error.clone()).await?;

        // エラーカウンターを更新
        self.update_error_counter(plugin_id, category.clone())
            .await?;

        // エラーコールバックを実行
        self.execute_callbacks(&error).await?;

        // 回復アクションを決定
        let recovery_action = self
            .determine_recovery_action(plugin_id, &category, severity)
            .await?;

        info!(
            "Recovery action determined for plugin {:?}: {:?}",
            plugin_id, recovery_action
        );

        Ok(recovery_action)
    }

    /// エラーを履歴に追加
    async fn add_to_history(&self, error: PluginError) -> Result<(), McpError> {
        let mut history = self.error_history.lock().await;

        history.push(error);

        // サイズ制限チェック
        if history.len() > self.config.max_history_size {
            history.remove(0);
        }

        // 古いエラーを削除
        let cutoff = chrono::Utc::now()
            - chrono::Duration::seconds(self.config.history_retention_seconds as i64);
        history.retain(|e| e.timestamp > cutoff);

        Ok(())
    }

    /// エラーカウンターを更新
    async fn update_error_counter(
        &self,
        plugin_id: Uuid,
        category: ErrorCategory,
    ) -> Result<(), McpError> {
        let mut counters = self.error_counters.write().await;

        let counter = counters.entry(plugin_id).or_default();

        counter.total_errors += 1;
        *counter.errors_by_category.entry(category).or_insert(0) += 1;
        counter.last_error_time = Some(chrono::Utc::now());
        counter.consecutive_errors += 1;

        Ok(())
    }

    /// 重大度を判定
    fn determine_severity(
        &self,
        category: &ErrorCategory,
        context: &HashMap<String, String>,
    ) -> ErrorSeverity {
        match category {
            ErrorCategory::SecurityViolation => ErrorSeverity::Critical,
            ErrorCategory::Crash => ErrorSeverity::Critical,
            ErrorCategory::OutOfMemory => ErrorSeverity::High,
            ErrorCategory::InitializationFailed => ErrorSeverity::High,
            ErrorCategory::CpuLimitExceeded => ErrorSeverity::Medium,
            ErrorCategory::Timeout => ErrorSeverity::Medium,
            ErrorCategory::NetworkError => ErrorSeverity::Low,
            ErrorCategory::FileSystemError => {
                // コンテキストに基づいて判定
                if context.get("critical").map_or(false, |v| v == "true") {
                    ErrorSeverity::High
                } else {
                    ErrorSeverity::Medium
                }
            }
            _ => ErrorSeverity::Low,
        }
    }

    /// 回復可能かどうか判定
    fn is_recoverable(&self, category: &ErrorCategory) -> bool {
        !matches!(
            category,
            ErrorCategory::SecurityViolation | ErrorCategory::InitializationFailed
        )
    }

    /// 回復アクションを決定
    async fn determine_recovery_action(
        &self,
        plugin_id: Uuid,
        category: &ErrorCategory,
        severity: ErrorSeverity,
    ) -> Result<RecoveryAction, McpError> {
        // 自動回復が無効な場合は何もしない
        if !self.config.auto_recovery_enabled {
            return Ok(RecoveryAction::None);
        }

        // エラーカウンターをチェック
        let counters = self.error_counters.read().await;
        if let Some(counter) = counters.get(&plugin_id) {
            // 連続エラー閾値チェック
            if counter.consecutive_errors >= self.config.consecutive_error_threshold {
                warn!(
                    "Consecutive error threshold exceeded for plugin {:?}",
                    plugin_id
                );
                return Ok(RecoveryAction::Quarantine);
            }

            // クリティカルエラー閾値チェック
            let critical_errors = self.count_critical_errors(plugin_id).await?;
            if critical_errors >= self.config.critical_error_threshold {
                error!(
                    "Critical error threshold exceeded for plugin {:?}",
                    plugin_id
                );
                return Ok(RecoveryAction::Shutdown);
            }
        }

        // 重大度に基づいて判定
        if severity == ErrorSeverity::Critical {
            return Ok(RecoveryAction::Quarantine);
        }

        // 回復戦略から決定
        let strategies = self.recovery_strategies.read().await;
        if let Some(strategy) = strategies.get(category) {
            Ok(match strategy {
                RecoveryStrategy::Restart {
                    max_retries,
                    backoff_seconds,
                } => RecoveryAction::Restart {
                    max_retries: *max_retries,
                    backoff_seconds: *backoff_seconds,
                },
                RecoveryStrategy::Quarantine => RecoveryAction::Quarantine,
                RecoveryStrategy::Rollback => RecoveryAction::Rollback,
                RecoveryStrategy::ResourceReset => RecoveryAction::ResourceReset,
                RecoveryStrategy::GracefulShutdown => RecoveryAction::Shutdown,
                RecoveryStrategy::None => RecoveryAction::None,
            })
        } else {
            Ok(RecoveryAction::None)
        }
    }

    /// クリティカルエラー数をカウント
    async fn count_critical_errors(&self, plugin_id: Uuid) -> Result<u32, McpError> {
        let history = self.error_history.lock().await;

        let count = history
            .iter()
            .filter(|e| e.plugin_id == plugin_id && e.severity == ErrorSeverity::Critical)
            .count() as u32;

        Ok(count)
    }

    /// エラーコールバックを実行
    async fn execute_callbacks(&self, error: &PluginError) -> Result<(), McpError> {
        let callbacks = self.error_callbacks.lock().await;

        for callback in callbacks.iter() {
            if let Err(e) = callback(error) {
                warn!("Error callback failed: {}", e);
            }
        }

        Ok(())
    }

    /// エラーコールバックを登録
    pub async fn register_callback(&self, callback: ErrorCallback) -> Result<(), McpError> {
        let mut callbacks = self.error_callbacks.lock().await;
        callbacks.push(callback);
        Ok(())
    }

    /// エラー履歴を取得
    pub async fn get_error_history(
        &self,
        plugin_id: Option<Uuid>,
        category: Option<ErrorCategory>,
        limit: Option<usize>,
    ) -> Result<Vec<PluginError>, McpError> {
        let history = self.error_history.lock().await;

        let mut filtered: Vec<PluginError> = history
            .iter()
            .filter(|e| {
                plugin_id.map_or(true, |id| e.plugin_id == id)
                    && category.as_ref().map_or(true, |cat| &e.category == cat)
            })
            .cloned()
            .collect();

        if let Some(limit) = limit {
            filtered.truncate(limit);
        }

        Ok(filtered)
    }

    /// エラー統計を取得
    pub async fn get_error_stats(&self, plugin_id: Uuid) -> Result<ErrorStats, McpError> {
        let counters = self.error_counters.read().await;

        if let Some(counter) = counters.get(&plugin_id) {
            Ok(ErrorStats {
                total_errors: counter.total_errors,
                errors_by_category: counter.errors_by_category.clone(),
                consecutive_errors: counter.consecutive_errors,
                last_error_time: counter.last_error_time,
            })
        } else {
            Ok(ErrorStats::default())
        }
    }

    /// エラーカウンターをリセット
    pub async fn reset_error_counter(&self, plugin_id: Uuid) -> Result<(), McpError> {
        let mut counters = self.error_counters.write().await;
        counters.remove(&plugin_id);
        Ok(())
    }

    /// 連続エラーカウンターをリセット
    pub async fn reset_consecutive_errors(&self, plugin_id: Uuid) -> Result<(), McpError> {
        let mut counters = self.error_counters.write().await;

        if let Some(counter) = counters.get_mut(&plugin_id) {
            counter.consecutive_errors = 0;
        }

        Ok(())
    }
}

/// 回復アクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    /// 何もしない
    None,
    /// プラグインを再起動
    Restart {
        /// 最大再試行回数
        max_retries: u32,
        /// バックオフ時間（秒）
        backoff_seconds: u64,
    },
    /// プラグインを隔離
    Quarantine,
    /// 前の状態にロールバック
    Rollback,
    /// リソースをリセット
    ResourceReset,
    /// プラグインをシャットダウン
    Shutdown,
}

/// エラー統計
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorStats {
    /// 総エラー数
    pub total_errors: u64,
    /// カテゴリ別エラー数
    pub errors_by_category: HashMap<ErrorCategory, u64>,
    /// 連続エラー数
    pub consecutive_errors: u32,
    /// 最後のエラー時刻
    pub last_error_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_handler_creation() {
        let config = ErrorHandlingConfig::default();
        let handler = PluginErrorHandler::new(config).await;
        assert!(handler.is_ok());
    }

    #[tokio::test]
    async fn test_handle_error() {
        let config = ErrorHandlingConfig::default();
        let handler = PluginErrorHandler::new(config).await.unwrap();

        let plugin_id = Uuid::new_v4();
        let result = handler
            .handle_error(
                plugin_id,
                ErrorCategory::NetworkError,
                "NET_001".to_string(),
                "Connection failed".to_string(),
                None,
                HashMap::new(),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_consecutive_errors_threshold() {
        let mut config = ErrorHandlingConfig::default();
        config.consecutive_error_threshold = 3;

        let handler = PluginErrorHandler::new(config).await.unwrap();
        let plugin_id = Uuid::new_v4();

        // 3回エラーを発生させる
        for _ in 0..3 {
            handler
                .handle_error(
                    plugin_id,
                    ErrorCategory::Timeout,
                    "TIMEOUT".to_string(),
                    "Operation timed out".to_string(),
                    None,
                    HashMap::new(),
                )
                .await
                .unwrap();
        }

        // 次のエラーで隔離アクションになるはず
        let result = handler
            .handle_error(
                plugin_id,
                ErrorCategory::Timeout,
                "TIMEOUT".to_string(),
                "Operation timed out".to_string(),
                None,
                HashMap::new(),
            )
            .await
            .unwrap();

        assert!(matches!(result, RecoveryAction::Quarantine));
    }

    #[tokio::test]
    async fn test_error_stats() {
        let config = ErrorHandlingConfig::default();
        let handler = PluginErrorHandler::new(config).await.unwrap();

        let plugin_id = Uuid::new_v4();

        // いくつかエラーを発生させる
        handler
            .handle_error(
                plugin_id,
                ErrorCategory::NetworkError,
                "NET_001".to_string(),
                "Error 1".to_string(),
                None,
                HashMap::new(),
            )
            .await
            .unwrap();

        handler
            .handle_error(
                plugin_id,
                ErrorCategory::FileSystemError,
                "FS_001".to_string(),
                "Error 2".to_string(),
                None,
                HashMap::new(),
            )
            .await
            .unwrap();

        // 統計を取得
        let stats = handler.get_error_stats(plugin_id).await.unwrap();

        assert_eq!(stats.total_errors, 2);
        assert_eq!(stats.consecutive_errors, 2);
    }
}
