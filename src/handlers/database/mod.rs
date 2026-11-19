//! Database Handler Module
//!
//! 様々なデータベースエンジンに対する統一的なMCPインターフェースを提供

pub mod column_provenance;
pub mod engine;
pub mod handler;
pub mod pool;
pub mod safety; // 安全機構モジュールを追加
pub mod security;
pub mod types;

// カラムレベル暗号化モジュール
pub mod column_encryption; // Column-level encryption with key management
pub mod column_encryption_rbac; // RBAC integration for column encryption

// エンジン実装
pub mod engines;

// テストモジュール（tests/ディレクトリに移動済み）
// #[cfg(test)]
// pub mod basic_tests; // → tests/database/engine_tests.rs
// #[cfg(test)]
// pub mod tests; // → tests/database/handler_tests.rs
// 残りのテストファイルも整理予定

// 公開API
pub use engine::{DatabaseConnection, DatabaseEngine, DatabaseTransaction};
pub use handler::{DatabaseHandler, DatabaseToolOptions};
pub use security::DatabaseSecurity;
pub use types::{DatabaseConfig, DatabaseFeature, DatabaseType, ExecuteResult, QueryResult};

// カラムレベル暗号化API
pub use column_encryption::{
    CacheStats, ColumnEncryptionConfig, ColumnEncryptionManager, EncryptionAlgorithm, KeyManager,
    KeyManagerConfig, KeyMetadata, KeyProvider,
};
pub use column_encryption_rbac::{
    ColumnEncryptionPermission, ColumnEncryptionRbac, EncryptionAuditLog, EncryptionOperation,
};
