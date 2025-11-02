//! # mcp-rs
//!
//! Rust implementation of the Model Context Protocol (MCP) for AI-agent integration.
//!
//! This crate provides a JSON-RPC based server implementation that follows the MCP specification,
//! with support for tool integration, resource management, and prompt handling.

pub mod core;
pub mod config;
pub mod plugins;
pub mod transport;
pub mod sdk;

// Legacy compatibility (deprecated)
pub mod mcp {
    //! Legacy MCP module for backward compatibility
    //! 
    //! This module is deprecated. Use `core` module instead.
    
    pub use crate::core::*;
}

pub mod handlers {
    //! Legacy handlers module for backward compatibility
    //! 
    //! This module is deprecated. Use `plugins` module instead.
    
    pub use crate::plugins::wordpress::*;
}

// Re-export commonly used types for convenience
pub use core::{
    McpError, Tool, Resource, Prompt, Content,
    JsonRpcRequest, JsonRpcResponse, JsonRpcError,
    ToolCallResult, ResourceReadResult, InitializeResult
};

pub use config::{McpConfig, load_config};
pub use plugins::{Plugin, PluginRegistry, ToolProvider, ResourceProvider, PromptProvider};
