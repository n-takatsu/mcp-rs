//! Plugin System for Dynamic Loading
//!
//! This module provides a complete plugin system for mcp-rs that allows
//! dynamic loading of MCP handlers at runtime.
//!
//! # Examples
//!
//! ## Basic Plugin Discovery
//!
//! ```rust
//! use mcp_rs::plugins::discover_plugins;
//! use std::path::Path;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Discover plugins in current directory (safe for testing)
//! let plugins = discover_plugins(Path::new(".")).await?;
//!
//! for manifest in plugins {
//!     println!("Found plugin: {}", manifest.metadata.name);
//!     println!("  Version: {}", manifest.metadata.version);
//!     println!("  Description: {}", manifest.metadata.description);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Plugin Registry Usage
//!
//! ```rust
//! use mcp_rs::plugins::DynamicPluginRegistry;
//! use mcp_rs::config::{McpConfig, PluginConfig};
//! use mcp_rs::core::HandlerRegistry;
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mcp_config = McpConfig::default();
//! let plugin_config = mcp_config.to_plugin_config().unwrap_or_default();
//! let handler_registry = Arc::new(RwLock::new(HandlerRegistry::new()));
//!
//! let mut registry = DynamicPluginRegistry::new(plugin_config, handler_registry);
//!
//! // Initialize the registry
//! registry.initialize().await?;
//!
//! // Discover plugins from configured paths
//! registry.discover_all_plugins().await?;
//! println!("Plugin discovery completed");
//!
//! // List discovered plugins
//! let discovered_plugins = registry.list_discovered_plugins();
//! println!("Discovered {} plugins", discovered_plugins.len());
//! # Ok(())
//! # }
//! ```

pub mod loader;
pub mod registry;
pub mod types;

pub use loader::PluginLoader;
pub use registry::DynamicPluginRegistry;
pub use types::*;

use crate::mcp::{McpError, McpHandler};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Plugin manifest file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin metadata
    pub metadata: PluginMetadata,
    /// Plugin entry points
    pub entry_points: HashMap<String, String>,
    /// Plugin dependencies
    pub dependencies: Option<Vec<PluginDependency>>,
    /// Minimum mcp-rs version required
    pub min_mcp_version: Option<String>,
    /// Plugin configuration schema
    pub config_schema: Option<serde_json::Value>,
}

/// Plugin metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name (unique identifier)
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
    /// Plugin author
    pub author: Option<String>,
    /// Plugin license
    pub license: Option<String>,
    /// Plugin homepage URL
    pub homepage: Option<String>,
    /// Plugin repository URL
    pub repository: Option<String>,
    /// Plugin tags for categorization
    pub tags: Option<Vec<String>>,
}

/// Plugin dependency specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// Dependency name
    pub name: String,
    /// Version requirement (semver)
    pub version: String,
    /// Whether dependency is optional
    pub optional: Option<bool>,
}

/// Type alias for handler collection to reduce complexity
pub type HandlerCollection = Vec<(String, Arc<dyn McpHandler>)>;

/// Plugin interface that all dynamic plugins must implement
pub trait DynamicPlugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Initialize the plugin with configuration
    fn initialize(&mut self, config: Option<serde_json::Value>) -> Result<(), McpError>;

    /// Create MCP handlers provided by this plugin
    fn create_handlers(&self) -> Result<HandlerCollection, McpError>;

    /// Shutdown the plugin and cleanup resources
    fn shutdown(&mut self) -> Result<(), McpError>;

    /// Validate plugin configuration
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), McpError> {
        // Default implementation - no validation
        let _ = config;
        Ok(())
    }

    /// Get plugin status information
    fn status(&self) -> PluginStatus {
        PluginStatus::Active
    }
}

/// Plugin status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginStatus {
    /// Plugin is not loaded
    NotLoaded,
    /// Plugin is loaded but not initialized
    Loaded,
    /// Plugin is active and providing handlers
    Active,
    /// Plugin encountered an error
    Error,
    /// Plugin is disabled
    Disabled,
}

/// Plugin loading errors
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin loading failed: {0}")]
    LoadingFailed(String),

    #[error("Plugin initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Plugin version incompatible: {0}")]
    VersionIncompatible(String),

    #[error("Plugin dependency missing: {0}")]
    DependencyMissing(String),

    #[error("Plugin manifest invalid: {0}")]
    InvalidManifest(String),

    #[error("Plugin ABI incompatible: {0}")]
    AbiIncompatible(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Dynamic library error: {0}")]
    LibLoading(#[from] libloading::Error),
}

/// Plugin API version for ABI compatibility checking
pub const PLUGIN_API_VERSION: &str = "0.2.0";

/// Plugin ABI compatibility check
pub fn check_plugin_abi_compatibility(plugin_version: &str) -> Result<(), PluginError> {
    // Simple version check - in production, use semver crate
    if plugin_version == PLUGIN_API_VERSION {
        Ok(())
    } else {
        Err(PluginError::AbiIncompatible(format!(
            "Plugin built with API version {} but runtime uses {}",
            plugin_version, PLUGIN_API_VERSION
        )))
    }
}

/// Plugin discovery in directories
pub async fn discover_plugins<P: AsRef<Path>>(
    plugin_dir: P,
) -> Result<Vec<PluginManifest>, PluginError> {
    let mut manifests = Vec::new();
    let mut entries = tokio::fs::read_dir(plugin_dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            let manifest_path = path.join("plugin.toml");
            if manifest_path.exists() {
                let content = tokio::fs::read_to_string(&manifest_path).await?;
                let manifest: PluginManifest = toml::from_str(&content)
                    .map_err(|e| PluginError::InvalidManifest(e.to_string()))?;
                manifests.push(manifest);
            }
        }
    }

    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_api_version_check() {
        assert!(check_plugin_abi_compatibility("0.2.0").is_ok());
        assert!(check_plugin_abi_compatibility("0.1.0").is_err());
        assert!(check_plugin_abi_compatibility("0.3.0").is_err());
    }

    #[test]
    fn test_plugin_manifest_serialization() {
        let manifest = PluginManifest {
            metadata: PluginMetadata {
                name: "test-plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "Test plugin".to_string(),
                author: Some("Test Author".to_string()),
                license: Some("MIT".to_string()),
                homepage: None,
                repository: None,
                tags: Some(vec!["test".to_string()]),
            },
            entry_points: {
                let mut map = HashMap::new();
                map.insert("main".to_string(), "libtest_plugin.so".to_string());
                map
            },
            dependencies: None,
            min_mcp_version: Some("0.2.0".to_string()),
            config_schema: None,
        };

        let serialized = toml::to_string(&manifest).unwrap();
        let deserialized: PluginManifest = toml::from_str(&serialized).unwrap();

        assert_eq!(manifest.metadata.name, deserialized.metadata.name);
        assert_eq!(manifest.metadata.version, deserialized.metadata.version);
    }
}
