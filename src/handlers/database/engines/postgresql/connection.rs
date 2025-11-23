//! PostgreSQL Connection Management
//!
//! Handles PostgreSQL connection pooling and lifecycle management.

use crate::handlers::database::types::DatabaseError;
use std::sync::Arc;

/// PostgreSQL Connection Configuration
#[derive(Clone, Debug)]
pub struct PostgreSqlConfig {
    /// Database server host
    pub host: String,
    /// Database server port
    pub port: u16,
    /// Database name
    pub database: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
    /// Maximum pool connections
    pub max_connections: u32,
}

impl Default for PostgreSqlConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "postgres".to_string(),
            username: "postgres".to_string(),
            password: String::new(),
            max_connections: 10,
        }
    }
}

impl PostgreSqlConfig {
    /// Create connection string for PostgreSQL
    pub fn connection_string(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database
        )
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), DatabaseError> {
        if self.host.is_empty() {
            return Err(DatabaseError::ValidationError(
                "PostgreSQL host cannot be empty".to_string(),
            ));
        }

        if self.port == 0 || self.port > 65535 {
            return Err(DatabaseError::ValidationError(format!(
                "Invalid PostgreSQL port: {}",
                self.port
            )));
        }

        if self.database.is_empty() {
            return Err(DatabaseError::ValidationError(
                "PostgreSQL database name cannot be empty".to_string(),
            ));
        }

        if self.max_connections == 0 || self.max_connections > 1000 {
            return Err(DatabaseError::ValidationError(format!(
                "PostgreSQL max_connections must be 1-1000, got {}",
                self.max_connections
            )));
        }

        Ok(())
    }
}

/// PostgreSQL Connection Placeholder
///
/// In production, this would wrap sqlx::PgPool
#[derive(Clone)]
pub struct PostgreSqlConnection {
    config: Arc<PostgreSqlConfig>,
}

impl PostgreSqlConnection {
    /// Create a new PostgreSQL connection wrapper
    pub fn new(config: PostgreSqlConfig) -> Result<Self, DatabaseError> {
        config.validate()?;

        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// Get connection configuration
    pub fn config(&self) -> &PostgreSqlConfig {
        &self.config
    }

    /// Get connection string
    pub fn connection_string(&self) -> String {
        self.config.connection_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_config_default() {
        let config = PostgreSqlConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.database, "postgres");
        assert_eq!(config.max_connections, 10);
    }

    #[test]
    fn test_connection_string_generation() {
        let config = PostgreSqlConfig {
            host: "db.example.com".to_string(),
            port: 5432,
            database: "myapp".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
            max_connections: 20,
        };

        let conn_str = config.connection_string();
        assert!(conn_str.contains("admin:secret"));
        assert!(conn_str.contains("db.example.com"));
        assert!(conn_str.contains(":5432"));
        assert!(conn_str.contains("myapp"));
    }

    #[test]
    fn test_config_validation_invalid_port() {
        let mut config = PostgreSqlConfig::default();
        config.port = 0;

        assert!(config.validate().is_err());

        config.port = 70000;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_invalid_max_connections() {
        let mut config = PostgreSqlConfig::default();
        config.max_connections = 0;

        assert!(config.validate().is_err());

        config.max_connections = 2000;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_connection_creation() {
        let config = PostgreSqlConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            max_connections: 10,
        };

        let conn = PostgreSqlConnection::new(config);
        assert!(conn.is_ok());
    }

    #[test]
    fn test_connection_creation_invalid_config() {
        let mut config = PostgreSqlConfig::default();
        config.host = String::new();

        let conn = PostgreSqlConnection::new(config);
        assert!(conn.is_err());
    }
}
