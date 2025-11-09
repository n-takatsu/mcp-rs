//! # MCP-RS: Model Context Protocol Implementation in Rust
//!
//! This library provides a complete implementation of the Model Context Protocol (MCP)
//! in Rust, enabling secure and efficient communication between AI systems and external
//! resources like WordPress sites, databases, and APIs.

#![allow(dead_code)] // Allow unused code for future extensibility
#![allow(unused_imports)] // Allow unused imports during development

pub mod config;
pub mod core;
pub mod error;
pub mod handlers;
pub mod logging;
// pub mod http_server; // 削除: Transport統合により不要
pub mod mcp;
pub mod mcp_server;
pub mod server;
pub mod session;
pub mod threat_intelligence;
// pub mod plugin_isolation; // 将来実装予定
pub mod canary_deployment;
// #[cfg(feature = "tui")]
// pub mod dashboard;
pub mod plugins;
pub mod policy_application;
pub mod policy_config;
pub mod policy_validation;
pub mod policy_watcher;
pub mod protocol;
pub mod rollback;
pub mod security;
pub mod transport;
pub mod types;

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
