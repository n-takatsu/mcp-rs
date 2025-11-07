//! Session Tracking System for MCP-RS
//!
//! 高性能でセキュアなセッション管理システム。
//! リアルタイムトラッキング、自動クリーンアップ、セキュリティ統合を提供。
//!
//! # Features
//!
//! - **Multi-backend Storage**: Redis, Database, Memory対応
//! - **Security Integration**: 既存セキュリティシステムとの統合
//! - **Real-time Analytics**: セッション統計とパフォーマンス監視
//! - **Auto Cleanup**: 期限切れセッションの自動削除
//! - **MCP Tools**: セッション管理用MCPツール群

pub mod types;
pub mod storage;
pub mod manager;
pub mod tools;
pub mod database;
pub mod security_integration;
pub mod middleware;
pub mod websocket_handler;
pub mod mcp_integration;

// 公開API
pub use manager::{SessionManager, SessionManagerConfig};
pub use storage::{SessionStorage, MemorySessionStorage};
pub use tools::SessionTools;
pub use security_integration::{
    SessionSecurityIntegration, SessionSecurityConfig, SessionSecurityValidationResult,
    SecurityEventType, SecuritySeverity, SecurityAction, SecurityValidationLevel
};
pub use middleware::{
    SessionSecurityMiddleware, SessionSecurityMiddlewareConfig, 
    ViolationResponseConfig, session_security_middleware
};
pub use websocket_handler::{
    SessionWebSocketHandler, WebSocketHandlerConfig, WebSocketConnectionManager,
    BroadcastMessage, BroadcastMessageType, ConnectionType, ClientCapabilities
};
pub use mcp_integration::{
    SessionAwareMcpHandler, SessionAwareMcpConfig, SessionOperationContext,
    OperationLogLevel, ErrorSessionHandling, ClientInfo
};
pub use types::{
    Session, SessionId, SessionState, SessionMetadata, SessionFilter,
    CreateSessionRequest, SessionStats, SecurityLevel, GeoLocation
};

use crate::error::McpError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// セッション管理システムの設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// デフォルトセッション有効期限
    pub default_ttl: Duration,
    /// 最大同時セッション数
    pub max_concurrent_sessions: u32,
    /// クリーンアップ間隔
    pub cleanup_interval: Duration,
    /// セキュリティ設定
    pub security: SessionSecurityConfig,
    /// ストレージ設定
    pub storage: SessionStorageConfig,
    /// 分析機能
    pub analytics_enabled: bool,
}

/// セッションセキュリティ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSecurityConfig {
    /// IP固定機能
    pub bind_to_ip: bool,
    /// User-Agent検証
    pub validate_user_agent: bool,
    /// セッション暗号化
    pub encryption_enabled: bool,
    /// 最大アイドル時間
    pub max_idle_time: Duration,
    /// セッション更新間隔
    pub refresh_threshold: Duration,
}

/// セッションストレージ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStorageConfig {
    /// ストレージバックエンド
    pub backend: String, // "memory", "redis", "database"
    /// 接続設定
    pub connection_config: Option<serde_json::Value>,
    /// キャッシュ設定
    pub cache_enabled: bool,
    /// バックアップ設定
    pub backup_enabled: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_hours(2),
            max_concurrent_sessions: 10000,
            cleanup_interval: Duration::from_mins(5),
            security: SessionSecurityConfig::default(),
            storage: SessionStorageConfig::default(),
            analytics_enabled: true,
        }
    }
}

impl Default for SessionSecurityConfig {
    fn default() -> Self {
        Self {
            bind_to_ip: true,
            validate_user_agent: true,
            encryption_enabled: true,
            max_idle_time: Duration::from_mins(30),
            refresh_threshold: Duration::from_mins(15),
        }
    }
}

impl Default for SessionStorageConfig {
    fn default() -> Self {
        Self {
            backend: "memory".to_string(),
            connection_config: None,
            cache_enabled: true,
            backup_enabled: false,
        }
    }
}

/// セッション管理エラー
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(String),
    
    #[error("Session expired: {0}")]
    Expired(String),
    
    #[error("Session validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<SessionError> for McpError {
    fn from(err: SessionError) -> Self {
        McpError::Internal(format!("Session error: {}", err))
    }
}

// Duration便利拡張
trait DurationExt {
    fn from_mins(mins: u64) -> Duration;
    fn from_hours(hours: u64) -> Duration;
}

impl DurationExt for Duration {
    fn from_mins(mins: u64) -> Duration {
        Duration::from_secs(mins * 60)
    }
    
    fn from_hours(hours: u64) -> Duration {
        Duration::from_secs(hours * 3600)
    }
}