//! Database Handler Module
//!
//! 様々なデータベースエンジンに対する統一的なMCPインターフェースを提供

pub mod engine;
pub mod handler;
pub mod pool;
pub mod safety; // 安全機構モジュールを追加
pub mod security;
pub mod types;

// 新しい可用性関連モジュール
pub mod availability; // 可用性管理
pub mod loadbalancer;
pub mod retry; // リトライ戦略 // 負荷分散
               // pub mod integrated_availability; // 統合可用性システム（一時的に無効化）

// 高度なセキュリティモジュール
pub mod advanced_security; // 高度なセキュリティ機能（MFA、RBAC、異常検知等）
pub mod integrated_security;
pub mod security_config; // 拡張セキュリティ設定 // 統合セキュリティマネージャー

// エンジン実装
pub mod engines;

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

// 高度なセキュリティAPI
pub use advanced_security::{
    AnomalyDetector, ColumnEncryption, MultiFactorAuth, RoleBasedAccessControl,
};
pub use integrated_security::{IntegratedSecurityManager, SecurityCheckResult};
pub use security_config::AdvancedSecurityConfig;
