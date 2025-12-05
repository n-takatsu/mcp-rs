//! MongoDB Configuration
//!
//! MongoDB connection and pool configuration

use crate::handlers::database::types::DatabaseError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// MongoDB Connection Configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MongoConfig {
    /// Connection URI
    pub uri: String,
    /// Database name
    pub database: String,
    /// Connection pool options
    pub pool_options: MongoConnectionOptions,
}

impl MongoConfig {
    /// Create new MongoDB configuration
    pub fn new(uri: String, database: String) -> Self {
        Self {
            uri,
            database,
            pool_options: MongoConnectionOptions::default(),
        }
    }

    /// Validate MongoDB configuration
    pub fn validate(&self) -> Result<(), DatabaseError> {
        if self.uri.is_empty() {
            return Err(DatabaseError::ConfigurationError(
                "MongoDB URI cannot be empty".to_string(),
            ));
        }
        if self.database.is_empty() {
            return Err(DatabaseError::ConfigurationError(
                "Database name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// MongoDB Connection Pool Options
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MongoConnectionOptions {
    /// Maximum number of connections in the pool
    pub max_pool_size: u32,
    /// Minimum number of connections in the pool
    pub min_pool_size: u32,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Server selection timeout
    pub server_selection_timeout: Duration,
    /// Maximum connection idle time
    pub max_idle_time: Option<Duration>,
    /// Application name
    pub app_name: Option<String>,
    /// Enable SSL/TLS
    pub tls: bool,
}

impl Default for MongoConnectionOptions {
    fn default() -> Self {
        Self {
            max_pool_size: 100,
            min_pool_size: 5,
            connect_timeout: Duration::from_secs(10),
            server_selection_timeout: Duration::from_secs(30),
            max_idle_time: Some(Duration::from_secs(600)),
            app_name: Some("mcp-rs".to_string()),
            tls: false,
        }
    }
}
