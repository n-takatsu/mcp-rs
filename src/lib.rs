//! # mcp-rs
//!
//! Rust implementation of the Model Context Protocol (MCP) for AI-agent integration.
//!
//! This crate provides a JSON-RPC based server implementation that follows the MCP specification,
//! with support for tool integration, resource management, and prompt handling.
//!
//! ## Quick Start
//!
//! ```rust
//! use mcp_rs::config::McpConfig;
//! use mcp_rs::core::{Runtime, RuntimeConfig};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load configuration
//! let mcp_config = McpConfig::default();
//!
//! // Create runtime configuration
//! let runtime_config = RuntimeConfig {
//!     mcp_config,
//!     max_concurrent_requests: 10,
//!     default_timeout_seconds: 30,
//!     enable_metrics: false,
//! };
//!
//! // Create and initialize runtime
//! let runtime = Runtime::new(runtime_config);
//! runtime.initialize().await?;
//!
//! println!("MCP-RS server initialized successfully!");
//! # Ok(())
//! # }
//! ```
//!
//! ## Plugin System
//!
//! ```rust
//! use mcp_rs::plugins::{DynamicPluginRegistry, discover_plugins};
//! use mcp_rs::config::{McpConfig, PluginConfig};
//! use mcp_rs::core::HandlerRegistry;
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//! use std::path::Path;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Discover available plugins
//! let plugins = discover_plugins(Path::new(".")).await?;
//! println!("Found {} plugins", plugins.len());
//!
//! // Set up plugin registry  
//! let config = PluginConfig::default();
//! let handler_registry = Arc::new(RwLock::new(HandlerRegistry::new()));
//! let mut registry = DynamicPluginRegistry::new(config, handler_registry);
//!
//! registry.initialize().await?;
//! registry.discover_all_plugins().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Documentation
//!
//! This crate uses **executable documentation** - all examples are automatically tested:
//!
//! - Run `cargo doc --open` to view comprehensive API documentation
//! - Run `cargo test --doc` to verify all examples work correctly
//! - See individual module documentation for detailed usage patterns
//!
//! ## Architecture
//!
//! The crate is organized into several key modules:
//!
//! - [`config`] - Configuration management with environment variable support
//! - [`core`] - Runtime system and handler registry  
//! - [`plugins`] - Dynamic plugin loading system
//! - [`transport`] - Communication layer (stdio, HTTP, WebSocket)
//! - [`handlers`] - Built-in handlers (WordPress, etc.)
//! - [`security`] - Security features (rate limiting, authentication)
//! - [`mcp`] - MCP protocol types and utilities

pub mod config;
pub mod core;
pub mod error;
pub mod handlers;
pub mod mcp;
pub mod plugins;
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
