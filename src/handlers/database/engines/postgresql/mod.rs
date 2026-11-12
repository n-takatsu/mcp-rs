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
    types::{
        DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType,
        ExecuteResult, HealthStatus, QueryResult, TableInfo, Value,
    },
};
use async_trait::async_trait;
use std::sync::Arc;

/// `DatabaseConfig::connection.options`のキー: `query()`で受け取る行数の
/// 上限（#453）
pub const OPTION_FETCH_LIMIT: &str = "fetch_limit";

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
        let fetch_limit = config
            .connection
            .options
            .get(OPTION_FETCH_LIMIT)
            .and_then(|v| v.parse::<usize>().ok());
        let conn = PostgreSqlConnection::with_pool(
            pool,
            config.connection.database.clone(),
            config.connection.username.clone(),
        )
        .with_statement_timeout(std::time::Duration::from_secs(u64::from(
            config.features.query_timeout,
        )))
        .with_fetch_limit(fetch_limit);
        Ok(Box::new(conn))
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        use crate::handlers::database::types::HealthStatusType;
        use chrono::Utc;

        Ok(HealthStatus {
            status: HealthStatusType::Healthy,
            last_check: Utc::now(),
            response_time_ms: 0,
            error_message: None,
            connection_count: 1,
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

    async fn get_version(&self) -> Result<String, DatabaseError> {
        Ok("PostgreSQL 15 (Mock)".to_string())
    }
}
