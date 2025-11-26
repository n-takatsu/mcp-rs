//! Redis engine implementation for mcp-rs
//! Provides high-performance in-memory data operations with Sorted Set support
//! and command restriction features for enterprise security.

pub mod command_restrict;
pub mod connection;
pub mod sorted_set;
pub mod types;

pub use command_restrict::CommandRestrictor;
pub use connection::RedisConnection;
pub use sorted_set::SortedSetOperations;
pub use types::{RedisCommand, RedisConfig, RedisValue};

use crate::handlers::database::{
    engine::{DatabaseConnection, DatabaseEngine},
    types::{
        DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType,
        ExecuteResult, HealthStatus, HealthStatusType, QueryResult, Value,
    },
};
use async_trait::async_trait;
use chrono::Utc;

/// Redis engine for in-memory data operations
pub struct RedisEngine {
    config: RedisConfig,
    connection: RedisConnection,
    command_restrictor: CommandRestrictor,
}

impl RedisEngine {
    /// Create new Redis engine instance
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        // Convert DatabaseConfig to RedisConfig
        let redis_config = RedisConfig {
            host: config.connection.host.clone(),
            port: config.connection.port,
            database: config.connection.database.parse().unwrap_or(0),
            password: if config.connection.password.is_empty() {
                None
            } else {
                Some(config.connection.password.clone())
            },
            use_tls: config.connection.ssl_mode.is_some(),
            ..Default::default()
        };

        let connection = RedisConnection::connect(&redis_config).await?;
        let command_restrictor = CommandRestrictor::new();

        Ok(RedisEngine {
            config: redis_config,
            connection,
            command_restrictor,
        })
    }

    /// Check if command is allowed
    pub fn is_command_allowed(&self, cmd: &RedisCommand) -> bool {
        self.command_restrictor.is_allowed(cmd)
    }

    /// Get current command whitelist
    pub fn get_command_whitelist(&self) -> Vec<String> {
        self.command_restrictor.get_whitelist()
    }

    /// Update command whitelist
    pub fn set_command_whitelist(&mut self, whitelist: Vec<String>) -> Result<(), DatabaseError> {
        self.command_restrictor.set_whitelist(whitelist)
    }
}

#[async_trait]
impl DatabaseEngine for RedisEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::Redis
    }

    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        // Convert DatabaseConfig to RedisConfig
        let redis_config = RedisConfig {
            host: config.connection.host.clone(),
            port: config.connection.port,
            database: config.connection.database.parse().unwrap_or(0),
            password: if config.connection.password.is_empty() {
                None
            } else {
                Some(config.connection.password.clone())
            },
            use_tls: config.connection.ssl_mode.is_some(),
            ..Default::default()
        };

        let conn = RedisConnection::connect(&redis_config).await?;
        Ok(Box::new(conn))
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        self.connection
            .health_check()
            .await
            .map(|_| HealthStatus {
                status: HealthStatusType::Healthy,
                last_check: Utc::now(),
                response_time_ms: 0,
                error_message: None,
                connection_count: 1,
                active_transactions: 0,
            })
            .map_err(|_| DatabaseError::ConnectionFailed("Redis health check failed".to_string()))
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::InMemoryStorage,
            DatabaseFeature::Transactions,
            DatabaseFeature::KeyValueStore,
        ]
    }

    fn validate_config(&self, config: &DatabaseConfig) -> Result<(), DatabaseError> {
        if config.connection.host.is_empty() {
            return Err(DatabaseError::ConfigurationError(
                "Redis host cannot be empty".to_string(),
            ));
        }
        if config.connection.port == 0 {
            return Err(DatabaseError::ConfigurationError(
                "Redis port must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        Ok("7.0.0".to_string()) // Redis バージョンは別途取得可能
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_engine_creation() {
        // Test placeholder - actual implementation depends on redis-rs
    }
}
