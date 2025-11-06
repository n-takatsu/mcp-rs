//! Database Engines Module
//!
//! 各種データベースエンジンの実装を提供

pub mod postgresql;
pub mod sqlite;
pub mod mysql;

// エンジンを直接アクセス可能にする
pub use mysql::MySqlEngine;
pub use postgresql::PostgreSqlEngine;
pub use sqlite::SqliteEngine;

// 将来の実装予定
// pub mod mongodb;
// pub mod redis;