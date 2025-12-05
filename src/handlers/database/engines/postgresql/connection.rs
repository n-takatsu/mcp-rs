//! PostgreSQL Connection Management
//!
//! Handles PostgreSQL connection pooling and lifecycle management.

use crate::handlers::database::types::DatabaseError;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;

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
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Idle timeout
    pub idle_timeout: Duration,
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
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
        }
    }
}

impl PostgreSqlConfig {
    /// Create connection string for PostgreSQL
    pub fn connection_string(&self) -> String {
        let timeout_secs = self.connection_timeout.as_secs();
        format!(
            "postgresql://{}:{}@{}:{}/{}?connect_timeout={}",
            self.username, self.password, self.host, self.port, self.database, timeout_secs
        )
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), DatabaseError> {
        if self.host.is_empty() {
            return Err(DatabaseError::ValidationError(
                "PostgreSQL host cannot be empty".to_string(),
            ));
        }

        if self.port == 0 {
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
                self.max_connections as usize
            )));
        }

        Ok(())
    }
}

/// PostgreSQL Connection Pool Manager
///
/// Manages connection pool lifecycle and provides database connections
pub struct PostgreSqlPool {
    pool: Arc<PgPool>,
    config: Arc<PostgreSqlConfig>,
}

impl PostgreSqlPool {
    /// Create a new PostgreSQL connection pool
    ///
    /// # Arguments
    ///
    /// * `config` - PostgreSQL configuration
    ///
    /// # Returns
    ///
    /// Returns a configured pool ready for use
    pub async fn new(config: PostgreSqlConfig) -> Result<Self, DatabaseError> {
        config.validate()?;

        let pool_options = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .max_lifetime(Some(config.connection_timeout))
            .idle_timeout(config.idle_timeout)
            .acquire_timeout(config.connection_timeout);

        let pool = pool_options
            .connect(&config.connection_string())
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            pool: Arc::new(pool),
            config: Arc::new(config),
        })
    }

    /// Get the underlying pool
    pub fn pool(&self) -> Arc<PgPool> {
        Arc::clone(&self.pool)
    }

    /// Get pool configuration
    pub fn config(&self) -> &PostgreSqlConfig {
        &self.config
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            num_idle: self.pool.num_idle() as u32,
            size: self.pool.size(),
        }
    }

    /// Close all connections in the pool
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Number of idle connections
    pub num_idle: u32,
    /// Total connections in pool
    pub size: u32,
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
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
        };

        let conn_str = config.connection_string();
        assert!(conn_str.contains("admin:secret"));
        assert!(conn_str.contains("db.example.com"));
        assert!(conn_str.contains(":5432"));
        assert!(conn_str.contains("myapp"));
        assert!(conn_str.contains("connect_timeout"));
    }

    #[test]
    fn test_config_validation_invalid_port() {
        let config = PostgreSqlConfig {
            port: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        // Port must be in range 1-65535 (u16 max)
        // We can't test > 65535 as it doesn't fit in u16
        // Test boundary: 0 should fail (tested above)
    }

    #[test]
    fn test_config_validation_invalid_max_connections() {
        let mut config = PostgreSqlConfig {
            max_connections: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        config.max_connections = 2000;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_success() {
        let config = PostgreSqlConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            max_connections: 10,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
        };

        assert!(config.validate().is_ok());
    }
}
