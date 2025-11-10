//! Core Runtime Module
//!
//! Provides the central runtime system for MCP-RS, including:
//! - Application lifecycle management
//! - Handler registry and plugin management  
//! - Execution context and resource management
//! - Transport abstraction

pub mod context;
pub mod registry;
pub mod runtime;

pub use registry::{HandlerRegistry, PluginInfo};
pub use runtime::{Runtime, RuntimeConfig};
