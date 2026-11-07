//! # MCP-RS: Model Context Protocol Implementation in Rust
//!
//! This library provides a complete implementation of the Model Context Protocol (MCP)
//! in Rust, enabling secure and efficient communication between AI systems and external
//! resources like WordPress sites, databases, and APIs.

// Allow unused code for future extensibility
#![allow(dead_code)]
// Allow unused imports during development
#![allow(unused_imports)]
// async_traitのマクロ展開が、既に#[must_use]なResultを返すメソッドに
// メッセージ無しの#[must_use]を付けるため、新しいclippyが誤検出する。
// CIでは-A clippy::double_must_useで無効化しているが、エディタ
// （rust-analyzer）にも同じ扱いをさせるためクレート単位で許可する。
// src/main.rsも同じモジュールを読み込むため、同じ許可を付けている。
#![allow(clippy::double_must_use)]

pub mod config;
pub mod core;
pub mod error;
pub mod handlers;
pub mod logging;
// pub mod http_server; // 削除: Transport統合により不要
pub mod canary_deployment;
pub mod mcp;
pub mod mcp_server;
pub mod server;
pub mod session;
pub mod threat_intelligence;
// #[cfg(feature = "tui")]
// pub mod dashboard;
pub mod plugin_proxy;
pub mod policy;
pub mod policy_application;
pub mod policy_config;
pub mod policy_validation;
pub mod policy_watcher;
pub mod protocol;
pub mod rollback;
pub mod security;
pub mod transport;
pub mod types;

// AI Integration module
pub mod ai;

// LLM Integration module
#[cfg(feature = "llm-integration")]
pub mod llm;

// Kubernetes Operator module
#[cfg(feature = "kubernetes-operator")]
pub mod operator;

// CLI module
pub mod cli;

// Monitoring module
pub mod monitoring;

// Analytics module
pub mod analytics;

// Optimization module
pub mod optimization;

// Zero Trust module
pub mod zero_trust;

// Compliance module (GDPR/CCPA)
pub mod compliance;

pub use error::{Error, Result, SessionError};
pub use mcp_server::McpServer;
pub use protocol::McpProtocol;

// Session Management System exports
pub use session::{
    MemorySessionStorage,
    SecurityConfig,
    SecurityEvent,
    SecurityEventType,
    SecuritySeverity,
    Session,
    SessionFilter,
    SessionId,
    SessionManager,
    SessionMiddleware,
    SessionSecurityMiddleware,
    SessionState,
    SessionStorage,
    // Real-time editing system
    SessionWebSocketHandler,
    WebSocketHandlerConfig,
};

// WebSocket Server exports moved to examples
// pub use server::{...};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = Error::InvalidRequest("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
