//! Handler Registry
//!
//! Manages handler registration, lifecycle, and plugin loading.

use crate::mcp::{McpError, McpHandler};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// Plugin metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin name (unique identifier)
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    pub description: String,
    /// Plugin author
    pub author: Option<String>,
    /// Whether plugin is enabled
    pub enabled: bool,
    /// Plugin configuration (JSON value)
    pub config: Option<serde_json::Value>,
}

impl PluginInfo {
    pub fn new(name: String, version: String, description: String) -> Self {
        Self {
            name,
            version,
            description,
            author: None,
            enabled: true,
            config: None,
        }
    }
}

/// Handler Registry manages all registered handlers
pub struct HandlerRegistry {
    /// Registered handlers by name
    handlers: HashMap<String, Arc<dyn McpHandler>>,
    /// Plugin metadata
    plugins: HashMap<String, PluginInfo>,
    /// Registry state
    initialized: bool,
}

impl HandlerRegistry {
    /// Create a new empty handler registry
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            plugins: HashMap::new(),
            initialized: false,
        }
    }

    /// Initialize the registry
    pub async fn initialize(&mut self) -> Result<(), McpError> {
        if self.initialized {
            warn!("Handler registry already initialized");
            return Ok(());
        }

        info!(
            "Initializing handler registry with {} handlers",
            self.handlers.len()
        );

        // Initialize all registered handlers
        for name in self.handlers.keys() {
            info!("Initializing handler: {}", name);
            // TODO: Call handler.initialize() when we have InitializeParams
            // For now, just log that we would initialize it
        }

        self.initialized = true;
        info!("Handler registry initialization complete");
        Ok(())
    }

    /// Register a new handler
    pub fn register_handler(
        &mut self,
        name: String,
        handler: Arc<dyn McpHandler>,
        plugin_info: PluginInfo,
    ) -> Result<(), McpError> {
        if self.handlers.contains_key(&name) {
            return Err(McpError::InvalidRequest(format!(
                "Handler '{}' is already registered",
                name
            )));
        }

        info!("Registering handler: {} (v{})", name, plugin_info.version);

        self.handlers.insert(name.clone(), handler);
        self.plugins.insert(name, plugin_info);

        Ok(())
    }

    /// Unregister a handler
    pub fn unregister_handler(&mut self, name: &str) -> Result<(), McpError> {
        if !self.handlers.contains_key(name) {
            return Err(McpError::ToolNotFound(format!(
                "Handler '{}' not found",
                name
            )));
        }

        info!("Unregistering handler: {}", name);

        self.handlers.remove(name);
        self.plugins.remove(name);

        Ok(())
    }

    /// Get a handler by name
    pub fn get_handler(&self, name: &str) -> Option<Arc<dyn McpHandler>> {
        self.handlers.get(name).cloned()
    }

    /// Get all registered handler names
    pub fn list_handlers(&self) -> Vec<String> {
        self.handlers.keys().cloned().collect()
    }

    /// Get plugin info by name
    pub fn get_plugin_info(&self, name: &str) -> Option<&PluginInfo> {
        self.plugins.get(name)
    }

    /// Get all plugin information
    pub fn list_plugins(&self) -> Vec<&PluginInfo> {
        self.plugins.values().collect()
    }

    /// Enable/disable a plugin
    pub fn set_plugin_enabled(&mut self, name: &str, enabled: bool) -> Result<(), McpError> {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.enabled = enabled;
            info!(
                "Plugin '{}' {}",
                name,
                if enabled { "enabled" } else { "disabled" }
            );
            Ok(())
        } else {
            Err(McpError::ToolNotFound(format!(
                "Plugin '{}' not found",
                name
            )))
        }
    }

    /// Check if registry is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get number of registered handlers
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Shutdown all handlers
    pub async fn shutdown_all(&mut self) -> Result<(), McpError> {
        info!("Shutting down {} handlers", self.handlers.len());

        // TODO: Call shutdown methods on handlers when available
        // For now, just clear the registry
        self.handlers.clear();
        self.plugins.clear();
        self.initialized = false;

        info!("All handlers shut down");
        Ok(())
    }

    /// Get handlers that are enabled
    pub fn enabled_handlers(&self) -> Vec<(String, Arc<dyn McpHandler>)> {
        self.handlers
            .iter()
            .filter(|(name, _)| self.plugins.get(*name).map(|p| p.enabled).unwrap_or(false))
            .map(|(name, handler)| (name.clone(), Arc::clone(handler)))
            .collect()
    }

    /// List all tools from all enabled handlers
    pub async fn list_all_tools(&self) -> Result<serde_json::Value, McpError> {
        let mut all_tools = Vec::new();

        for (name, handler) in self.enabled_handlers() {
            match handler.list_tools().await {
                Ok(tools) => {
                    info!("Handler '{}' provided {} tools", name, tools.len());
                    all_tools.extend(tools);
                }
                Err(e) => {
                    warn!("Failed to get tools from handler '{}': {}", name, e);
                    // Continue with other handlers
                }
            }
        }

        Ok(serde_json::json!({
            "tools": all_tools
        }))
    }

    /// Validate handler configuration
    pub fn validate_config(&self, name: &str, config: &serde_json::Value) -> Result<(), McpError> {
        // TODO: Implement configuration validation
        // For now, just check if handler exists
        if !self.handlers.contains_key(name) {
            return Err(McpError::ToolNotFound(format!(
                "Handler '{}' not found",
                name
            )));
        }

        // Basic JSON validation
        if !config.is_object() {
            return Err(McpError::InvalidParams(
                "Configuration must be a JSON object".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{InitializeParams, Resource, ResourceReadParams, Tool, ToolCallParams};
    use async_trait::async_trait;

    // Mock handler for testing
    struct MockHandler;

    #[async_trait]
    impl McpHandler for MockHandler {
        async fn initialize(
            &self,
            _params: InitializeParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({"status": "initialized"}))
        }

        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(vec![])
        }

        async fn call_tool(&self, _params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({"result": "success"}))
        }

        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(vec![])
        }

        async fn read_resource(
            &self,
            _params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({"content": "test"}))
        }
    }

    #[tokio::test]
    async fn test_registry_lifecycle() {
        let mut registry = HandlerRegistry::new();
        assert_eq!(registry.handler_count(), 0);
        assert!(!registry.is_initialized());

        // Register a handler
        let handler = Arc::new(MockHandler);
        let plugin_info = PluginInfo::new(
            "test".to_string(),
            "1.0.0".to_string(),
            "Test handler".to_string(),
        );

        registry
            .register_handler("test".to_string(), handler, plugin_info)
            .unwrap();
        assert_eq!(registry.handler_count(), 1);

        // Initialize
        registry.initialize().await.unwrap();
        assert!(registry.is_initialized());

        // Get handler
        assert!(registry.get_handler("test").is_some());
        assert!(registry.get_handler("nonexistent").is_none());

        // Shutdown
        registry.shutdown_all().await.unwrap();
        assert_eq!(registry.handler_count(), 0);
        assert!(!registry.is_initialized());
    }

    #[test]
    fn test_plugin_management() {
        let mut registry = HandlerRegistry::new();
        let handler = Arc::new(MockHandler);
        let plugin_info = PluginInfo::new(
            "test".to_string(),
            "1.0.0".to_string(),
            "Test handler".to_string(),
        );

        registry
            .register_handler("test".to_string(), handler, plugin_info)
            .unwrap();

        // Check plugin info
        let info = registry.get_plugin_info("test").unwrap();
        assert_eq!(info.name, "test");
        assert_eq!(info.version, "1.0.0");
        assert!(info.enabled);

        // Disable plugin
        registry.set_plugin_enabled("test", false).unwrap();
        let info = registry.get_plugin_info("test").unwrap();
        assert!(!info.enabled);

        // Enable plugin
        registry.set_plugin_enabled("test", true).unwrap();
        let info = registry.get_plugin_info("test").unwrap();
        assert!(info.enabled);
    }
}
