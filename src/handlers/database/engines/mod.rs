//! Database Engines Module
//!
//! 各種データベースエンジンの実装を提供

pub mod mongodb;
pub mod mysql;
pub mod postgresql;
pub mod redis;
pub mod sqlite;

// エンジンを直接アクセス可能にする
pub use mongodb::MongoEngine;
pub use mysql::MySqlEngine;
pub use postgresql::PostgreSqlEngine;
pub use redis::RedisEngine;
pub use sqlite::SqliteEngine;

// 将来の実装予定
// pub mod mariadb;
