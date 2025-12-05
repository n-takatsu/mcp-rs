//! MongoDB Engine Implementation
//!
//! Main MongoDB engine entry point

use super::{config::MongoConfig, connection::MongoConnection, convert_mongodb_error};
use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    types::{
        DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType, HealthStatus,
        HealthStatusType,
    },
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;

/// MongoDB Database Engine
#[derive(Clone)]
pub struct MongoEngine {
    config: DatabaseConfig,
    mongo_config: Arc<MongoConfig>,
}

impl MongoEngine {
    /// Create new MongoDB engine
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        // Build MongoDB URI from config
        let uri = format!(
            "mongodb://{}:{}@{}:{}",
            config.connection.username,
            config.connection.password,
            config.connection.host,
            config.connection.port
        );

        let mongo_config = MongoConfig::new(uri, config.connection.database.clone());
        mongo_config.validate()?;

        Ok(Self {
            config,
            mongo_config: Arc::new(mongo_config),
        })
    }

    /// Create MongoDB engine with custom config
    pub async fn new_with_config(
        config: DatabaseConfig,
        mongo_config: MongoConfig,
    ) -> Result<Self, DatabaseError> {
        mongo_config.validate()?;

        Ok(Self {
            config,
            mongo_config: Arc::new(mongo_config),
        })
    }

    /// Get MongoDB configuration
    pub fn mongo_config(&self) -> &MongoConfig {
        &self.mongo_config
    }
}

#[async_trait]
impl DatabaseEngine for MongoEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::MongoDB
    }

    async fn connect(
        &self,
        _config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let connection = MongoConnection::new(self.mongo_config.clone()).await?;
        Ok(Box::new(connection))
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        let start = std::time::Instant::now();

        // Try to create a connection and ping (ping is done during connection)
        let result = MongoConnection::new(self.mongo_config.clone()).await;

        let response_time_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(_) => Ok(HealthStatus {
                status: HealthStatusType::Healthy,
                last_check: Utc::now(),
                response_time_ms,
                error_message: None,
                connection_count: 0,
                active_transactions: 0,
            }),
            Err(e) => Ok(HealthStatus {
                status: HealthStatusType::Critical,
                last_check: Utc::now(),
                response_time_ms,
                error_message: Some(e.to_string()),
                connection_count: 0,
                active_transactions: 0,
            }),
        }
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::JsonSupport,
            DatabaseFeature::Sharding,
            DatabaseFeature::Replication,
            DatabaseFeature::EventualConsistency,
            DatabaseFeature::DocumentStore,
            DatabaseFeature::AggregationPipeline,
            DatabaseFeature::GridFS,
            DatabaseFeature::FullTextSearch,
            DatabaseFeature::Geospatial,
        ]
    }

    fn validate_config(&self, config: &DatabaseConfig) -> Result<(), DatabaseError> {
        if config.connection.host.is_empty() {
            return Err(DatabaseError::ConfigurationError(
                "MongoDB host cannot be empty".to_string(),
            ));
        }
        if config.connection.database.is_empty() {
            return Err(DatabaseError::ConfigurationError(
                "MongoDB database name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        let connection = MongoConnection::new(self.mongo_config.clone()).await?;
        connection.get_server_version().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mongo_engine_creation() {
        let config = DatabaseConfig {
            database_type: DatabaseType::MongoDB,
            connection: crate::handlers::database::types::ConnectionConfig {
                host: "localhost".to_string(),
                port: 27017,
                database: "test".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let engine = MongoEngine::new(config).await.unwrap();
        assert_eq!(engine.engine_type(), DatabaseType::MongoDB);
    }

    #[test]
    fn test_mongo_supported_features() {
        let config = DatabaseConfig {
            database_type: DatabaseType::MongoDB,
            connection: crate::handlers::database::types::ConnectionConfig {
                host: "localhost".to_string(),
                port: 27017,
                database: "test".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        let mongo_config = MongoConfig::new(
            "mongodb://test:test@localhost:27017".to_string(),
            "test".to_string(),
        );

        let engine = MongoEngine {
            config,
            mongo_config: Arc::new(mongo_config),
        };

        let features = engine.supported_features();
        assert!(features.contains(&DatabaseFeature::JsonSupport));
        assert!(features.contains(&DatabaseFeature::DocumentStore));
        assert!(features.contains(&DatabaseFeature::AggregationPipeline));
    }
}
