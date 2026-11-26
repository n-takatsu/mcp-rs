//! SQLite Database Engine Implementation
//!
//! Provides SQLite-specific database engine with file-based storage support

use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    types::{
        DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseType, HealthStatus,
        HealthStatusType,
    },
};
use async_trait::async_trait;
use chrono::Utc;

use super::connection::{open_pools, pools_key, SqlitePools};
use super::SqliteConnection;
use std::sync::Arc;

/// SQLite Database Engine
///
/// Main entry point for SQLite database operations。プール（読み取りと書き込み）
/// は、最初の接続で開き、後の接続で共有する（#528。MySQLと同じ）
#[derive(Clone)]
pub struct SqliteEngine {
    config: DatabaseConfig,
    pools: Arc<tokio::sync::OnceCell<Arc<SqlitePools>>>,
}

impl SqliteEngine {
    /// Create a new SQLite engine
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        Ok(Self {
            config,
            pools: Arc::new(tokio::sync::OnceCell::new()),
        })
    }

    /// Get database config
    pub fn get_config(&self) -> &DatabaseConfig {
        &self.config
    }
}

#[async_trait]
impl DatabaseEngine for SqliteEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let pools = self
            .pools
            .get_or_try_init(|| open_pools(config))
            .await?
            .clone();
        // 別のファイル・書き込みの許し方の設定なら、共有のプールを使わず、
        // その接続だけのプールを開く（PR #798のレビュー）
        if pools.key != pools_key(config) {
            return Ok(Box::new(SqliteConnection::new(config.clone()).await?));
        }
        Ok(Box::new(SqliteConnection::with_pools(
            pools,
            config.clone(),
        )))
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        let start = std::time::Instant::now();

        match self.connect(&self.config).await {
            Ok(conn) => {
                conn.ping().await?;
                let elapsed = start.elapsed();

                Ok(HealthStatus {
                    status: HealthStatusType::Healthy,
                    last_check: Utc::now(),
                    response_time_ms: elapsed.as_millis() as u64,
                    error_message: None,
                    connection_count: 1,
                    active_transactions: 0,
                })
            }
            Err(e) => Ok(HealthStatus {
                status: HealthStatusType::Critical,
                last_check: Utc::now(),
                response_time_ms: start.elapsed().as_millis() as u64,
                error_message: Some(e.to_string()),
                connection_count: 0,
                active_transactions: 0,
            }),
        }
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::Transactions,
            DatabaseFeature::PreparedStatements,
        ]
    }

    fn validate_config(&self, config: &DatabaseConfig) -> Result<(), DatabaseError> {
        if config.connection.host.is_empty() {
            return Err(DatabaseError::ConnectionFailed(
                "Database path is required".to_string(),
            ));
        }
        Ok(())
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        let conn = self.connect(&self.config).await?;
        let result = conn.query("SELECT sqlite_version()", &[]).await?;

        if let Some(row) = result.rows.first() {
            if let Some(crate::handlers::database::types::Value::String(s)) = row.first() {
                return Ok(s.clone());
            }
        }

        Ok("unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::{
        ConnectionConfig, FeatureConfig, PoolConfig, SecurityConfig,
    };
    use std::collections::HashMap;

    fn create_test_config() -> DatabaseConfig {
        DatabaseConfig {
            database_type: DatabaseType::SQLite,
            connection: ConnectionConfig {
                host: ":memory:".to_string(),
                port: 0,
                database: "test".to_string(),
                username: String::new(),
                password: String::new(),
                ssl_mode: None,
                timeout_seconds: 30,
                retry_attempts: 3,
                options: HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await;
        assert!(engine.is_ok());
    }

    #[tokio::test]
    async fn test_engine_type() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await.unwrap();
        assert_eq!(engine.engine_type(), DatabaseType::SQLite);
    }

    #[tokio::test]
    async fn test_supported_features() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await.unwrap();
        let features = engine.supported_features();

        assert!(features.contains(&DatabaseFeature::Transactions));
        assert!(features.contains(&DatabaseFeature::PreparedStatements));
    }

    #[tokio::test]
    async fn test_validate_config() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        assert!(engine.validate_config(&config).is_ok());
    }

    #[tokio::test]
    async fn test_validate_config_empty_path() {
        let mut config = create_test_config();
        config.connection.host = String::new();

        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        assert!(engine.validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn test_connect() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await;
        assert!(conn.is_ok());
    }

    #[tokio::test]
    async fn test_health_check() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await.unwrap();
        let health = engine.health_check().await;
        assert!(health.is_ok());
    }

    #[tokio::test]
    async fn test_get_version() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await.unwrap();
        let version = engine.get_version().await;
        assert!(version.is_ok());
        assert!(!version.unwrap().is_empty());
    }
}
