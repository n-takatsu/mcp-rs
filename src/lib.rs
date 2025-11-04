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
pub mod mcp;
// pub mod plugin_isolation; // 将来実装予定
pub mod plugins;
pub mod policy_watcher;
pub mod protocol;
pub mod security;
pub mod server;
pub mod transport;
pub mod types;

pub use error::{Error, Result};
pub use protocol::McpProtocol;
pub use server::McpServer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = Error::InvalidRequest("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
