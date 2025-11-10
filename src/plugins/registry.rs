//! Dynamic Plugin Registry
//!
//! Manages plugin discovery, loading, and integration with the main handler registry.

use super::{
    discover_plugins, PluginError, PluginLoader, PluginManifest, PluginMetadata, PluginStatus,
};
use crate::core::registry::{HandlerRegistry, PluginInfo};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Configuration for plugin loading
pub type PluginConfig = crate::config::PluginConfig;

/// Dynamic plugin registry that integrates with the core handler registry
pub struct DynamicPluginRegistry {
    /// Plugin loader instance
    loader: PluginLoader,
    /// Plugin configuration
    config: PluginConfig,
    /// Discovered plugin manifests
    manifests: HashMap<String, PluginManifest>,
    /// Plugin load order for dependency resolution
    load_order: Vec<String>,
    /// Core handler registry integration
    handler_registry: Arc<RwLock<HandlerRegistry>>,
}

impl DynamicPluginRegistry {
    /// Create a new dynamic plugin registry
    pub fn new(config: PluginConfig, handler_registry: Arc<RwLock<HandlerRegistry>>) -> Self {
        let mut loader = PluginLoader::new();

        // Add search paths to loader
        for path in &config.search_paths {
            loader.add_search_path(path);
        }

        Self {
            loader,
            config,
            manifests: HashMap::new(),
            load_order: Vec::new(),
            handler_registry,
        }
    }

    /// Initialize the plugin registry
    pub async fn initialize(&mut self) -> Result<(), PluginError> {
        info!("Initializing dynamic plugin registry");

        // Discover plugins in search paths
        self.discover_all_plugins().await?;

        // Auto-load plugins if configured
        if self.config.auto_load {
            self.load_all_plugins().await?;
        }

        info!(
            "Dynamic plugin registry initialized with {} plugins discovered",
            self.manifests.len()
        );
        Ok(())
    }

    /// Discover all plugins in search paths
    pub async fn discover_all_plugins(&mut self) -> Result<(), PluginError> {
        info!("Discovering plugins in search paths");
        let mut total_discovered = 0;

        for search_path in &self.config.search_paths {
            if search_path.exists() {
                debug!("Scanning plugin directory: {:?}", search_path);
                match discover_plugins(search_path).await {
                    Ok(manifests) => {
                        let count = manifests.len();
                        total_discovered += count;
                        info!("Found {} plugins in {:?}", count, search_path);

                        for manifest in manifests {
                            self.manifests
                                .insert(manifest.metadata.name.clone(), manifest);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to discover plugins in {:?}: {}", search_path, e);
                    }
                }
            } else {
                debug!("Plugin search path does not exist: {:?}", search_path);
            }
        }

        // Resolve load order based on dependencies
        self.resolve_load_order()?;

        info!(
            "Plugin discovery complete: {} plugins found",
            total_discovered
        );
        Ok(())
    }

    /// Load all discovered plugins
    pub async fn load_all_plugins(&mut self) -> Result<(), PluginError> {
        info!("Loading all discovered plugins");
        let mut loaded_count = 0;
        let mut failed_count = 0;

        // Check plugin limit
        if let Some(max_plugins) = self.config.max_plugins {
            if self.manifests.len() > max_plugins {
                warn!(
                    "Plugin limit exceeded: {} > {}, loading first {} plugins",
                    self.manifests.len(),
                    max_plugins,
                    max_plugins
                );
            }
        }

        // Load plugins in dependency order
        for plugin_name in &self.load_order.clone() {
            if let Some(max_plugins) = self.config.max_plugins {
                if loaded_count >= max_plugins {
                    warn!(
                        "Reached plugin limit ({}), stopping plugin loading",
                        max_plugins
                    );
                    break;
                }
            }

            match self.load_plugin(plugin_name).await {
                Ok(()) => {
                    loaded_count += 1;
                    info!("Successfully loaded plugin '{}'", plugin_name);
                }
                Err(e) => {
                    failed_count += 1;
                    error!("Failed to load plugin '{}': {}", plugin_name, e);
                }
            }
        }

        info!(
            "Plugin loading complete: {} loaded, {} failed",
            loaded_count, failed_count
        );

        if failed_count > 0 {
            warn!("{} plugins failed to load", failed_count);
        }

        Ok(())
    }

    /// Load a specific plugin by name
    pub async fn load_plugin(&mut self, plugin_name: &str) -> Result<(), PluginError> {
        let _manifest = self
            .manifests
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?
            .clone();

        // Find plugin directory
        let plugin_dir = self.find_plugin_directory(plugin_name)?;

        // Load the plugin
        self.loader.load_plugin_from_directory(&plugin_dir).await?;

        // Initialize with configuration
        let config = self.config.plugins.get(plugin_name).cloned();
        self.loader.initialize_plugin(plugin_name, config)?;

        // Integrate with handler registry
        self.integrate_plugin_handlers(plugin_name).await?;

        Ok(())
    }

    /// Unload a specific plugin by name
    pub async fn unload_plugin(&mut self, plugin_name: &str) -> Result<(), PluginError> {
        info!("Unloading plugin '{}'", plugin_name);

        // Remove handlers from registry
        self.remove_plugin_handlers(plugin_name).await?;

        // Unload from plugin loader
        self.loader.unload_plugin(plugin_name)?;

        info!("Plugin '{}' unloaded successfully", plugin_name);
        Ok(())
    }

    /// Get plugin status
    pub fn get_plugin_status(&self, plugin_name: &str) -> PluginStatus {
        self.loader.get_plugin_status(plugin_name)
    }

    /// List all discovered plugins
    pub fn list_discovered_plugins(&self) -> Vec<&PluginMetadata> {
        self.manifests.values().map(|m| &m.metadata).collect()
    }

    /// List all loaded plugins
    pub fn list_loaded_plugins(&self) -> Vec<&str> {
        self.loader.list_loaded_plugins()
    }

    /// Enable/disable a plugin
    pub async fn set_plugin_enabled(
        &mut self,
        plugin_name: &str,
        enabled: bool,
    ) -> Result<(), PluginError> {
        if enabled {
            if self.get_plugin_status(plugin_name) == PluginStatus::NotLoaded {
                self.load_plugin(plugin_name).await?;
            }
        } else if self.get_plugin_status(plugin_name) != PluginStatus::NotLoaded {
            self.unload_plugin(plugin_name).await?;
        }

        // Update handler registry
        let mut registry = self.handler_registry.write().await;
        if let Err(e) = registry.set_plugin_enabled(plugin_name, enabled) {
            warn!(
                "Failed to update plugin enabled status in handler registry: {}",
                e
            );
        }

        Ok(())
    }

    /// Get plugin configuration
    pub fn get_plugin_config(&self, plugin_name: &str) -> Option<&serde_json::Value> {
        self.config.plugins.get(plugin_name)
    }

    /// Update plugin configuration
    pub fn update_plugin_config(
        &mut self,
        plugin_name: &str,
        config: serde_json::Value,
    ) -> Result<(), PluginError> {
        // Validate configuration if plugin is loaded
        if let Some(plugin) = self.loader.get_plugin(plugin_name) {
            plugin
                .validate_config(&config)
                .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;
        }

        self.config.plugins.insert(plugin_name.to_string(), config);
        Ok(())
    }

    /// Reload a plugin (unload and load again)
    pub async fn reload_plugin(&mut self, plugin_name: &str) -> Result<(), PluginError> {
        info!("Reloading plugin '{}'", plugin_name);

        if self.get_plugin_status(plugin_name) != PluginStatus::NotLoaded {
            self.unload_plugin(plugin_name).await?;
        }

        self.load_plugin(plugin_name).await?;

        info!("Plugin '{}' reloaded successfully", plugin_name);
        Ok(())
    }

    /// Shutdown all plugins
    pub async fn shutdown(&mut self) -> Result<(), PluginError> {
        info!("Shutting down dynamic plugin registry");

        // Unload all plugins
        let loaded_plugins: Vec<String> = self
            .loader
            .list_loaded_plugins()
            .iter()
            .map(|s| s.to_string())
            .collect();
        for plugin_name in loaded_plugins {
            if let Err(e) = self.unload_plugin(&plugin_name).await {
                error!("Error unloading plugin '{}': {}", plugin_name, e);
            }
        }

        // Shutdown plugin loader
        self.loader.shutdown_all()?;

        info!("Dynamic plugin registry shutdown complete");
        Ok(())
    }

    /// Integrate plugin handlers with the main handler registry
    async fn integrate_plugin_handlers(&mut self, plugin_name: &str) -> Result<(), PluginError> {
        let plugin = self
            .loader
            .get_plugin(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;

        let handlers = plugin
            .create_handlers()
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        let mut registry = self.handler_registry.write().await;

        for (handler_name, handler) in handlers {
            let plugin_info = PluginInfo {
                name: format!("{}::{}", plugin_name, handler_name),
                version: plugin.metadata().version.clone(),
                description: format!("{} (from {})", handler_name, plugin.metadata().description),
                author: plugin.metadata().author.clone(),
                enabled: true,
                config: self.config.plugins.get(plugin_name).cloned(),
            };

            registry
                .register_handler(handler_name.clone(), handler, plugin_info)
                .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

            info!(
                "Registered handler '{}' from plugin '{}'",
                handler_name, plugin_name
            );
        }

        Ok(())
    }

    /// Remove plugin handlers from the main handler registry
    async fn remove_plugin_handlers(&mut self, plugin_name: &str) -> Result<(), PluginError> {
        let mut registry = self.handler_registry.write().await;

        // Get all handlers that belong to this plugin
        let plugin_handlers: Vec<String> = registry
            .list_handlers()
            .into_iter()
            .filter(|h| {
                registry
                    .get_plugin_info(h)
                    .map(|info| info.name.starts_with(&format!("{}::", plugin_name)))
                    .unwrap_or(false)
            })
            .collect();

        // Unregister all plugin handlers
        for handler_name in plugin_handlers {
            if let Err(e) = registry.unregister_handler(&handler_name) {
                warn!("Failed to unregister handler '{}': {}", handler_name, e);
            } else {
                info!(
                    "Unregistered handler '{}' from plugin '{}'",
                    handler_name, plugin_name
                );
            }
        }

        Ok(())
    }

    /// Find plugin directory by name
    fn find_plugin_directory(&self, plugin_name: &str) -> Result<PathBuf, PluginError> {
        for search_path in &self.config.search_paths {
            let plugin_dir = search_path.join(plugin_name);
            if plugin_dir.exists() && plugin_dir.is_dir() {
                return Ok(plugin_dir);
            }
        }

        Err(PluginError::NotFound(format!(
            "Plugin directory not found for '{}'",
            plugin_name
        )))
    }

    /// Resolve plugin load order based on dependencies
    fn resolve_load_order(&mut self) -> Result<(), PluginError> {
        let mut order = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut visiting = std::collections::HashSet::new();

        for plugin_name in self.manifests.keys() {
            if !visited.contains(plugin_name) {
                self.visit_plugin_for_order(plugin_name, &mut order, &mut visited, &mut visiting)?;
            }
        }

        self.load_order = order;
        debug!("Plugin load order resolved: {:?}", self.load_order);
        Ok(())
    }

    /// Depth-first search for dependency resolution
    fn visit_plugin_for_order(
        &self,
        plugin_name: &str,
        order: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
        visiting: &mut std::collections::HashSet<String>,
    ) -> Result<(), PluginError> {
        if visiting.contains(plugin_name) {
            return Err(PluginError::DependencyMissing(format!(
                "Circular dependency detected involving '{}'",
                plugin_name
            )));
        }

        if visited.contains(plugin_name) {
            return Ok(());
        }

        visiting.insert(plugin_name.to_string());

        // Visit dependencies first
        if let Some(manifest) = self.manifests.get(plugin_name) {
            if let Some(dependencies) = &manifest.dependencies {
                for dep in dependencies {
                    if !dep.optional.unwrap_or(false) {
                        if !self.manifests.contains_key(&dep.name) {
                            return Err(PluginError::DependencyMissing(format!(
                                "Required dependency '{}' not found for plugin '{}'",
                                dep.name, plugin_name
                            )));
                        }

                        self.visit_plugin_for_order(&dep.name, order, visited, visiting)?;
                    }
                }
            }
        }

        visiting.remove(plugin_name);
        visited.insert(plugin_name.to_string());
        order.push(plugin_name.to_string());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::registry::HandlerRegistry;

    #[tokio::test]
    async fn test_plugin_registry_creation() {
        let config = PluginConfig::default();
        let handler_registry = Arc::new(RwLock::new(HandlerRegistry::new()));
        let registry = DynamicPluginRegistry::new(config, handler_registry);

        assert_eq!(registry.list_loaded_plugins().len(), 0);
        assert_eq!(registry.list_discovered_plugins().len(), 0);
    }

    #[test]
    fn test_plugin_config_defaults() {
        let config = PluginConfig::default();
        assert!(!config.search_paths.is_empty());
        assert!(config.auto_load);
        assert!(!config.hot_reload);
        assert!(config.max_plugins.is_some());
    }
}
