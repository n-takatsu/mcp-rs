//! MariaDB Engine Implementation
//!
//! MariaDB is a fork of MySQL and maintains wire protocol compatibility.
//! This implementation uses the same mysql_async driver as MySQL.

use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    engines::mysql::MySqlEngine,
    security::DatabaseSecurity,
    types::{
        DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType, HealthStatus,
        QueryResult, TableInfo, Value,
    },
};
use async_trait::async_trait;
use std::sync::Arc;

/// MariaDB Database Engine
///
/// MariaDB maintains MySQL compatibility, so we use MySqlEngine internally
/// with MariaDB-specific feature flags and optimizations.
#[derive(Clone)]
pub struct MariaDbEngine {
    mysql_engine: MySqlEngine,
    config: DatabaseConfig,
}

impl MariaDbEngine {
    /// Create new MariaDB engine with security integration
    pub async fn new(
        config: DatabaseConfig,
        security: Arc<DatabaseSecurity>,
    ) -> Result<Self, DatabaseError> {
        // Create underlying MySQL engine
        let mysql_engine = MySqlEngine::new(config.clone(), security).await?;

        Ok(Self {
            mysql_engine,
            config,
        })
    }

    /// Create MariaDB engine without security (for testing)
    pub async fn new_without_security(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        let mysql_engine = MySqlEngine::new_without_security(config.clone()).await?;

        Ok(Self {
            mysql_engine,
            config,
        })
    }

    /// Get database config
    pub fn get_config(&self) -> &DatabaseConfig {
        &self.config
    }
}

#[async_trait]
impl DatabaseEngine for MariaDbEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::MariaDB
    }

    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        // Use MySQL connection (MariaDB is wire-protocol compatible)
        self.mysql_engine.connect(config).await
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        self.mysql_engine.health_check().await
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        let mut features = self.mysql_engine.supported_features();

        // MariaDB-specific features
        // MariaDB has some additional features compared to MySQL
        features.extend(vec![
            // MariaDB has better JSON support in newer versions
            DatabaseFeature::JsonSupport,
        ]);

        features.dedup();
        features
    }

    fn validate_config(&self, config: &DatabaseConfig) -> Result<(), DatabaseError> {
        self.mysql_engine.validate_config(config)
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        // Get actual MariaDB version from server
        self.mysql_engine.get_version().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::{ConnectionConfig, PoolConfig, SecurityConfig};
    use std::collections::HashMap;

    fn create_test_config() -> DatabaseConfig {
        DatabaseConfig {
            database_type: DatabaseType::MariaDB,
            connection: ConnectionConfig {
                host: "localhost".to_string(),
                port: 3306,
                database: "test".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                ssl_mode: None,
                timeout_seconds: 30,
                retry_attempts: 3,
                options: HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_mariadb_engine_creation() {
        let config = create_test_config();
        let engine = MariaDbEngine::new_without_security(config).await.unwrap();

        assert_eq!(engine.engine_type(), DatabaseType::MariaDB);
    }

    #[tokio::test]
    async fn test_mariadb_supported_features() {
        let config = create_test_config();
        let engine = MariaDbEngine::new_without_security(config).await.unwrap();

        let features = engine.supported_features();
        assert!(!features.is_empty());
        assert!(features.contains(&DatabaseFeature::JsonSupport));
    }

    #[test]
    fn test_mariadb_config() {
        let config = create_test_config();
        assert_eq!(config.database_type, DatabaseType::MariaDB);
        assert_eq!(config.connection.port, 3306);
    }
}
