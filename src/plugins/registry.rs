use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::config::PluginConfig;
use crate::core::{McpError, Prompt, Resource, Tool};

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
    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value>;
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
        Err(McpError::other("Resource subscription not supported"))
    }

    /// Unsubscribe from resource updates (optional)
    async fn unsubscribe_resource(&self, _uri: &str) -> PluginResult<()> {
        Err(McpError::other("Resource subscription not supported"))
    }
}

/// Prompt provider trait
#[async_trait]
pub trait PromptProvider: Plugin {
    /// List available prompts
    async fn list_prompts(&self) -> PluginResult<Vec<Prompt>>;

    /// Get a specific prompt
    async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value>;
}

/// Plugin capabilities enum
#[derive(Debug, Clone, PartialEq)]
pub enum PluginCapability {
    Tools,
    Resources,
    Prompts,
}

/// Unified plugin trait that combines all provider capabilities
#[async_trait]
pub trait UnifiedPlugin: Plugin {
    /// Get plugin capabilities
    fn capabilities(&self) -> Vec<PluginCapability>;

    /// Tool provider methods (if supported)
    async fn list_tools(&self) -> PluginResult<Vec<Tool>> {
        Err(McpError::other("Tool provider not supported"))
    }

    async fn call_tool(
        &self,
        _name: &str,
        _arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value> {
        Err(McpError::other("Tool provider not supported"))
    }

    /// Resource provider methods (if supported)
    async fn list_resources(&self) -> PluginResult<Vec<Resource>> {
        Err(McpError::other("Resource provider not supported"))
    }

    async fn read_resource(&self, _uri: &str) -> PluginResult<Value> {
        Err(McpError::other("Resource provider not supported"))
    }

    async fn subscribe_resource(&self, _uri: &str) -> PluginResult<()> {
        Err(McpError::other("Resource subscription not supported"))
    }

    async fn unsubscribe_resource(&self, _uri: &str) -> PluginResult<()> {
        Err(McpError::other("Resource subscription not supported"))
    }

    /// Prompt provider methods (if supported)
    async fn list_prompts(&self) -> PluginResult<Vec<Prompt>> {
        Err(McpError::other("Prompt provider not supported"))
    }

    async fn get_prompt(
        &self,
        _name: &str,
        _arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value> {
        Err(McpError::other("Prompt provider not supported"))
    }
}

/// Plugin factory trait
pub trait PluginFactory: Send + Sync {
    /// Create a new plugin instance
    fn create(&self) -> Box<dyn UnifiedPlugin>;

    /// Get the plugin name
    fn name(&self) -> &str;

    /// Get the plugin capabilities
    fn capabilities(&self) -> Vec<PluginCapability>;
}

/// Plugin registry
pub struct PluginRegistry {
    factories: HashMap<String, Box<dyn PluginFactory>>,
    instances: HashMap<String, Box<dyn UnifiedPlugin>>,
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
            Err(McpError::other(format!(
                "Plugin factory not found: {}",
                name
            )))
        }
    }

    /// Get a plugin instance
    pub fn get_plugin(&self, name: &str) -> Option<&dyn UnifiedPlugin> {
        self.instances.get(name).map(|p| p.as_ref())
    }

    /// Get a mutable plugin instance
    pub fn get_plugin_mut<F, R>(&mut self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&mut dyn UnifiedPlugin) -> R,
    {
        self.instances.get_mut(name).map(|p| f(p.as_mut()))
    }

    /// Get all plugins with tool capability
    pub fn get_tool_providers(&self) -> Vec<(&str, &dyn UnifiedPlugin)> {
        self.instances
            .iter()
            .filter(|(_, plugin)| plugin.capabilities().contains(&PluginCapability::Tools))
            .map(|(name, plugin)| (name.as_str(), plugin.as_ref()))
            .collect()
    }

    /// Get all plugins with resource capability
    pub fn get_resource_providers(&self) -> Vec<(&str, &dyn UnifiedPlugin)> {
        self.instances
            .iter()
            .filter(|(_, plugin)| plugin.capabilities().contains(&PluginCapability::Resources))
            .map(|(name, plugin)| (name.as_str(), plugin.as_ref()))
            .collect()
    }

    /// Get all plugins with prompt capability
    pub fn get_prompt_providers(&self) -> Vec<(&str, &dyn UnifiedPlugin)> {
        self.instances
            .iter()
            .filter(|(_, plugin)| plugin.capabilities().contains(&PluginCapability::Prompts))
            .map(|(name, plugin)| (name.as_str(), plugin.as_ref()))
            .collect()
    }

    /// Get all tools from all tool providers
    pub async fn list_all_tools(&self) -> PluginResult<Vec<Tool>> {
        let mut all_tools = Vec::new();

        for (name, plugin) in &self.instances {
            if plugin.capabilities().contains(&PluginCapability::Tools) {
                match plugin.list_tools().await {
                    Ok(mut tools) => all_tools.append(&mut tools),
                    Err(e) => {
                        tracing::error!("Failed to list tools from plugin {}: {}", name, e);
                        // Continue with other plugins instead of failing completely
                    }
                }
            }
        }

        Ok(all_tools)
    }

    /// Get all resources from all resource providers
    pub async fn list_all_resources(&self) -> PluginResult<Vec<Resource>> {
        let mut all_resources = Vec::new();

        for (name, plugin) in &self.instances {
            if plugin.capabilities().contains(&PluginCapability::Resources) {
                match plugin.list_resources().await {
                    Ok(mut resources) => all_resources.append(&mut resources),
                    Err(e) => {
                        tracing::error!("Failed to list resources from plugin {}: {}", name, e);
                    }
                }
            }
        }

        Ok(all_resources)
    }

    /// Get all prompts from all prompt providers
    pub async fn list_all_prompts(&self) -> PluginResult<Vec<Prompt>> {
        let mut all_prompts = Vec::new();

        for (name, plugin) in &self.instances {
            if plugin.capabilities().contains(&PluginCapability::Prompts) {
                match plugin.list_prompts().await {
                    Ok(mut prompts) => all_prompts.append(&mut prompts),
                    Err(e) => {
                        tracing::error!("Failed to list prompts from plugin {}: {}", name, e);
                    }
                }
            }
        }

        Ok(all_prompts)
    }

    /// Call a tool by name across all tool providers
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value> {
        for plugin in self.instances.values() {
            if plugin.capabilities().contains(&PluginCapability::Tools) {
                // Check if this plugin has the requested tool
                if let Ok(tools) = plugin.list_tools().await {
                    if tools.iter().any(|tool| tool.name == tool_name) {
                        return plugin.call_tool(tool_name, arguments).await;
                    }
                }
            }
        }

        Err(McpError::tool_not_found(tool_name))
    }

    /// Read a resource by URI across all resource providers
    pub async fn read_resource(&self, uri: &str) -> PluginResult<Value> {
        for plugin in self.instances.values() {
            if plugin.capabilities().contains(&PluginCapability::Resources) {
                // Check if this plugin can handle the URI
                if let Ok(resources) = plugin.list_resources().await {
                    if resources.iter().any(|resource| resource.uri == uri) {
                        return plugin.read_resource(uri).await;
                    }
                }
            }
        }

        Err(McpError::resource_not_found(uri))
    }

    /// Get a prompt by name across all prompt providers
    pub async fn get_prompt(
        &self,
        prompt_name: &str,
        arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value> {
        for plugin in self.instances.values() {
            if plugin.capabilities().contains(&PluginCapability::Prompts) {
                // Check if this plugin has the requested prompt
                if let Ok(prompts) = plugin.list_prompts().await {
                    if prompts.iter().any(|prompt| prompt.name == prompt_name) {
                        return plugin.get_prompt(prompt_name, arguments).await;
                    }
                }
            }
        }

        Err(McpError::other(format!(
            "Prompt not found: {}",
            prompt_name
        )))
    }

    /// List all registered plugin names
    pub fn list_plugins(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }

    /// List all active plugin instances with their capabilities
    pub fn list_active_plugins(&self) -> Vec<(String, Vec<PluginCapability>)> {
        self.instances
            .iter()
            .map(|(name, plugin)| (name.clone(), plugin.capabilities()))
            .collect()
    }

    /// Get plugin capabilities
    pub fn get_plugin_capabilities(&self, name: &str) -> Option<Vec<PluginCapability>> {
        self.instances.get(name).map(|plugin| plugin.capabilities())
    }

    /// Perform health check on all plugins
    pub async fn health_check_all(&self) -> HashMap<String, PluginResult<bool>> {
        let mut results = HashMap::new();

        for (name, plugin) in &self.instances {
            let health = plugin.health_check().await;
            results.insert(name.clone(), health);
        }

        results
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Mock plugin for testing
    struct MockPlugin {
        name: String,
        capabilities: Vec<PluginCapability>,
        initialized: bool,
    }

    impl MockPlugin {
        fn new(name: &str, capabilities: Vec<PluginCapability>) -> Self {
            Self {
                name: name.to_string(),
                capabilities,
                initialized: false,
            }
        }
    }

    #[async_trait]
    impl Plugin for MockPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                name: self.name.clone(),
                version: "1.0.0".to_string(),
                description: "Mock plugin for testing".to_string(),
                author: "Test".to_string(),
                homepage: None,
                dependencies: vec![],
            }
        }

        async fn initialize(&mut self, _config: &PluginConfig) -> PluginResult {
            self.initialized = true;
            Ok(())
        }

        async fn shutdown(&mut self) -> PluginResult {
            self.initialized = false;
            Ok(())
        }

        async fn health_check(&self) -> PluginResult<bool> {
            Ok(self.initialized)
        }
    }

    #[async_trait]
    impl UnifiedPlugin for MockPlugin {
        fn capabilities(&self) -> Vec<PluginCapability> {
            self.capabilities.clone()
        }

        async fn list_tools(&self) -> PluginResult<Vec<Tool>> {
            if self.capabilities.contains(&PluginCapability::Tools) {
                Ok(vec![Tool {
                    name: format!("{}_tool", self.name),
                    description: "Mock tool".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {}
                    }),
                }])
            } else {
                Err(McpError::other("Tool provider not supported"))
            }
        }

        async fn call_tool(
            &self,
            name: &str,
            _arguments: Option<HashMap<String, Value>>,
        ) -> PluginResult<Value> {
            if self.capabilities.contains(&PluginCapability::Tools) {
                Ok(json!({
                    "tool": name,
                    "result": "mock result"
                }))
            } else {
                Err(McpError::other("Tool provider not supported"))
            }
        }

        async fn list_resources(&self) -> PluginResult<Vec<Resource>> {
            if self.capabilities.contains(&PluginCapability::Resources) {
                Ok(vec![Resource {
                    uri: format!("mock://{}/resource", self.name),
                    name: "Mock Resource".to_string(),
                    description: Some("Mock resource for testing".to_string()),
                    mime_type: Some("application/json".to_string()),
                }])
            } else {
                Err(McpError::other("Resource provider not supported"))
            }
        }

        async fn read_resource(&self, uri: &str) -> PluginResult<Value> {
            if self.capabilities.contains(&PluginCapability::Resources) {
                Ok(json!({
                    "uri": uri,
                    "content": "mock content"
                }))
            } else {
                Err(McpError::other("Resource provider not supported"))
            }
        }
    }

    struct MockPluginFactory {
        name: String,
        capabilities: Vec<PluginCapability>,
    }

    impl MockPluginFactory {
        fn new(name: &str, capabilities: Vec<PluginCapability>) -> Self {
            Self {
                name: name.to_string(),
                capabilities,
            }
        }
    }

    impl PluginFactory for MockPluginFactory {
        fn create(&self) -> Box<dyn UnifiedPlugin> {
            Box::new(MockPlugin::new(&self.name, self.capabilities.clone()))
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn capabilities(&self) -> Vec<PluginCapability> {
            self.capabilities.clone()
        }
    }

    #[tokio::test]
    async fn test_plugin_registry_basic_operations() {
        let mut registry = PluginRegistry::new();

        // Register a plugin factory
        let factory = MockPluginFactory::new("test", vec![PluginCapability::Tools]);
        registry.register_factory(factory);

        // Check factory registration
        assert_eq!(registry.list_plugins(), vec!["test"]);

        // Create and initialize plugin
        let config = PluginConfig {
            enabled: true,
            priority: Some(0),
            config: serde_json::json!({}),
        };

        registry.create_plugin("test", &config).await.unwrap();

        // Check active plugins
        let active = registry.list_active_plugins();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].0, "test");
        assert_eq!(active[0].1, vec![PluginCapability::Tools]);

        // Check capabilities
        let capabilities = registry.get_plugin_capabilities("test").unwrap();
        assert_eq!(capabilities, vec![PluginCapability::Tools]);

        // Test health check
        let health = registry.health_check_all().await;
        assert!(health.get("test").unwrap().as_ref().unwrap());
    }

    #[tokio::test]
    async fn test_plugin_registry_tool_operations() {
        let mut registry = PluginRegistry::new();

        let factory = MockPluginFactory::new("tool_test", vec![PluginCapability::Tools]);
        registry.register_factory(factory);

        let config = PluginConfig {
            enabled: true,
            priority: Some(0),
            config: serde_json::json!({}),
        };

        registry.create_plugin("tool_test", &config).await.unwrap();

        // Test list all tools
        let tools = registry.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "tool_test_tool");

        // Test call tool
        let result = registry.call_tool("tool_test_tool", None).await.unwrap();
        assert_eq!(result["tool"], "tool_test_tool");
        assert_eq!(result["result"], "mock result");

        // Test call non-existent tool
        let result = registry.call_tool("nonexistent", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_plugin_registry_resource_operations() {
        let mut registry = PluginRegistry::new();

        let factory = MockPluginFactory::new("resource_test", vec![PluginCapability::Resources]);
        registry.register_factory(factory);

        let config = PluginConfig {
            enabled: true,
            priority: Some(0),
            config: serde_json::json!({}),
        };

        registry
            .create_plugin("resource_test", &config)
            .await
            .unwrap();

        // Test list all resources
        let resources = registry.list_all_resources().await.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "mock://resource_test/resource");

        // Test read resource
        let result = registry
            .read_resource("mock://resource_test/resource")
            .await
            .unwrap();
        assert_eq!(result["uri"], "mock://resource_test/resource");
        assert_eq!(result["content"], "mock content");

        // Test read non-existent resource
        let result = registry.read_resource("nonexistent://resource").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_plugin_registry_mixed_capabilities() {
        let mut registry = PluginRegistry::new();

        // Plugin with multiple capabilities
        let mixed_factory = MockPluginFactory::new(
            "mixed",
            vec![PluginCapability::Tools, PluginCapability::Resources],
        );
        registry.register_factory(mixed_factory);

        // Plugin with only tools
        let tool_factory = MockPluginFactory::new("tool_only", vec![PluginCapability::Tools]);
        registry.register_factory(tool_factory);

        let config = PluginConfig {
            enabled: true,
            priority: Some(0),
            config: serde_json::json!({}),
        };

        registry.create_plugin("mixed", &config).await.unwrap();
        registry.create_plugin("tool_only", &config).await.unwrap();

        // Test tool providers
        let tool_providers = registry.get_tool_providers();
        assert_eq!(tool_providers.len(), 2);

        // Test resource providers
        let resource_providers = registry.get_resource_providers();
        assert_eq!(resource_providers.len(), 1);

        // Test all tools
        let tools = registry.list_all_tools().await.unwrap();
        assert_eq!(tools.len(), 2);

        // Test all resources
        let resources = registry.list_all_resources().await.unwrap();
        assert_eq!(resources.len(), 1);
    }

    #[tokio::test]
    async fn test_plugin_registry_shutdown() {
        let mut registry = PluginRegistry::new();

        let factory = MockPluginFactory::new("shutdown_test", vec![PluginCapability::Tools]);
        registry.register_factory(factory);

        let config = PluginConfig {
            enabled: true,
            priority: Some(0),
            config: serde_json::json!({}),
        };

        registry
            .create_plugin("shutdown_test", &config)
            .await
            .unwrap();

        // Verify plugin is active
        assert_eq!(registry.list_active_plugins().len(), 1);

        // Shutdown all plugins
        registry.shutdown_all().await.unwrap();

        // Verify no active plugins
        assert_eq!(registry.list_active_plugins().len(), 0);
    }
}
