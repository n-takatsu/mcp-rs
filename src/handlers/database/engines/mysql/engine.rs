//! MySQL Engine Implementation
//!
//! Simple MySQL engine implementation for testing purposes

use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    security::DatabaseSecurity,
    types::{
        DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType,
        ExecuteResult, HealthStatus, QueryResult, TableInfo, Value,
    },
};
use async_trait::async_trait;
use std::sync::Arc;

/// MySQL Database Engine (Test Implementation)
#[derive(Clone)]
pub struct MySqlEngine {
    config: DatabaseConfig,
    security: Arc<DatabaseSecurity>,
}

impl MySqlEngine {
    /// Create new MySQL engine with security integration
    pub async fn new(
        config: DatabaseConfig,
        security: Arc<DatabaseSecurity>,
    ) -> Result<Self, DatabaseError> {
        Ok(Self { config, security })
    }

    /// Get database config
    pub fn get_config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// Create MySQL engine without security (for testing)
    pub async fn new_without_security(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        use crate::handlers::database::{security::DatabaseSecurity, types::SecurityConfig};
        use std::sync::Arc;

        // デフォルトのセキュリティ設定でDatabaseSecurityを作成
        let security_config = SecurityConfig::default();
        let security = Arc::new(DatabaseSecurity::new(security_config, None));
        Ok(Self { config, security })
    }
}

#[async_trait]
impl DatabaseEngine for MySqlEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn connect(
        &self,
        _config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        // Mock implementation for testing
        Err(DatabaseError::UnsupportedOperation(
            "Mock implementation - connect not supported".to_string(),
        ))
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
            DatabaseFeature::StoredProcedures,
            DatabaseFeature::PreparedStatements,
        ]
    }

    fn validate_config(&self, _config: &DatabaseConfig) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        Ok("MySQL Test Engine 1.0".to_string())
    }
}
