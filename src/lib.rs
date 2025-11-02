//! # mcp-rs
//!
//! High-performance Rust implementation of the Model Context Protocol (MCP) for AI agent integration.
//!
//! This crate provides a complete implementation of the Model Context Protocol, designed to enable
//! AI agents like Copilot Studio to interact with external systems through a standardized JSON-RPC interface.
//!
//! ## Features
//!
//! - **Full MCP Protocol Support**: Complete implementation of MCP tools, resources, and prompts
//! - **Type-Safe**: Comprehensive type definitions with serde serialization/deserialization
//! - **High Performance**: Built on Tokio for async/await and high-throughput operations
//! - **Extensible**: Modular architecture allowing easy addition of new handlers
//! - **Production Ready**: Comprehensive error handling and logging
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use mcp_rs::{McpServer, handlers::WordPressHandler};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut server = McpServer::new();
//!     
//!     let wordpress_handler = WordPressHandler::new(
//!         "https://your-wordpress-site.com".to_string(),
//!         Some("username".to_string()),
//!         Some("password".to_string()),
//!     );
//!     
//!     server.add_handler("wordpress".to_string(), Arc::new(wordpress_handler));
//!     
//!     // Run on stdio for MCP clients
//!     server.run_stdio().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Architecture
//!
//! The crate is organized into the following modules:
//!
//! - [`mcp`] - Core MCP protocol types and server implementation
//! - [`handlers`] - Protocol handlers for various external systems
//!
//! ## Creating Custom Handlers
//!
//! To create a custom MCP handler, implement the [`McpHandler`] trait:
//!
//! ```rust,no_run
//! use async_trait::async_trait;
//! use mcp_rs::mcp::{McpHandler, McpError, Tool, Resource, InitializeParams, ToolCallParams, ResourceReadParams};
//!
//! pub struct CustomHandler;
//!
//! #[async_trait]
//! impl McpHandler for CustomHandler {
//!     async fn initialize(&self, params: InitializeParams) -> Result<serde_json::Value, McpError> {
//!         // Implementation here
//!         Ok(serde_json::json!({"status": "initialized"}))
//!     }
//!     
//!     async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
//!         // Return available tools
//!         Ok(vec![])
//!     }
//!     
//!     async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
//!         // Handle tool calls
//!         Ok(serde_json::json!({"result": "success"}))
//!     }
//!     
//!     async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
//!         // Return available resources
//!         Ok(vec![])
//!     }
//!     
//!     async fn read_resource(&self, params: ResourceReadParams) -> Result<serde_json::Value, McpError> {
//!         // Handle resource reads
//!         Ok(serde_json::json!({"content": "resource data"}))
//!     }
//! }
//! ```

pub mod mcp;
pub mod handlers;

// Re-export commonly used types for convenience
pub use mcp::{
    McpServer, McpHandler, McpError,
    JsonRpcRequest, JsonRpcResponse, JsonRpcError,
    Tool, Resource, InitializeParams, ToolCallParams, ResourceReadParams
};

pub use handlers::WordPressHandler;