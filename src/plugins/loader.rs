//! Dynamic Plugin Loader
//!
//! Provides functionality to load plugins at runtime using libloading.
//!
//! # Examples
//!
//! ## Basic Plugin Loading
//!
//! ```rust
//! use mcp_rs::plugins::PluginLoader;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut loader = PluginLoader::new();
//!
//! // Add search paths for plugins
//! loader.add_search_path("./plugins");
//! loader.add_search_path("/usr/local/lib/mcp-rs/plugins");
//!
//! // Attempt to find a plugin library
//! if let Some(lib_path) = loader.find_plugin_library("example_plugin") {
//!     println!("Found plugin library at: {:?}", lib_path);
//!     
//!     // Note: Actual loading is commented out for safety in this example
//!     // In production, you would use load_plugin() method
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Plugin Search Path Management
//!
//! ```rust
//! use mcp_rs::plugins::PluginLoader;
//!
//! let mut loader = PluginLoader::new();
//!
//! // Add multiple search paths
//! let paths = vec![
//!     "./local_plugins",
//!     "/opt/mcp-rs/plugins",
//!     "/home/user/.mcp-rs/plugins"
//! ];
//!
//! for path in paths {
//!     loader.add_search_path(path);
//! }
//!
//! // Search for a specific plugin
//! if let Some(plugin_path) = loader.find_plugin_library("my_plugin") {
//!     println!("Found plugin at: {:?}", plugin_path);
//! }
//! ```

use super::{
    check_plugin_abi_compatibility, DynamicPlugin, PluginError, PluginManifest, PluginStatus,
};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

/// Type definition for plugin factory function  
/// Returns a raw pointer to avoid FFI safety issues
type PluginFactoryFn = unsafe extern "C" fn() -> *mut std::ffi::c_void;

/// Type definition for plugin API version function
type PluginApiVersionFn = unsafe extern "C" fn() -> *const std::os::raw::c_char;

/// Dynamic plugin loader
pub struct PluginLoader {
    /// Loaded libraries
    libraries: HashMap<String, Library>,
    /// Plugin search paths
    search_paths: Vec<PathBuf>,
    /// Loaded plugin instances
    plugins: HashMap<String, Box<dyn DynamicPlugin>>,
}

impl PluginLoader {
    /// Create a new plugin loader
    pub fn new() -> Self {
        Self {
            libraries: HashMap::new(),
            search_paths: Vec::new(),
            plugins: HashMap::new(),
        }
    }

    /// Add a search path for plugins
    pub fn add_search_path<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref().to_path_buf();
        if !self.search_paths.contains(&path) {
            info!("Adding plugin search path: {:?}", path);
            self.search_paths.push(path);
        }
    }

    /// Load a plugin from a dynamic library file
    pub async fn load_plugin<P: AsRef<Path>>(
        &mut self,
        manifest: &PluginManifest,
        library_path: P,
    ) -> Result<(), PluginError> {
        let library_path = library_path.as_ref();
        let plugin_name = &manifest.metadata.name;

        info!(
            "Loading plugin '{}' v{} from {:?}",
            plugin_name, manifest.metadata.version, library_path
        );

        // Check if plugin is already loaded
        if self.plugins.contains_key(plugin_name) {
            warn!("Plugin '{}' is already loaded", plugin_name);
            return Err(PluginError::LoadingFailed(format!(
                "Plugin '{}' is already loaded",
                plugin_name
            )));
        }

        // Load the dynamic library
        let library = unsafe { Library::new(library_path)? };

        // Check API version compatibility
        let api_version = self.get_plugin_api_version(&library)?;
        check_plugin_abi_compatibility(&api_version)?;

        // Get the plugin factory function
        let factory: Symbol<PluginFactoryFn> = unsafe { library.get(b"mcp_plugin_create\0")? };

        // Create plugin instance
        let plugin_ptr = unsafe { factory() };
        if plugin_ptr.is_null() {
            return Err(PluginError::LoadingFailed(
                "Plugin factory returned null pointer".to_string(),
            ));
        }

        // Note: In a real implementation, this would require the plugin
        // to provide a way to safely cast back to the trait object.
        // For now, we'll return an error as this operation is inherently unsafe
        // without proper plugin metadata.
        Err(PluginError::LoadingFailed(
            "Dynamic plugin loading requires additional safety mechanisms".to_string(),
        ))

        // The following code would be used in a complete implementation:
        // 1. Store the library for later cleanup
        // 2. Store plugin metadata for management
        // Note: Actual plugin instance creation is commented out due to safety concerns

        // self.libraries.insert(plugin_name.clone(), library);
        // info!("Successfully loaded plugin '{}'", plugin_name);
        // Ok(())
    }

    /// Load a plugin from directory (searches for manifest and library)
    pub async fn load_plugin_from_directory<P: AsRef<Path>>(
        &mut self,
        plugin_dir: P,
    ) -> Result<(), PluginError> {
        let plugin_dir = plugin_dir.as_ref();
        debug!("Loading plugin from directory: {:?}", plugin_dir);

        // Read plugin manifest
        let manifest_path = plugin_dir.join("plugin.toml");
        if !manifest_path.exists() {
            return Err(PluginError::NotFound(format!(
                "Plugin manifest not found: {:?}",
                manifest_path
            )));
        }

        let manifest_content = tokio::fs::read_to_string(&manifest_path).await?;
        let manifest: PluginManifest = toml::from_str(&manifest_content)
            .map_err(|e| PluginError::InvalidManifest(e.to_string()))?;

        // Find the main entry point library
        let main_entry = manifest
            .entry_points
            .get("main")
            .ok_or_else(|| PluginError::InvalidManifest("No main entry point".to_string()))?;

        let library_path = plugin_dir.join(main_entry);
        if !library_path.exists() {
            return Err(PluginError::NotFound(format!(
                "Plugin library not found: {:?}",
                library_path
            )));
        }

        self.load_plugin(&manifest, &library_path).await
    }

    /// Initialize a loaded plugin with configuration
    pub fn initialize_plugin(
        &mut self,
        plugin_name: &str,
        config: Option<serde_json::Value>,
    ) -> Result<(), PluginError> {
        let plugin = self
            .plugins
            .get_mut(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.to_string()))?;

        plugin
            .initialize(config)
            .map_err(|e| PluginError::InitializationFailed(e.to_string()))?;

        info!("Plugin '{}' initialized successfully", plugin_name);
        Ok(())
    }

    /// Unload a plugin
    pub fn unload_plugin(&mut self, plugin_name: &str) -> Result<(), PluginError> {
        info!("Unloading plugin '{}'", plugin_name);

        // Shutdown the plugin
        if let Some(mut plugin) = self.plugins.remove(plugin_name) {
            if let Err(e) = plugin.shutdown() {
                warn!("Error shutting down plugin '{}': {}", plugin_name, e);
            }
        }

        // Remove the library
        if let Some(_library) = self.libraries.remove(plugin_name) {
            info!("Plugin library '{}' unloaded", plugin_name);
        }

        Ok(())
    }

    /// Get a reference to a loaded plugin
    pub fn get_plugin(&self, plugin_name: &str) -> Option<&dyn DynamicPlugin> {
        self.plugins.get(plugin_name).map(|p| p.as_ref())
    }

    /// Get a mutable reference to a loaded plugin by taking ownership of the string
    pub fn get_plugin_mut(&mut self, plugin_name: &str) -> Option<&mut Box<dyn DynamicPlugin>> {
        self.plugins.get_mut(plugin_name)
    }

    /// List all loaded plugins
    pub fn list_loaded_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Get plugin status
    pub fn get_plugin_status(&self, plugin_name: &str) -> PluginStatus {
        self.plugins
            .get(plugin_name)
            .map(|p| p.status())
            .unwrap_or(PluginStatus::NotLoaded)
    }

    /// Find plugin library in search paths
    pub fn find_plugin_library(&self, library_name: &str) -> Option<PathBuf> {
        let library_name = if cfg!(windows) {
            if !library_name.ends_with(".dll") {
                format!("{}.dll", library_name)
            } else {
                library_name.to_string()
            }
        } else if cfg!(target_os = "macos") {
            if !library_name.ends_with(".dylib") {
                format!("lib{}.dylib", library_name)
            } else {
                library_name.to_string()
            }
        } else if !library_name.ends_with(".so") {
            format!("lib{}.so", library_name)
        } else {
            library_name.to_string()
        };

        for search_path in &self.search_paths {
            let full_path = search_path.join(&library_name);
            if full_path.exists() {
                return Some(full_path);
            }
        }

        None
    }

    /// Reload a plugin (unload and load again)
    pub async fn reload_plugin(&mut self, plugin_name: &str) -> Result<(), PluginError> {
        info!("Reloading plugin '{}'", plugin_name);

        // Store the plugin directory for reloading
        // In a real implementation, we'd need to track the original load path
        // For now, we'll return an error
        Err(PluginError::LoadingFailed(
            "Plugin reloading not yet implemented".to_string(),
        ))
    }

    /// Get plugin API version from library
    fn get_plugin_api_version(&self, library: &Library) -> Result<String, PluginError> {
        let version_fn: Symbol<PluginApiVersionFn> =
            unsafe { library.get(b"mcp_plugin_api_version\0")? };

        let version_ptr = unsafe { version_fn() };
        if version_ptr.is_null() {
            return Err(PluginError::AbiIncompatible(
                "Plugin API version function returned null".to_string(),
            ));
        }

        let c_str = unsafe { std::ffi::CStr::from_ptr(version_ptr) };
        let version = c_str
            .to_str()
            .map_err(|e| PluginError::AbiIncompatible(e.to_string()))?;

        Ok(version.to_string())
    }

    /// Shutdown all plugins
    pub fn shutdown_all(&mut self) -> Result<(), PluginError> {
        info!("Shutting down all plugins");

        let plugin_names: Vec<String> = self.plugins.keys().cloned().collect();
        for plugin_name in plugin_names {
            if let Err(e) = self.unload_plugin(&plugin_name) {
                error!("Error unloading plugin '{}': {}", plugin_name, e);
            }
        }

        Ok(())
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PluginLoader {
    fn drop(&mut self) {
        if !self.plugins.is_empty() {
            warn!(
                "PluginLoader dropped with {} plugins still loaded",
                self.plugins.len()
            );
            let _ = self.shutdown_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_loader_creation() {
        let loader = PluginLoader::new();
        assert!(loader.list_loaded_plugins().is_empty());
    }

    #[test]
    fn test_search_path_management() {
        let mut loader = PluginLoader::new();
        loader.add_search_path("/path/to/plugins");
        loader.add_search_path("/another/path");

        assert_eq!(loader.search_paths.len(), 2);
        assert!(loader
            .search_paths
            .contains(&PathBuf::from("/path/to/plugins")));
        assert!(loader
            .search_paths
            .contains(&PathBuf::from("/another/path")));
    }

    #[test]
    fn test_library_name_resolution() {
        let loader = PluginLoader::new();

        if cfg!(windows) {
            let result = loader.find_plugin_library("test");
            assert!(result.is_none()); // No search paths added
        }
    }
}
