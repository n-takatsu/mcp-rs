//! PostgreSQL Database Engine
//!
//! Provides PostgreSQL-specific implementations for database operations
//! including prepared statements, transactions, and JSON support.

pub mod connection;
pub mod json_support;
pub mod pg_connection;
pub mod prepared;
pub mod transaction;

pub use connection::{PostgreSqlConfig, PostgreSqlPool};
pub use json_support::PostgreSqlJsonSupport;
pub use pg_connection::PostgreSqlConnection;
pub use prepared::PostgreSqlPreparedStatement;
pub use transaction::PostgreSqlTransaction;

use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    types::{DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseType, HealthStatus},
};
use async_trait::async_trait;
use std::sync::Arc;

// 接続の設定のキー。MySQL（#527）と共有するので、`engines`に置いた
pub use super::{OPTION_BYTE_LIMIT, OPTION_FETCH_LIMIT, OPTION_SSL_ROOT_CERT};

/// PostgreSQL Database Engine
///
/// Main entry point for PostgreSQL database operations
#[derive(Clone)]
pub struct PostgreSqlEngine {
    config: DatabaseConfig,
    /// 接続のプール。最初の`connect`で作り、以後の接続で共有する（#453）。
    /// 以前は`connect`のたびに新しいプールを作っていた
    pool: Arc<tokio::sync::OnceCell<sqlx::PgPool>>,
}

impl PostgreSqlEngine {
    /// Create a new PostgreSQL engine
    pub async fn new(
        config: crate::handlers::database::types::DatabaseConfig,
    ) -> Result<Self, crate::handlers::database::types::DatabaseError> {
        Ok(Self {
            config,
            pool: Arc::new(tokio::sync::OnceCell::new()),
        })
    }

    /// Get database config
    pub fn get_config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// 共有のプール。まだ無ければ、エンジンの設定で作る（つなぐ）
    async fn shared_pool(&self) -> Result<sqlx::PgPool, DatabaseError> {
        Ok(self
            .pool
            .get_or_try_init(|| pg_connection::connect_pool(&self.config))
            .await?
            .clone())
    }

    /// `health_check`・`get_version`の待ち時間の上限（`query_timeout`秒。
    /// 0なら1秒）
    fn check_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(u64::from(self.config.features.query_timeout).max(1))
    }

    /// プールを用意し（まだ無ければつなぎ）、`run`を、期限の中で実行する
    async fn with_pool<T, F, Fut>(&self, run: F) -> Result<T, DatabaseError>
    where
        F: FnOnce(sqlx::PgPool) -> Fut,
        Fut: std::future::Future<Output = Result<T, DatabaseError>>,
    {
        let timeout = self.check_timeout();
        tokio::time::timeout(timeout, async { run(self.shared_pool().await?).await })
            .await
            .map_err(|_| {
                DatabaseError::TimeoutError(format!(
                    "PostgreSQL did not respond within {} seconds",
                    timeout.as_secs()
                ))
            })?
    }
}

#[async_trait]
impl DatabaseEngine for PostgreSqlEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let pool = self
            .pool
            .get_or_try_init(|| pg_connection::connect_pool(config))
            .await?
            .clone();
        let limit = |key: &str| {
            config
                .connection
                .options
                .get(key)
                .and_then(|v| v.parse::<usize>().ok())
        };
        let conn = PostgreSqlConnection::with_pool(
            pool,
            config.connection.database.clone(),
            config.connection.username.clone(),
        )
        .with_statement_timeout(std::time::Duration::from_secs(u64::from(
            config.features.query_timeout,
        )))
        .with_fetch_limit(limit(OPTION_FETCH_LIMIT))
        .with_byte_limit(limit(OPTION_BYTE_LIMIT));
        Ok(Box::new(conn))
    }

    /// 接続先に`SELECT 1`を送り、答えれば`Healthy`（#758）。つなげない・
    /// 答えない・期限（`query_timeout`）を過ぎたときは`Critical`と、その
    /// 理由。まだつないでいなければ、エンジンの設定でつなぐ。以前は
    /// データベースに触れず、常に`Healthy`を返した
    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        use crate::handlers::database::types::HealthStatusType;
        use chrono::Utc;

        let started = std::time::Instant::now();
        let result = self
            .with_pool(|pool| async move {
                sqlx::query("SELECT 1")
                    .execute(&pool)
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
                Ok(pool)
            })
            .await;
        let response_time_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let connection_count = self.pool.get().map_or(0, |pool| pool.size());
        let (status, error_message) = match result {
            Ok(_) => (HealthStatusType::Healthy, None),
            Err(e) => (HealthStatusType::Critical, Some(e.to_string())),
        };
        Ok(HealthStatus {
            status,
            last_check: Utc::now(),
            response_time_ms,
            error_message,
            connection_count,
            active_transactions: 0,
        })
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::Transactions,
            DatabaseFeature::PreparedStatements,
            DatabaseFeature::JsonSupport,
            DatabaseFeature::Acid,
        ]
    }

    fn validate_config(&self, _config: &DatabaseConfig) -> Result<(), DatabaseError> {
        Ok(())
    }

    /// 接続先の版（`SHOW server_version`）を`PostgreSQL 16.4`の形で返す
    /// （#758）。つなげない・読めないときはエラー。以前は接続先によらず
    /// `PostgreSQL 15 (Mock)`を返した
    async fn get_version(&self) -> Result<String, DatabaseError> {
        let version: String = self
            .with_pool(|pool| async move {
                sqlx::query_scalar("SHOW server_version")
                    .fetch_one(&pool)
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
            })
            .await?;
        Ok(format!("PostgreSQL {}", version.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::HealthStatusType;

    /// 誰も待ち受けていない（つなげない）接続先の設定
    fn unreachable_config() -> DatabaseConfig {
        let mut config = DatabaseConfig {
            database_type: DatabaseType::PostgreSQL,
            ..Default::default()
        };
        config.connection.host = "127.0.0.1".to_string();
        // 予約されたポート（待ち受けているものは無い）
        config.connection.port = 1;
        config.connection.timeout_seconds = 1;
        config.features.query_timeout = 2;
        config
    }

    /// つなげないときは`Healthy`でなく`Critical`と理由を返す（#758）
    #[tokio::test]
    async fn test_health_check_reports_an_unreachable_server() {
        let engine = PostgreSqlEngine::new(unreachable_config()).await.unwrap();
        let health = engine.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatusType::Critical);
        assert!(health.error_message.is_some(), "{health:?}");
        assert_eq!(health.connection_count, 0);
    }

    /// つなげないときは、決まった版でなくエラーを返す（#758）
    #[tokio::test]
    async fn test_get_version_fails_without_a_server() {
        let engine = PostgreSqlEngine::new(unreachable_config()).await.unwrap();
        let result = engine.get_version().await;
        assert!(result.is_err(), "{result:?}");
    }
}
