use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::core::{McpError, Tool, Resource, Prompt};
use crate::config::PluginConfig;

/// Plugin initialization result
pub type PluginResult<T = ()> = Result<T, McpError>;

/// Plugin metadata
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Plugin name
    pub name: String,
    
    /// Plugin version
    pub version: String,
    
    /// Plugin description
    pub description: String,
    
    /// Plugin author
    pub author: String,
    
    /// Plugin homepage
    pub homepage: Option<String>,
    
    /// Plugin dependencies
    pub dependencies: Vec<String>,
}

/// Plugin lifecycle trait
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Get plugin metadata
    fn metadata(&self) -> PluginMetadata;
    
    /// Initialize the plugin with configuration
    async fn initialize(&mut self, config: &PluginConfig) -> PluginResult;
    
    /// Shutdown the plugin
    async fn shutdown(&mut self) -> PluginResult;
    
    /// Check if plugin is healthy
    async fn health_check(&self) -> PluginResult<bool>;
}

/// Tool provider trait
#[async_trait]
pub trait ToolProvider: Plugin {
    /// List available tools
    async fn list_tools(&self) -> PluginResult<Vec<Tool>>;
    
    /// Call a specific tool
    async fn call_tool(&self, name: &str, arguments: Option<HashMap<String, Value>>) -> PluginResult<Value>;
}

/// Resource provider trait
#[async_trait]
pub trait ResourceProvider: Plugin {
    /// List available resources
    async fn list_resources(&self) -> PluginResult<Vec<Resource>>;
    
    /// Read a specific resource
    async fn read_resource(&self, uri: &str) -> PluginResult<Value>;
    
    /// Subscribe to resource updates (optional)
    async fn subscribe_resource(&self, _uri: &str) -> PluginResult<()> {
        Err(McpError::Other("Resource subscription not supported".to_string()))
    }
    
    /// Unsubscribe from resource updates (optional)
    async fn unsubscribe_resource(&self, _uri: &str) -> PluginResult<()> {
        Err(McpError::Other("Resource subscription not supported".to_string()))
    }
}

/// Prompt provider trait
#[async_trait]
pub trait PromptProvider: Plugin {
    /// List available prompts
    async fn list_prompts(&self) -> PluginResult<Vec<Prompt>>;
    
    /// Get a specific prompt
    async fn get_prompt(&self, name: &str, arguments: Option<HashMap<String, Value>>) -> PluginResult<Value>;
}

/// Plugin factory trait
pub trait PluginFactory: Send + Sync {
    /// Create a new plugin instance
    fn create(&self) -> Box<dyn Plugin>;
    
    /// Get the plugin name
    fn name(&self) -> &str;
}

/// Plugin registry
pub struct PluginRegistry {
    factories: HashMap<String, Box<dyn PluginFactory>>,
    instances: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
            instances: HashMap::new(),
        }
    }
    
    /// Register a plugin factory
    pub fn register_factory<F>(&mut self, factory: F) 
    where
        F: PluginFactory + 'static,
    {
        let name = factory.name().to_string();
        self.factories.insert(name, Box::new(factory));
    }
    
    /// Create and initialize a plugin
    pub async fn create_plugin(&mut self, name: &str, config: &PluginConfig) -> PluginResult<()> {
        if let Some(factory) = self.factories.get(name) {
            let mut plugin = factory.create();
            plugin.initialize(config).await?;
            self.instances.insert(name.to_string(), plugin);
            Ok(())
        } else {
            Err(McpError::Other(format!("Plugin factory not found: {}", name)))
        }
    }
    
    /// Get a plugin instance
    pub fn get_plugin(&self, name: &str) -> Option<&dyn Plugin> {
        self.instances.get(name).map(|p| p.as_ref())
    }
    
    /// Get a tool provider
    pub fn get_tool_provider(&self, name: &str) -> Option<&dyn ToolProvider> {
        self.instances.get(name)?.as_ref().downcast_ref()
    }
    
    /// Get a resource provider
    pub fn get_resource_provider(&self, name: &str) -> Option<&dyn ResourceProvider> {
        self.instances.get(name)?.as_ref().downcast_ref()
    }
    
    /// Get a prompt provider
    pub fn get_prompt_provider(&self, name: &str) -> Option<&dyn PromptProvider> {
        self.instances.get(name)?.as_ref().downcast_ref()
    }
    
    /// List all registered plugin names
    pub fn list_plugins(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
    
    /// List all active plugin instances
    pub fn list_active_plugins(&self) -> Vec<String> {
        self.instances.keys().cloned().collect()
    }
    
    /// Shutdown all plugins
    pub async fn shutdown_all(&mut self) -> PluginResult<()> {
        for (name, plugin) in &mut self.instances {
            if let Err(e) = plugin.shutdown().await {
                tracing::error!("Failed to shutdown plugin {}: {}", name, e);
            }
        }
        self.instances.clear();
        Ok(())
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}