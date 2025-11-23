//! PostgreSQL Database Engine
//!
//! Provides PostgreSQL-specific implementations for database operations
//! including prepared statements, transactions, and JSON support.

pub mod connection;
pub mod json_support;
pub mod prepared;
pub mod transaction;

pub use connection::{PostgreSqlConfig, PostgreSqlPool};
pub use json_support::PostgreSqlJsonSupport;
pub use prepared::PostgreSqlPreparedStatement;
pub use transaction::{PostgreSqlTransaction, PostgreSqlTransactionManager};

/// PostgreSQL Database Engine
///
/// Main entry point for PostgreSQL database operations
pub struct PostgreSqlEngine;

impl PostgreSqlEngine {
    /// Create a new PostgreSQL engine
    pub async fn new(
        _config: crate::handlers::database::types::DatabaseConfig,
    ) -> Result<Self, crate::handlers::database::types::DatabaseError> {
        Ok(Self)
    }
}
