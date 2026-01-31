//! PostgreSQL configuration

use crate::handlers::database::engines::postgres::error::{PostgresError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// PostgreSQL connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    /// Database host
    pub host: String,
    /// Database port
    pub port: u16,
    /// Database name
    pub database: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
    /// Minimum number of connections in the pool
    pub min_connections: u32,
    /// Maximum number of connections in the pool
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Idle timeout in seconds
    pub idle_timeout_secs: u64,
    /// Maximum connection lifetime in seconds
    pub max_lifetime_secs: u64,
    /// Maximum size of the prepared statement cache
    pub statement_cache_capacity: usize,
    /// Enable SSL/TLS
    pub ssl_mode: SslMode,
    /// Application name for PostgreSQL logs
    pub application_name: String,
}

/// SSL/TLS mode for PostgreSQL connections
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SslMode {
    /// Disable SSL/TLS
    Disable,
    /// Prefer SSL/TLS but allow plain connections
    Prefer,
    /// Require SSL/TLS
    Require,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "postgres".to_string(),
            username: "postgres".to_string(),
            password: String::new(),
            min_connections: 5,
            max_connections: 100,
            connection_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
            statement_cache_capacity: 1000,
            ssl_mode: SslMode::Prefer,
            application_name: "mcp-rs".to_string(),
        }
    }
}

impl PostgresConfig {
    /// Create a new configuration builder
    pub fn builder() -> PostgresConfigBuilder {
        PostgresConfigBuilder::default()
    }

    /// Build a PostgreSQL connection string
    pub fn connection_string(&self) -> String {
        let ssl_mode_str = match self.ssl_mode {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
        };

        format!(
            "postgresql://{}:{}@{}:{}/{}?sslmode={}&application_name={}",
            self.username,
            self.password,
            self.host,
            self.port,
            self.database,
            ssl_mode_str,
            self.application_name
        )
    }

    /// Get connection timeout as Duration
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.connection_timeout_secs)
    }

    /// Get idle timeout as Duration
    pub fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.idle_timeout_secs)
    }

    /// Get max lifetime as Duration
    pub fn max_lifetime(&self) -> Duration {
        Duration::from_secs(self.max_lifetime_secs)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.host.is_empty() {
            return Err(PostgresError::Configuration(
                "Host cannot be empty".to_string(),
            ));
        }

        if self.database.is_empty() {
            return Err(PostgresError::Configuration(
                "Database cannot be empty".to_string(),
            ));
        }

        if self.username.is_empty() {
            return Err(PostgresError::Configuration(
                "Username cannot be empty".to_string(),
            ));
        }

        if self.min_connections > self.max_connections {
            return Err(PostgresError::Configuration(
                "min_connections cannot be greater than max_connections".to_string(),
            ));
        }

        if self.max_connections == 0 {
            return Err(PostgresError::Configuration(
                "max_connections must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Builder for PostgresConfig
#[derive(Debug, Default)]
pub struct PostgresConfigBuilder {
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    username: Option<String>,
    password: Option<String>,
    min_connections: Option<u32>,
    max_connections: Option<u32>,
    connection_timeout_secs: Option<u64>,
    idle_timeout_secs: Option<u64>,
    max_lifetime_secs: Option<u64>,
    statement_cache_capacity: Option<usize>,
    ssl_mode: Option<SslMode>,
    application_name: Option<String>,
}

impl PostgresConfigBuilder {
    /// Set the database host
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Set the database port
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the database name
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// Set the username
    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// Set the password
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Set the minimum number of connections
    pub fn min_connections(mut self, min: u32) -> Self {
        self.min_connections = Some(min);
        self
    }

    /// Set the maximum number of connections
    pub fn max_connections(mut self, max: u32) -> Self {
        self.max_connections = Some(max);
        self
    }

    /// Set the connection timeout in seconds
    pub fn connection_timeout_secs(mut self, timeout: u64) -> Self {
        self.connection_timeout_secs = Some(timeout);
        self
    }

    /// Set the idle timeout in seconds
    pub fn idle_timeout_secs(mut self, timeout: u64) -> Self {
        self.idle_timeout_secs = Some(timeout);
        self
    }

    /// Set the maximum connection lifetime in seconds
    pub fn max_lifetime_secs(mut self, lifetime: u64) -> Self {
        self.max_lifetime_secs = Some(lifetime);
        self
    }

    /// Set the prepared statement cache capacity
    pub fn statement_cache_capacity(mut self, capacity: usize) -> Self {
        self.statement_cache_capacity = Some(capacity);
        self
    }

    /// Set the SSL mode
    pub fn ssl_mode(mut self, mode: SslMode) -> Self {
        self.ssl_mode = Some(mode);
        self
    }

    /// Set the application name
    pub fn application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = Some(name.into());
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<PostgresConfig> {
        let default = PostgresConfig::default();

        let config = PostgresConfig {
            host: self.host.unwrap_or(default.host),
            port: self.port.unwrap_or(default.port),
            database: self.database.unwrap_or(default.database),
            username: self.username.unwrap_or(default.username),
            password: self.password.unwrap_or(default.password),
            min_connections: self.min_connections.unwrap_or(default.min_connections),
            max_connections: self.max_connections.unwrap_or(default.max_connections),
            connection_timeout_secs: self
                .connection_timeout_secs
                .unwrap_or(default.connection_timeout_secs),
            idle_timeout_secs: self.idle_timeout_secs.unwrap_or(default.idle_timeout_secs),
            max_lifetime_secs: self.max_lifetime_secs.unwrap_or(default.max_lifetime_secs),
            statement_cache_capacity: self
                .statement_cache_capacity
                .unwrap_or(default.statement_cache_capacity),
            ssl_mode: self.ssl_mode.unwrap_or(default.ssl_mode),
            application_name: self.application_name.unwrap_or(default.application_name),
        };

        config.validate()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PostgresConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.max_connections, 100);
    }

    #[test]
    fn test_config_builder() {
        let config = PostgresConfig::builder()
            .host("db.example.com")
            .port(5433)
            .database("mydb")
            .username("user")
            .password("pass")
            .min_connections(10)
            .max_connections(50)
            .build()
            .unwrap();

        assert_eq!(config.host, "db.example.com");
        assert_eq!(config.port, 5433);
        assert_eq!(config.database, "mydb");
        assert_eq!(config.username, "user");
        assert_eq!(config.min_connections, 10);
        assert_eq!(config.max_connections, 50);
    }

    #[test]
    fn test_connection_string() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .database("testdb")
            .username("testuser")
            .password("testpass")
            .ssl_mode(SslMode::Require)
            .build()
            .unwrap();

        let conn_str = config.connection_string();
        assert!(conn_str.contains("postgresql://testuser:testpass@localhost:5432/testdb"));
        assert!(conn_str.contains("sslmode=require"));
    }

    #[test]
    fn test_validation_min_max() {
        let result = PostgresConfig::builder()
            .min_connections(100)
            .max_connections(50)
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_validation_empty_host() {
        let result = PostgresConfig::builder().host("").build();

        assert!(result.is_err());
    }
}
