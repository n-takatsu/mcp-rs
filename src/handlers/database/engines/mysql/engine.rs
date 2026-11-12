//! MySQL Engine Implementation
//!
//! Simple MySQL engine implementation for testing purposes

use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    types::{DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseType, HealthStatus},
};
use async_trait::async_trait;

/// MySQL Database Engine (Test Implementation)
#[derive(Clone)]
pub struct MySqlEngine {
    config: DatabaseConfig,
}

impl MySqlEngine {
    /// Create new MySQL engine
    ///
    /// The engine does not check queries itself. It used to take a
    /// `DatabaseSecurity` that it never used, and had a `new_without_security`
    /// that behaved the same (#499).
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        Ok(Self { config })
    }

    /// Get database config
    pub fn get_config(&self) -> &DatabaseConfig {
        &self.config
    }
}

#[async_trait]
impl DatabaseEngine for MySqlEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        use super::MySqlConnection;

        // Build connection string from config
        let connection_string = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.connection.username,
            config.connection.password,
            config.connection.host,
            config.connection.port,
            config.connection.database
        );

        let conn = MySqlConnection::new(&connection_string).await?;
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
