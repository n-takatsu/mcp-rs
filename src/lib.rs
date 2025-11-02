//! # mcp-rs
//!
//! Rust implementation of the Model Context Protocol (MCP) for AI-agent integration.
//!
//! This crate provides a JSON-RPC based server implementation that follows the MCP specification,
//! with support for tool integration, resource management, and prompt handling.

pub mod config;
pub mod error;
pub mod handlers;
pub mod mcp;
pub mod protocol;
pub mod server;
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
