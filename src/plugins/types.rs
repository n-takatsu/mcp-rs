//! Plugin Type Definitions
//!
//! Common types and utilities for the plugin system.

pub use super::{
    DynamicPlugin, PluginDependency, PluginError, PluginManifest, PluginMetadata, PluginStatus,
    PLUGIN_API_VERSION,
};

/// Re-export plugin system components
pub use super::loader::PluginLoader;
pub use super::registry::{DynamicPluginRegistry, PluginConfig};

/// Plugin API constants
pub mod api {
    /// Plugin factory function symbol name
    pub const PLUGIN_FACTORY_SYMBOL: &[u8] = b"mcp_plugin_create\0";

    /// Plugin API version function symbol name
    pub const PLUGIN_API_VERSION_SYMBOL: &[u8] = b"mcp_plugin_api_version\0";

    /// Plugin destructor function symbol name
    pub const PLUGIN_DESTRUCTOR_SYMBOL: &[u8] = b"mcp_plugin_destroy\0";
}

/// Convenience macro for defining plugin factory functions
#[macro_export]
macro_rules! export_plugin {
    ($plugin_type:ty) => {
        #[no_mangle]
        pub extern "C" fn mcp_plugin_create() -> *mut std::ffi::c_void {
            let plugin = Box::new(<$plugin_type>::new());
            Box::into_raw(plugin) as *mut std::ffi::c_void
        }

        #[no_mangle]
        pub extern "C" fn mcp_plugin_api_version() -> *const std::os::raw::c_char {
            $crate::plugins::PLUGIN_API_VERSION.as_ptr() as *const std::os::raw::c_char
        }

        #[no_mangle]
        pub extern "C" fn mcp_plugin_destroy(plugin: *mut std::ffi::c_void) {
            if !plugin.is_null() {
                unsafe {
                    let _plugin: Box<$plugin_type> = Box::from_raw(plugin as *mut $plugin_type);
                    // Plugin will be dropped here
                }
            }
        }
    };
}

/// Plugin manifest builder for easier construction
pub struct PluginManifestBuilder {
    manifest: PluginManifest,
}

impl PluginManifestBuilder {
    /// Create a new builder with required fields
    pub fn new(name: String, version: String, description: String) -> Self {
        Self {
            manifest: PluginManifest {
                metadata: PluginMetadata {
                    name,
                    version,
                    description,
                    author: None,
                    license: None,
                    homepage: None,
                    repository: None,
                    tags: None,
                },
                entry_points: std::collections::HashMap::new(),
                dependencies: None,
                min_mcp_version: Some(PLUGIN_API_VERSION.to_string()),
                config_schema: None,
            },
        }
    }

    /// Set the plugin author
    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.manifest.metadata.author = Some(author.into());
        self
    }

    /// Set the plugin license
    pub fn license(mut self, license: impl Into<String>) -> Self {
        self.manifest.metadata.license = Some(license.into());
        self
    }

    /// Set the plugin homepage URL
    pub fn homepage(mut self, homepage: impl Into<String>) -> Self {
        self.manifest.metadata.homepage = Some(homepage.into());
        self
    }

    /// Set the plugin repository URL
    pub fn repository(mut self, repository: impl Into<String>) -> Self {
        self.manifest.metadata.repository = Some(repository.into());
        self
    }

    /// Add tags for categorization
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.manifest.metadata.tags = Some(tags);
        self
    }

    /// Add an entry point
    pub fn entry_point(mut self, name: impl Into<String>, path: impl Into<String>) -> Self {
        self.manifest.entry_points.insert(name.into(), path.into());
        self
    }

    /// Set the main entry point
    pub fn main_entry_point(self, path: impl Into<String>) -> Self {
        self.entry_point("main", path)
    }

    /// Add a dependency
    pub fn dependency(mut self, name: String, version: String, optional: bool) -> Self {
        let dependency = PluginDependency {
            name,
            version,
            optional: Some(optional),
        };

        if let Some(ref mut deps) = self.manifest.dependencies {
            deps.push(dependency);
        } else {
            self.manifest.dependencies = Some(vec![dependency]);
        }

        self
    }

    /// Set minimum MCP version required
    pub fn min_mcp_version(mut self, version: impl Into<String>) -> Self {
        self.manifest.min_mcp_version = Some(version.into());
        self
    }

    /// Set configuration schema
    pub fn config_schema(mut self, schema: serde_json::Value) -> Self {
        self.manifest.config_schema = Some(schema);
        self
    }

    /// Build the manifest
    pub fn build(self) -> PluginManifest {
        self.manifest
    }
}

/// Plugin information for runtime queries
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginRuntimeInfo {
    /// Plugin metadata
    pub metadata: PluginMetadata,
    /// Current plugin status
    pub status: PluginStatus,
    /// Plugin load time
    pub loaded_at: Option<std::time::SystemTime>,
    /// Plugin handlers count
    pub handlers_count: usize,
    /// Plugin configuration
    pub config: Option<serde_json::Value>,
    /// Plugin errors (if any)
    pub errors: Vec<String>,
}

/// Plugin capability flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginCapabilities {
    /// Supports hot reloading
    pub hot_reload: bool,
    /// Supports configuration updates
    pub runtime_config: bool,
    /// Supports graceful shutdown
    pub graceful_shutdown: bool,
    /// Provides metrics
    pub metrics: bool,
    /// Supports health checks
    pub health_check: bool,
}

impl Default for PluginCapabilities {
    fn default() -> Self {
        Self {
            hot_reload: false,
            runtime_config: false,
            graceful_shutdown: true,
            metrics: false,
            health_check: false,
        }
    }
}

/// Plugin health check result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginHealthCheck {
    /// Plugin name
    pub plugin_name: String,
    /// Health status
    pub healthy: bool,
    /// Health check message
    pub message: Option<String>,
    /// Last check time
    pub checked_at: std::time::SystemTime,
    /// Response time in milliseconds
    pub response_time_ms: u64,
}

/// Plugin metrics information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginMetrics {
    /// Plugin name
    pub plugin_name: String,
    /// Number of handler calls
    pub handler_calls: u64,
    /// Number of successful handler calls
    pub handler_successes: u64,
    /// Number of failed handler calls
    pub handler_failures: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Memory usage in bytes (if available)
    pub memory_usage_bytes: Option<u64>,
    /// Last metrics update time
    pub updated_at: std::time::SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest_builder() {
        let manifest = PluginManifestBuilder::new(
            "test-plugin".to_string(),
            "1.0.0".to_string(),
            "Test plugin".to_string(),
        )
        .author("Test Author")
        .license("MIT")
        .main_entry_point("libtest_plugin.so")
        .dependency("dep1".to_string(), "1.0.0".to_string(), false)
        .build();

        assert_eq!(manifest.metadata.name, "test-plugin");
        assert_eq!(manifest.metadata.version, "1.0.0");
        assert_eq!(manifest.metadata.author, Some("Test Author".to_string()));
        assert_eq!(manifest.metadata.license, Some("MIT".to_string()));
        assert!(manifest.entry_points.contains_key("main"));
        assert!(manifest.dependencies.is_some());
        assert_eq!(manifest.dependencies.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_plugin_capabilities_default() {
        let caps = PluginCapabilities::default();
        assert!(!caps.hot_reload);
        assert!(!caps.runtime_config);
        assert!(caps.graceful_shutdown);
        assert!(!caps.metrics);
        assert!(!caps.health_check);
    }

    #[test]
    fn test_plugin_health_check_creation() {
        let health = PluginHealthCheck {
            plugin_name: "test".to_string(),
            healthy: true,
            message: Some("OK".to_string()),
            checked_at: std::time::SystemTime::now(),
            response_time_ms: 42,
        };

        assert_eq!(health.plugin_name, "test");
        assert!(health.healthy);
        assert_eq!(health.response_time_ms, 42);
    }
}
