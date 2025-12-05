//! MariaDB Database Engine
//!
//! MariaDB is a MySQL-compatible database, so this implementation
//! wraps the MySQL engine with MariaDB-specific configurations.

pub mod engine;

pub use engine::MariaDbEngine;
