//! Database Retry and Timeout Strategies
//!
//! データベース操作のリトライとタイムアウト戦略

use super::types::{DatabaseError, ExecuteResult, QueryResult};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{error, info, warn};

/// リトライ戦略
#[derive(Debug, Clone)]
pub enum RetryStrategy {
    /// 固定間隔でリトライ
    FixedInterval {
        interval: Duration,
        max_attempts: u32,
    },
    /// 指数バックオフ
    ExponentialBackoff {
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
        max_attempts: u32,
    },
    /// リニアバックオフ
    LinearBackoff {
        initial_delay: Duration,
        increment: Duration,
        max_attempts: u32,
    },
    /// カスタム間隔
    Custom { delays: Vec<Duration> },
}

impl RetryStrategy {
    /// デフォルトの指数バックオフ戦略
    pub fn default_exponential() -> Self {
        Self::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            max_attempts: 3,
        }
    }

    /// デフォルトの固定間隔戦略
    pub fn default_fixed() -> Self {
        Self::FixedInterval {
            interval: Duration::from_secs(1),
            max_attempts: 3,
        }
    }

    /// リトライ可能な操作を実行
    pub async fn execute<F, T, Fut>(&self, operation: F) -> Result<T, DatabaseError>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T, DatabaseError>> + Send,
        T: Send,
    {
        let mut last_error = None;

        match self {
            Self::FixedInterval {
                interval,
                max_attempts,
            } => {
                for attempt in 1..=*max_attempts {
                    match operation().await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            last_error = Some(e.clone());
                            if attempt < *max_attempts {
                                if self.should_retry(&e) {
                                    info!(
                                        "Retrying operation (attempt {}/{})",
                                        attempt, max_attempts
                                    );
                                    sleep(*interval).await;
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
            }
            Self::ExponentialBackoff {
                initial_delay,
                max_delay,
                multiplier,
                max_attempts,
            } => {
                let mut delay = *initial_delay;
                for attempt in 1..=*max_attempts {
                    match operation().await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            last_error = Some(e.clone());
                            if attempt < *max_attempts {
                                if self.should_retry(&e) {
                                    info!("Retrying operation with exponential backoff (attempt {}/{}), delay: {:?}", 
                                          attempt, max_attempts, delay);
                                    sleep(delay).await;
                                    delay = Duration::from_millis(
                                        ((delay.as_millis() as f64) * multiplier) as u64,
                                    )
                                    .min(*max_delay);
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
            }
            Self::LinearBackoff {
                initial_delay,
                increment,
                max_attempts,
            } => {
                let mut delay = *initial_delay;
                for attempt in 1..=*max_attempts {
                    match operation().await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            last_error = Some(e.clone());
                            if attempt < *max_attempts {
                                if self.should_retry(&e) {
                                    info!("Retrying operation with linear backoff (attempt {}/{}), delay: {:?}", 
                                          attempt, max_attempts, delay);
                                    sleep(delay).await;
                                    delay += *increment;
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
            }
            Self::Custom { delays } => {
                for (attempt, delay) in delays.iter().enumerate() {
                    match operation().await {
                        Ok(result) => return Ok(result),
                        Err(e) => {
                            last_error = Some(e.clone());
                            if attempt < delays.len() - 1 {
                                if self.should_retry(&e) {
                                    info!("Retrying operation with custom delay (attempt {}/{}), delay: {:?}", 
                                          attempt + 1, delays.len(), delay);
                                    sleep(*delay).await;
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            DatabaseError::OperationFailed("All retry attempts failed".to_string())
        }))
    }

    /// エラーがリトライ可能かどうかを判定
    fn should_retry(&self, error: &DatabaseError) -> bool {
        match error {
            // ネットワーク関連のエラーはリトライ可能
            DatabaseError::ConnectionFailed(_) => true,
            DatabaseError::TimeoutError(_) => true,
            DatabaseError::NetworkError(_) => true,

            // データベースサーバーの一時的な問題
            DatabaseError::ServerUnavailable(_) => true,
            DatabaseError::DeadlockDetected(_) => true,

            // 設定や認証の問題はリトライしない
            DatabaseError::AuthenticationError(_) => false,
            DatabaseError::ConfigurationError(_) => false,
            DatabaseError::ValidationError(_) => false,

            // SQL構文エラーはリトライしない
            DatabaseError::SqlSyntaxError(_) => false,

            // その他は保守的にリトライしない
            _ => false,
        }
    }
}

/// タイムアウト戦略
#[derive(Debug, Clone)]
pub struct TimeoutStrategy {
    /// 接続タイムアウト
    pub connection_timeout: Duration,
    /// クエリタイムアウト
    pub query_timeout: Duration,
    /// トランザクションタイムアウト
    pub transaction_timeout: Duration,
    /// バルク操作タイムアウト
    pub bulk_operation_timeout: Duration,
}

impl Default for TimeoutStrategy {
    fn default() -> Self {
        Self {
            connection_timeout: Duration::from_secs(10),
            query_timeout: Duration::from_secs(30),
            transaction_timeout: Duration::from_secs(60),
            bulk_operation_timeout: Duration::from_secs(300),
        }
    }
}

impl TimeoutStrategy {
    /// 短いタイムアウト設定（レスポンス重視）
    pub fn fast() -> Self {
        Self {
            connection_timeout: Duration::from_secs(3),
            query_timeout: Duration::from_secs(10),
            transaction_timeout: Duration::from_secs(30),
            bulk_operation_timeout: Duration::from_secs(60),
        }
    }

    /// 長いタイムアウト設定（安定性重視）
    pub fn robust() -> Self {
        Self {
            connection_timeout: Duration::from_secs(30),
            query_timeout: Duration::from_secs(120),
            transaction_timeout: Duration::from_secs(300),
            bulk_operation_timeout: Duration::from_secs(1800),
        }
    }

    /// タイムアウト付きでクエリを実行
    pub async fn execute_query<F, Fut>(&self, operation: &F) -> Result<QueryResult, DatabaseError>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<QueryResult, DatabaseError>> + Send,
    {
        match timeout(self.query_timeout, operation()).await {
            Ok(result) => result,
            Err(_) => Err(DatabaseError::TimeoutError(format!(
                "Query timed out after {:?}",
                self.query_timeout
            ))),
        }
    }

    /// タイムアウト付きで実行操作を実行
    pub async fn execute_command<F, Fut>(
        &self,
        operation: &F,
    ) -> Result<ExecuteResult, DatabaseError>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<ExecuteResult, DatabaseError>> + Send,
    {
        match timeout(self.query_timeout, operation()).await {
            Ok(result) => result,
            Err(_) => Err(DatabaseError::TimeoutError(format!(
                "Command execution timed out after {:?}",
                self.query_timeout
            ))),
        }
    }

    /// タイムアウト付きで接続を実行
    pub async fn execute_connection<F, Fut, T>(&self, operation: F) -> Result<T, DatabaseError>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, DatabaseError>> + Send,
        T: Send,
    {
        match timeout(self.connection_timeout, operation()).await {
            Ok(result) => result,
            Err(_) => Err(DatabaseError::TimeoutError(format!(
                "Connection timed out after {:?}",
                self.connection_timeout
            ))),
        }
    }
}

/// 包括的な実行戦略（リトライ + タイムアウト）
#[derive(Clone)]
pub struct ExecutionStrategy {
    pub retry: RetryStrategy,
    pub timeout: TimeoutStrategy,
}

impl Default for ExecutionStrategy {
    fn default() -> Self {
        Self {
            retry: RetryStrategy::default_exponential(),
            timeout: TimeoutStrategy::default(),
        }
    }
}

impl ExecutionStrategy {
    /// 高速実行戦略（短いタイムアウト、少ないリトライ）
    pub fn fast() -> Self {
        Self {
            retry: RetryStrategy::FixedInterval {
                interval: Duration::from_millis(500),
                max_attempts: 2,
            },
            timeout: TimeoutStrategy::fast(),
        }
    }

    /// 堅牢実行戦略（長いタイムアウト、多いリトライ）
    pub fn robust() -> Self {
        Self {
            retry: RetryStrategy::ExponentialBackoff {
                initial_delay: Duration::from_millis(500),
                max_delay: Duration::from_secs(60),
                multiplier: 2.0,
                max_attempts: 5,
            },
            timeout: TimeoutStrategy::robust(),
        }
    }

    /// タイムアウトとリトライ付きでクエリを実行
    pub async fn execute_query<F, Fut>(&self, operation: F) -> Result<QueryResult, DatabaseError>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<QueryResult, DatabaseError>> + Send,
    {
        self.retry
            .execute(|| async { self.timeout.execute_query(&operation).await })
            .await
    }

    /// タイムアウトとリトライ付きで実行操作を実行
    pub async fn execute_command<F, Fut>(
        &self,
        operation: F,
    ) -> Result<ExecuteResult, DatabaseError>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<ExecuteResult, DatabaseError>> + Send,
    {
        self.retry
            .execute(|| async { self.timeout.execute_command(&operation).await })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_fixed_interval_retry() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        let strategy = RetryStrategy::FixedInterval {
            interval: Duration::from_millis(10),
            max_attempts: 3,
        };

        let result = strategy
            .execute(|| {
                let counter = Arc::clone(&counter_clone);
                async move {
                    let attempt = counter.fetch_add(1, Ordering::SeqCst);
                    if attempt < 2 {
                        Err(DatabaseError::ConnectionFailed("Test error".to_string()))
                    } else {
                        Ok("Success")
                    }
                }
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let strategy = RetryStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(100),
            multiplier: 2.0,
            max_attempts: 3,
        };

        let start = std::time::Instant::now();
        let result = strategy
            .execute(|| async {
                Err::<(), _>(DatabaseError::ConnectionFailed("Always fail".to_string()))
            })
            .await;

        let elapsed = start.elapsed();
        assert!(result.is_err());
        // 1 + 2 + 4 = 7ms の遅延があるはず（実際にはそれ以上）
        assert!(elapsed >= Duration::from_millis(6));
    }

    #[tokio::test]
    async fn test_timeout_strategy() {
        let strategy = TimeoutStrategy {
            connection_timeout: Duration::from_millis(50),
            query_timeout: Duration::from_millis(50),
            transaction_timeout: Duration::from_millis(50),
            bulk_operation_timeout: Duration::from_millis(50),
        };

        let operation = || async {
            sleep(Duration::from_millis(100)).await;
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                total_rows: Some(0),
                execution_time_ms: 100,
            })
        };

        let result = strategy.execute_query(&operation).await;

        assert!(result.is_err());
        if let Err(DatabaseError::TimeoutError(_)) = result {
            // タイムアウトエラーが正しく発生
        } else {
            panic!("Expected timeout error");
        }
    }

    #[tokio::test]
    async fn test_execution_strategy() {
        let strategy = ExecutionStrategy::fast();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        let operation = || {
            let counter = Arc::clone(&counter_clone);
            async move {
                let attempt = counter.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    Err(DatabaseError::ConnectionFailed(
                        "First attempt fails".to_string(),
                    ))
                } else {
                    Ok(QueryResult {
                        columns: vec![],
                        rows: vec![],
                        total_rows: Some(1),
                        execution_time_ms: 10,
                    })
                }
            }
        };

        let result = strategy.execute_query(&operation).await;

        assert!(result.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
