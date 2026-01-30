//! Kubernetes integration module
//!
//! This module provides Kubernetes operator functionality for MCP plugin management.

pub mod crd;
pub mod operator;
pub mod reconciler;

pub use crd::{PluginDeployment, PluginDeploymentSpec, PluginPolicy, PluginPolicySpec, ResourceLimits};
pub use operator::{PluginOperator, OperatorConfig};
pub use reconciler::PluginReconciler;

use crate::error::McpError;

pub type Result<T> = std::result::Result<T, McpError>;
