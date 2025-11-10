//! Generic Handler Trait Extensions
//!
//! 様々な対象システムに対応するためのハンドラー拡張

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 汎用ハンドラー設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerConfig {
    /// ハンドラー名
    pub name: String,
    /// 対象システムタイプ
    pub target_type: TargetType,
    /// 接続設定
    pub connection: ConnectionConfig,
    /// セキュリティ設定
    pub security: SecurityConfig,
    /// カスタム設定
    pub custom: HashMap<String, serde_json::Value>,
}

/// 対象システムタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TargetType {
    WordPress,
    Database(DatabaseType),
    Filesystem,
    CloudService(CloudProvider),
    WebAPI,
    MessageQueue,
    Custom(String),
}

/// データベースタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
    MongoDB,
    Redis,
}

/// クラウドプロバイダー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CloudProvider {
    Aws,
    Gcp,
    Azure,
    DigitalOcean,
}

/// 接続設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// エンドポイントURL
    pub endpoint: String,
    /// 認証情報
    pub auth: AuthConfig,
    /// タイムアウト設定
    pub timeout_seconds: u32,
    /// リトライ設定
    pub retry_attempts: u32,
}

/// 認証設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthConfig {
    None,
    BasicAuth { username: String, password: String },
    ApiKey { key: String, header: Option<String> },
    OAuth2 { token: String },
    Custom(HashMap<String, String>),
}

/// セキュリティ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// TLS使用
    pub use_tls: bool,
    /// 証明書検証
    pub verify_cert: bool,
    /// レート制限
    pub rate_limit: Option<RateLimitConfig>,
    /// IP許可リスト
    pub allowed_ips: Vec<String>,
}

/// レート制限設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

/// 汎用ハンドラートレイト拡張
#[async_trait]
pub trait ExtendedMcpHandler: Send + Sync {
    /// ハンドラータイプを取得
    fn handler_type(&self) -> TargetType;

    /// 健全性チェック
    async fn health_check(&self) -> Result<HealthStatus, crate::mcp::McpError>;

    /// 設定更新
    async fn update_config(&self, config: HandlerConfig) -> Result<(), crate::mcp::McpError>;

    /// 統計情報取得
    async fn get_statistics(&self) -> Result<HandlerStatistics, crate::mcp::McpError>;
}

/// 健全性ステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub response_time_ms: u64,
    pub error_message: Option<String>,
}

/// ハンドラー統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandlerStatistics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_response_time_ms: f64,
    pub uptime_seconds: u64,
}
