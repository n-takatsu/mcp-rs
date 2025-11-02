//! # MCP-RS Plugin Development SDK
//!
//! This module provides utilities and macros to simplify plugin development for mcp-rs.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use mcp_rs_sdk::prelude::*;
//!
//! #[derive(Plugin)]
//! struct MyPlugin {
//!     config: MyPluginConfig,
//! }
//!
//! #[async_trait]
//! impl ToolProvider for MyPlugin {
//!     async fn list_tools(&self) -> PluginResult<Vec<Tool>> {
//!         Ok(vec![
//!             tool!("my_tool", "Description of my tool", {
//!                 "param1": "string",
//!                 "param2": "integer"
//!             })
//!         ])
//!     }
//!
//!     async fn call_tool(&self, name: &str, args: Option<HashMap<String, Value>>) -> PluginResult<Value> {
//!         match name {
//!             "my_tool" => {
//!                 let result = self.handle_my_tool(args).await?;
//!                 Ok(tool_result!(result))
//!             }
//!             _ => Err(McpError::ToolNotFound(name.to_string()))
//!         }
//!     }
//! }
//! ```

pub mod helpers;
pub mod macros;
pub mod testing;

pub mod prelude {
    //! Common imports for plugin development

    pub use async_trait::async_trait;
    pub use serde::{Deserialize, Serialize};
    pub use serde_json::{json, Value};
    pub use std::collections::HashMap;

    pub use crate::core::{
        Content, McpError, MessageRole, Prompt, PromptGetResult, PromptMessage, Resource,
        ResourceContent, ResourceReadResult, Tool, ToolCallResult,
    };

    pub use crate::config::PluginConfig;
    pub use crate::plugins::{
        Plugin, PluginFactory, PluginMetadata, PluginResult, PromptProvider, ResourceProvider,
        ToolProvider,
    };

    pub use super::helpers::*;
}
