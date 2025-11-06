//! Database Handler Module
//!
//! 様々なデータベースエンジンに対する統一的なMCPインターフェースを提供

pub mod engine;
pub mod handler;
pub mod pool;
pub mod safety;   // 安全機構モジュールを追加
pub mod security;
pub mod types;

// エンジン実装
pub mod engines {
    pub mod postgresql;
    // 将来実装予定
    // pub mod mysql;
    // pub mod sqlite;
    // pub mod mongodb;
}

// テストモジュール
#[cfg(test)]
pub mod basic_tests;
#[cfg(test)]
pub mod simple_test;
#[cfg(test)]
pub mod tests;
#[cfg(test)]
pub mod ultra_simple;

// 公開API
pub use engine::{DatabaseConnection, DatabaseEngine, DatabaseTransaction};
pub use handler::DatabaseHandler;
pub use security::DatabaseSecurity;
pub use types::{DatabaseConfig, DatabaseFeature, DatabaseType, ExecuteResult, QueryResult};
