//! PostgreSQL-specific error types

use std::fmt;

/// PostgreSQL-specific error types
#[derive(Debug)]
pub enum PostgresError {
    /// Connection error
    Connection(String),
    /// Query execution error
    QueryExecution(String),
    /// Transaction error
    Transaction(String),
    /// Configuration error
    Configuration(String),
    /// Pool error
    Pool(String),
    /// Invalid parameter
    InvalidParameter(String),
    /// JSONB operation error
    JsonbError(String),
    /// Notification error
    NotificationError(String),
    /// Migration error
    Migration(String),
    /// Serialization error
    Serialization(String),
    /// sqlx error
    Sqlx(sqlx::Error),
}

impl fmt::Display for PostgresError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(msg) => write!(f, "PostgreSQL connection error: {}", msg),
            Self::QueryExecution(msg) => write!(f, "PostgreSQL query execution error: {}", msg),
            Self::Transaction(msg) => write!(f, "PostgreSQL transaction error: {}", msg),
            Self::Configuration(msg) => write!(f, "PostgreSQL configuration error: {}", msg),
            Self::Pool(msg) => write!(f, "PostgreSQL pool error: {}", msg),
            Self::InvalidParameter(msg) => write!(f, "Invalid parameter: {}", msg),
            Self::JsonbError(msg) => write!(f, "JSONB operation error: {}", msg),
            Self::NotificationError(msg) => write!(f, "Notification error: {}", msg),
            Self::Migration(msg) => write!(f, "Migration error: {}", msg),
            Self::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            Self::Sqlx(err) => write!(f, "sqlx error: {}", err),
        }
    }
}

impl std::error::Error for PostgresError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlx(err) => Some(err),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for PostgresError {
    fn from(err: sqlx::Error) -> Self {
        Self::Sqlx(err)
    }
}

impl From<sqlx::migrate::MigrateError> for PostgresError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(err.to_string())
    }
}

impl From<serde_json::Error> for PostgresError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Result type for PostgreSQL operations
pub type Result<T> = std::result::Result<T, PostgresError>;
