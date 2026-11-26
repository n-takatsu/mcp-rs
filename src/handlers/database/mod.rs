//! Database Handler Module
//!
//! 様々なデータベースエンジンに対する統一的なMCPインターフェースを提供

pub mod column_provenance;
pub mod engine;
pub mod handler;
pub mod pool;
#[cfg(feature = "redis-backend")]
pub mod redis_handler;
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

/// `[database]`から、MCPのハンドラーを作る（#529）。Redis（`engine = "redis"`）は
/// Redisのツールのハンドラー、ほかはSQLのツールのハンドラー
pub async fn handler_from_settings(
    settings: &crate::config::DatabaseSettings,
    audit_logger: std::sync::Arc<crate::security::AuditLogger>,
) -> Result<std::sync::Arc<dyn crate::mcp::McpHandler>, String> {
    if settings.is_redis() {
        #[cfg(feature = "redis-backend")]
        {
            let handler = redis_handler::RedisHandler::from_settings(settings)
                .await?
                .with_audit_logger(audit_logger);
            return Ok(std::sync::Arc::new(handler));
        }
        #[cfg(not(feature = "redis-backend"))]
        {
            let _ = audit_logger;
            return Err(REDIS_NOT_BUILT.to_string());
        }
    }
    let handler = DatabaseHandler::from_settings(settings)
        .await?
        .with_audit_logger(audit_logger);
    Ok(std::sync::Arc::new(handler))
}

/// `[database]`を、つながずに確かめる（`mcp-rs check`、#589）。通信を守れない
/// 設定なら、その警告を返す
pub fn check_settings(
    settings: &crate::config::DatabaseSettings,
) -> Result<Option<String>, String> {
    if settings.is_redis() {
        #[cfg(feature = "redis-backend")]
        return redis_handler::check_settings(settings);
        #[cfg(not(feature = "redis-backend"))]
        {
            settings.validate()?;
            return Err(REDIS_NOT_BUILT.to_string());
        }
    }
    DatabaseHandler::check_settings(settings)
}

/// `redis-backend`無しでビルドしたmcp-rsで、Redisを選んだときの理由
#[cfg(not(feature = "redis-backend"))]
const REDIS_NOT_BUILT: &str =
    "this build of mcp-rs does not include Redis support (build it with --features redis-backend)";

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
