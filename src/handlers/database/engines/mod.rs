//! Database Engines Module
//!
//! 各種データベースエンジンの実装を提供
//! MySQL: mysql_asyncライブラリを使用してセキュアに復活

#[cfg(feature = "mysql-backend")]
pub mod mariadb; // MariaDB (MySQLベース)
#[cfg(feature = "mongodb-backend")]
pub mod mongodb;
#[cfg(feature = "mysql-backend")]
pub mod mysql; // mysql_asyncライブラリでセキュア復活
pub mod postgresql;
#[cfg(feature = "redis-backend")]
pub mod redis;
pub mod sqlite;

// エンジンを直接アクセス可能にする
#[cfg(feature = "mysql-backend")]
pub use mariadb::MariaDbEngine;
#[cfg(feature = "mongodb-backend")]
pub use mongodb::MongoEngine;
#[cfg(feature = "mysql-backend")]
pub use mysql::MySqlEngine; // mysql_asyncライブラリでセキュア復活
pub use postgresql::PostgreSqlEngine;
#[cfg(feature = "redis-backend")]
pub use redis::RedisEngine;
pub use sqlite::SqliteEngine;
