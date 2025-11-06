//! Safety mechanisms for preventing infinite loops and deadlocks
//! 
//! 無限ループとデッドロックを防ぐための安全機構

use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use tracing::{warn, error, info};

/// 操作タイムアウト設定
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// デフォルトタイムアウト（30秒）
    pub default_timeout: Duration,
    /// 接続取得タイムアウト（10秒）
    pub connection_timeout: Duration,
    /// クエリ実行タイムアウト（60秒）
    pub query_timeout: Duration,
    /// プール操作タイムアウト（5秒）
    pub pool_timeout: Duration,
    /// ヘルスチェックタイムアウト（3秒）
    pub health_check_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(10),
            query_timeout: Duration::from_secs(60),
            pool_timeout: Duration::from_secs(5),
            health_check_timeout: Duration::from_secs(3),
        }
    }
}

/// サーキットブレーカーの状態
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,    // 正常状態
    Open,      // 異常状態（リクエスト遮断）
    HalfOpen,  // 復旧試行状態
}

/// サーキットブレーカー
#[derive(Debug)]
pub struct CircuitBreaker {
    state: Arc<tokio::sync::RwLock<CircuitState>>,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_failure_time: Arc<tokio::sync::RwLock<Option<Instant>>>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// 失敗閾値（この回数失敗したらOPEN状態にする）
    pub failure_threshold: u64,
    /// 復旧試行までの待機時間
    pub recovery_timeout: Duration,
    /// 成功閾値（HALF_OPEN状態でこの回数成功したらCLOSED状態にする）
    pub success_threshold: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(60),
            success_threshold: 3,
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(tokio::sync::RwLock::new(CircuitState::Closed)),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_failure_time: Arc::new(tokio::sync::RwLock::new(None)),
            config,
        }
    }

    /// 操作実行前のチェック
    pub async fn can_execute(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // 復旧試行時間が経過したかチェック
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() > self.config.recovery_timeout {
                        drop(state);
                        let mut state_mut = self.state.write().await;
                        *state_mut = CircuitState::HalfOpen;
                        info!("Circuit breaker moved to HALF_OPEN state");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// 操作成功を記録
    pub async fn record_success(&self) {
        let current_count = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
        
        let state = self.state.read().await;
        if *state == CircuitState::HalfOpen && current_count >= self.config.success_threshold {
            drop(state);
            let mut state_mut = self.state.write().await;
            *state_mut = CircuitState::Closed;
            self.failure_count.store(0, Ordering::SeqCst);
            self.success_count.store(0, Ordering::SeqCst);
            info!("Circuit breaker recovered to CLOSED state");
        }
    }

    /// 操作失敗を記録
    pub async fn record_failure(&self) {
        let current_count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure_time.write().await = Some(Instant::now());

        if current_count >= self.config.failure_threshold {
            let mut state = self.state.write().await;
            *state = CircuitState::Open;
            warn!("Circuit breaker opened due to {} failures", current_count);
        }
    }

    /// 現在の状態を取得
    pub async fn get_state(&self) -> CircuitState {
        self.state.read().await.clone()
    }
}

/// ループカウンター（無限ループ検出）
#[derive(Debug)]
pub struct LoopGuard {
    max_iterations: u64,
    current_iterations: AtomicU64,
    start_time: Instant,
    operation_name: String,
}

impl LoopGuard {
    pub fn new(operation_name: &str, max_iterations: u64) -> Self {
        Self {
            max_iterations,
            current_iterations: AtomicU64::new(0),
            start_time: Instant::now(),
            operation_name: operation_name.to_string(),
        }
    }

    /// 反復処理をチェック（trueなら継続、falseなら停止）
    pub fn check_iteration(&self) -> bool {
        let current = self.current_iterations.fetch_add(1, Ordering::SeqCst) + 1;
        
        if current > self.max_iterations {
            error!(
                "Infinite loop detected in '{}': {} iterations exceeded limit of {}",
                self.operation_name, current, self.max_iterations
            );
            return false;
        }

        // 長時間実行の警告
        let elapsed = self.start_time.elapsed();
        if elapsed > Duration::from_secs(10) && current % 100 == 0 {
            warn!(
                "Long-running operation '{}': {} iterations in {:?}",
                self.operation_name, current, elapsed
            );
        }

        true
    }

    /// 現在の反復回数を取得
    pub fn current_iterations(&self) -> u64 {
        self.current_iterations.load(Ordering::SeqCst)
    }
}

/// リソース使用量監視
#[derive(Debug)]
pub struct ResourceMonitor {
    max_memory_mb: u64,
    max_active_connections: u32,
    current_connections: AtomicU64,
    emergency_shutdown: AtomicBool,
}

impl ResourceMonitor {
    pub fn new(max_memory_mb: u64, max_active_connections: u32) -> Self {
        Self {
            max_memory_mb,
            max_active_connections,
            current_connections: AtomicU64::new(0),
            emergency_shutdown: AtomicBool::new(false),
        }
    }

    /// 接続数を増加
    pub fn increment_connections(&self) -> bool {
        let current = self.current_connections.fetch_add(1, Ordering::SeqCst) + 1;
        
        if current > self.max_active_connections as u64 {
            warn!("Connection limit exceeded: {}/{}", current, self.max_active_connections);
            self.current_connections.fetch_sub(1, Ordering::SeqCst);
            return false;
        }
        
        true
    }

    /// 接続数を減少
    pub fn decrement_connections(&self) {
        self.current_connections.fetch_sub(1, Ordering::SeqCst);
    }

    /// 緊急停止フラグをチェック
    pub fn is_emergency_shutdown(&self) -> bool {
        self.emergency_shutdown.load(Ordering::SeqCst)
    }

    /// 緊急停止を発動
    pub fn trigger_emergency_shutdown(&self, reason: &str) {
        error!("Emergency shutdown triggered: {}", reason);
        self.emergency_shutdown.store(true, Ordering::SeqCst);
    }

    /// 現在の接続数を取得
    pub fn current_connections(&self) -> u64 {
        self.current_connections.load(Ordering::SeqCst)
    }
}

/// 包括的安全機構
#[derive(Debug, Clone)]
pub struct SafetyManager {
    pub timeout_config: TimeoutConfig,
    pub circuit_breaker: Arc<CircuitBreaker>,
    pub resource_monitor: Arc<ResourceMonitor>,
}

impl SafetyManager {
    pub fn new() -> Self {
        Self {
            timeout_config: TimeoutConfig::default(),
            circuit_breaker: Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default())),
            resource_monitor: Arc::new(ResourceMonitor::new(512, 100)), // 512MB, 100接続
        }
    }

    /// 安全な操作実行（タイムアウト + サーキットブレーカー）
    pub async fn safe_execute<T, F, Fut>(&self, operation: F, operation_name: &str) -> Result<T, SafetyError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        // 緊急停止チェック
        if self.resource_monitor.is_emergency_shutdown() {
            return Err(SafetyError::EmergencyShutdown);
        }

        // サーキットブレーカーチェック
        if !self.circuit_breaker.can_execute().await {
            return Err(SafetyError::CircuitOpen);
        }

        // リソース制限チェック
        if !self.resource_monitor.increment_connections() {
            return Err(SafetyError::ResourceLimitExceeded);
        }

        let _guard = ConnectionGuard(&self.resource_monitor);

        // タイムアウト付き実行
        let result = timeout(self.timeout_config.default_timeout, operation()).await;

        match result {
            Ok(Ok(value)) => {
                self.circuit_breaker.record_success().await;
                Ok(value)
            }
            Ok(Err(e)) => {
                self.circuit_breaker.record_failure().await;
                Err(SafetyError::OperationFailed(e.to_string()))
            }
            Err(_) => {
                self.circuit_breaker.record_failure().await;
                warn!("Operation '{}' timed out", operation_name);
                Err(SafetyError::Timeout)
            }
        }
    }

    /// プール操作専用の安全実行
    pub async fn safe_pool_operation<T, F, Fut>(&self, operation: F, operation_name: &str) -> Result<T, SafetyError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let result = timeout(self.timeout_config.pool_timeout, operation()).await;
        
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => {
                warn!("Pool operation '{}' failed: {}", operation_name, e);
                Err(SafetyError::OperationFailed(e.to_string()))
            }
            Err(_) => {
                error!("Pool operation '{}' timed out after {:?}", operation_name, self.timeout_config.pool_timeout);
                Err(SafetyError::Timeout)
            }
        }
    }
}

/// 接続数の自動管理
struct ConnectionGuard<'a>(&'a ResourceMonitor);

impl<'a> Drop for ConnectionGuard<'a> {
    fn drop(&mut self) {
        self.0.decrement_connections();
    }
}

/// 安全機構エラー
#[derive(Debug, thiserror::Error)]
pub enum SafetyError {
    #[error("Operation timed out")]
    Timeout,
    #[error("Circuit breaker is open")]
    CircuitOpen,
    #[error("Resource limit exceeded")]
    ResourceLimitExceeded,
    #[error("Emergency shutdown activated")]
    EmergencyShutdown,
    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// マクロ：安全なループ実行
#[macro_export]
macro_rules! safe_loop {
    ($operation_name:expr, $max_iterations:expr, $body:block) => {
        {
            let loop_guard = $crate::safety::LoopGuard::new($operation_name, $max_iterations);
            loop {
                if !loop_guard.check_iteration() {
                    break Err($crate::safety::SafetyError::OperationFailed(
                        format!("Loop limit exceeded in {}", $operation_name)
                    ));
                }
                
                match { $body } {
                    Ok(result) => break Ok(result),
                    Err(e) if loop_guard.current_iterations() > $max_iterations / 2 => {
                        // 半分以上試行したら諦める
                        break Err(e);
                    }
                    Err(_) => continue, // リトライ継続
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker() {
        let circuit = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_millis(100),
            success_threshold: 2,
        });

        // 初期状態はClosed
        assert_eq!(circuit.get_state().await, CircuitState::Closed);
        assert!(circuit.can_execute().await);

        // 失敗を記録してOpen状態にする
        for _ in 0..3 {
            circuit.record_failure().await;
        }
        assert_eq!(circuit.get_state().await, CircuitState::Open);
        assert!(!circuit.can_execute().await);

        // 復旧待機時間後にHalfOpen状態になる
        sleep(Duration::from_millis(150)).await;
        assert!(circuit.can_execute().await);
        assert_eq!(circuit.get_state().await, CircuitState::HalfOpen);

        // 成功を記録してClosed状態に戻る
        for _ in 0..2 {
            circuit.record_success().await;
        }
        assert_eq!(circuit.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_loop_guard() {
        let guard = LoopGuard::new("test_operation", 5);
        
        // 制限内は継続
        for _ in 0..5 {
            assert!(guard.check_iteration());
        }
        
        // 制限を超えると停止
        assert!(!guard.check_iteration());
    }

    #[tokio::test]
    async fn test_safety_manager() {
        let safety = SafetyManager::new();
        
        // 成功ケース
        let result = safety.safe_execute(
            || async { Ok::<i32, Box<dyn std::error::Error + Send + Sync>>(42) },
            "test_operation"
        ).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        // タイムアウトケース
        let result = safety.safe_execute(
            || async {
                sleep(Duration::from_secs(60)).await;
                Ok::<i32, Box<dyn std::error::Error + Send + Sync>>(42)
            },
            "timeout_operation"
        ).await;
        assert!(matches!(result, Err(SafetyError::Timeout)));
    }
}