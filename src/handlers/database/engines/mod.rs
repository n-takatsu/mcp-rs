//! Database Engines Module
//!
//! 各種データベースエンジンの実装を提供
//! MySQL: mysql_asyncライブラリを使用してセキュアに復活

/// `DatabaseConfig::connection.options`のキー: `query()`で受け取る行数の
/// 上限（#453）。PostgreSQL・MySQL（#527）が使う
pub const OPTION_FETCH_LIMIT: &str = "fetch_limit";

/// `DatabaseConfig::connection.options`のキー: `query()`で受け取る結果の
/// だいたいのバイト数の上限（#539）。PostgreSQL・MySQL（#527）が使う
pub const OPTION_BYTE_LIMIT: &str = "byte_limit";

/// `DatabaseConfig::connection.options`のキー: サーバーの証明書を確かめる
/// CAのファイル（#539）。PostgreSQL・MySQL（#527）が使う
pub const OPTION_SSL_ROOT_CERT: &str = "ssl_root_cert";

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
