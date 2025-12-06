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

// 動的データベース切り替えモジュール
pub mod dynamic_engine; // 動的エンジン切り替え機能
pub mod dynamic_tools; // 動的切り替えMCPツール

// データマスキングモジュール
pub mod data_masking; // データマスキングエンジン
pub mod masking_formatters; // マスキングフォーマッタ
pub mod masking_rules; // マスキングルール定義

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
pub use handler::DatabaseHandler;
pub use security::DatabaseSecurity;
pub use types::{DatabaseConfig, DatabaseFeature, DatabaseType, ExecuteResult, QueryResult};

// 高度なセキュリティAPI
pub use advanced_security::{
    AnomalyDetector, ColumnEncryption, MultiFactorAuth, RoleBasedAccessControl,
};
pub use integrated_security::{IntegratedSecurityManager, SecurityCheckResult};
pub use security_config::AdvancedSecurityConfig;

// データマスキングAPI
pub use data_masking::{AuditEntry, DataMaskingEngine, MaskingStatistics};
pub use masking_formatters::{MaskingFormatter, PredefinedFormatters};
pub use masking_rules::{
    ColumnPattern, DataType, HashAlgorithm, MaskingContext, MaskingPolicy, MaskingPurpose,
    MaskingRule, MaskingType, NetworkConstraints, TimeConstraints, TimeRange,
};
