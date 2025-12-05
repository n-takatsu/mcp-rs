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

/// PostgreSQL Database Engine
///
/// Main entry point for PostgreSQL database operations
#[derive(Clone)]
pub struct PostgreSqlEngine {
    config: DatabaseConfig,
}

impl PostgreSqlEngine {
    /// Create a new PostgreSQL engine
    pub async fn new(
        config: crate::handlers::database::types::DatabaseConfig,
    ) -> Result<Self, crate::handlers::database::types::DatabaseError> {
        Ok(Self { config })
    }

    /// Create a new PostgreSQL engine without security checks (for testing)
    pub async fn new_without_security(
        config: DatabaseConfig,
    ) -> Result<Self, DatabaseError> {
        Ok(Self { config })
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
        // Build connection string from config
        let connection_string = format!(
            "postgresql://{}:{}@{}:{}/{}",
            config.connection.username,
            config.connection.password,
            config.connection.host,
            config.connection.port,
            config.connection.database
        );

        let conn = PostgreSqlConnection::new(&connection_string).await?;
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
