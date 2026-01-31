//! PostgreSQL Database Engine Implementation
//!
//! This module provides optimized PostgreSQL support with advanced features:
//! - Optimized connection pooling with configurable sizing
//! - JSONB native operations
//! - LISTEN/NOTIFY for real-time notifications
//! - CTEs and Window Functions support
//! - Full-text search capabilities
//! - Query performance analysis

pub mod config;
pub mod connection;
pub mod error;
pub mod pool;
pub mod transaction;

pub use config::{PostgresConfig, PostgresConfigBuilder};
pub use connection::{PostgresConnection, PostgresPreparedStatement};
pub use error::{PostgresError, Result};
pub use pool::{create_optimized_pool, OptimizedPoolConfig, PoolMetrics};
pub use transaction::PostgresTransaction;

use crate::handlers::database::{DatabaseEngine, DatabaseConnection};
use crate::handlers::database::types::{DatabaseConfig, DatabaseFeature, DatabaseType, DatabaseError, ExecuteResult, QueryResult, Value, HealthStatus, HealthStatusType};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tokio::sync::RwLock;

/// PostgreSQL database engine with optimized connection pooling and advanced features
#[derive(Clone)]
pub struct PostgresEngine {
    /// Optimized connection pool
    pool: PgPool,
    /// Configuration
    config: PostgresConfig,
    /// Pool metrics for monitoring
    metrics: Arc<RwLock<PoolMetrics>>,
}

impl PostgresEngine {
    /// Create a new PostgreSQL engine with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - PostgreSQL configuration
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mcp_rs::handlers::database::engines::postgres::{PostgresEngine, PostgresConfig};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = PostgresConfig::builder()
    ///     .host("localhost")
    ///     .port(5432)
    ///     .database("mydb")
    ///     .username("postgres")
    ///     .password("password")
    ///     .build()?;
    ///
    /// let engine = PostgresEngine::new(config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(config: PostgresConfig) -> Result<Self> {
        let pool = create_optimized_pool(&config).await?;
        let metrics = Arc::new(RwLock::new(PoolMetrics::default()));

        Ok(Self {
            pool,
            config,
            metrics,
        })
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get current pool metrics
    pub async fn metrics(&self) -> PoolMetrics {
        self.metrics.read().await.clone()
    }

    /// Update pool metrics
    async fn update_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.update_from_pool(&self.pool);
    }

    /// Get the database configuration
    pub fn config(&self) -> &PostgresConfig {
        &self.config
    }

    /// Begin a new transaction
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mcp_rs::handlers::database::engines::postgres::PostgresEngine;
    /// # async fn example(engine: &PostgresEngine) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut tx = engine.begin_transaction().await?;
    /// // ... perform operations
    /// tx.commit().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn begin_transaction(&self) -> std::result::Result<PostgresTransaction, DatabaseError> {
        PostgresTransaction::begin(&self.pool).await
    }

    /// Begin a transaction with specific isolation level
    pub async fn begin_transaction_with_isolation(
        &self,
        level: crate::handlers::database::engine::IsolationLevel,
    ) -> std::result::Result<PostgresTransaction, DatabaseError> {
        PostgresTransaction::begin_with_isolation(&self.pool, level).await
    }
}

#[async_trait]
impl DatabaseEngine for PostgresEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn connect(
        &self,
        _config: &DatabaseConfig,
    ) -> std::result::Result<Box<dyn DatabaseConnection>, DatabaseError> {
        // Return a connection using the existing pool
        let connection = PostgresConnection::new(self.pool.clone());
        Ok(Box::new(connection))
    }

    async fn health_check(&self) -> std::result::Result<HealthStatus, DatabaseError> {
        let start = std::time::Instant::now();
        
        match sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
        {
            Ok(_) => {
                let response_time = start.elapsed().as_millis() as u64;
                Ok(HealthStatus {
                    status: HealthStatusType::Healthy,
                    last_check: Utc::now(),
                    response_time_ms: response_time,
                    error_message: None,
                    connection_count: self.pool.size(),
                    active_transactions: 0,
                })
            }
            Err(e) => {
                let response_time = start.elapsed().as_millis() as u64;
                tracing::warn!("PostgreSQL health check failed: {}", e);
                Ok(HealthStatus {
                    status: HealthStatusType::Critical,
                    last_check: Utc::now(),
                    response_time_ms: response_time,
                    error_message: Some(format!("Health check failed: {}", e)),
                    connection_count: self.pool.size(),
                    active_transactions: 0,
                })
            }
        }
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::Transactions,
            DatabaseFeature::PreparedStatements,
            DatabaseFeature::JSONB,
            DatabaseFeature::FullTextSearch,
            DatabaseFeature::CTEs,
            DatabaseFeature::WindowFunctions,
            DatabaseFeature::Notifications,
        ]
    }

    fn validate_config(&self, config: &DatabaseConfig) -> std::result::Result<(), DatabaseError> {
        if config.connection.host.is_empty() {
            return Err(DatabaseError::ConfigurationError("Host cannot be empty".to_string()));
        }

        if config.connection.database.is_empty() {
            return Err(DatabaseError::ConfigurationError("Database name cannot be empty".to_string()));
        }

        Ok(())
    }

    async fn get_version(&self) -> std::result::Result<String, DatabaseError> {
        let row = sqlx::query("SELECT version()")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let version: String = row.try_get(0).map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(version)
    }
}

/// Helper function to bind a JSON value to a query
fn bind_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &'q JsonValue,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>> {
    match value {
        JsonValue::Null => Ok(query.bind(None::<String>)),
        JsonValue::Bool(b) => Ok(query.bind(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(query.bind(i))
            } else if let Some(f) = n.as_f64() {
                Ok(query.bind(f))
            } else {
                Err(PostgresError::InvalidParameter(format!(
                    "Unsupported number type: {}",
                    n
                )))
            }
        }
        JsonValue::String(s) => Ok(query.bind(s.as_str())),
        JsonValue::Array(_) | JsonValue::Object(_) => {
            // For arrays and objects, bind as JSONB
            Ok(query.bind(sqlx::types::Json(value)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_engine_creation() {
        // Test will be implemented with actual PostgreSQL connection
    }
}

