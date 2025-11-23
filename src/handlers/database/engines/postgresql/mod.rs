//! PostgreSQL Database Engine
//!
//! Provides PostgreSQL-specific implementations for database operations
//! including prepared statements, transactions, and JSON support.

pub mod connection;
pub mod json_support;
pub mod prepared;
pub mod transaction;

pub use connection::PostgreSqlConnection;
pub use json_support::PostgreSqlJsonSupport;
pub use prepared::PostgreSqlPreparedStatement;
pub use transaction::{PostgreSqlTransaction, PostgreSqlTransactionManager};
