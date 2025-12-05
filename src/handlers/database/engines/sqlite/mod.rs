//! SQLite Database Engine
//!
//! Provides SQLite-specific implementations for database operations
//! including prepared statements, transactions, and file-based storage.

pub mod connection;
pub mod engine;
pub mod prepared;
pub mod transaction;

pub use connection::SqliteConnection;
pub use engine::SqliteEngine;
pub use prepared::SqlitePreparedStatement;
pub use transaction::SqliteTransaction;
