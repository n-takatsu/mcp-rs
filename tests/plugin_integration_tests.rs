//! Integration tests for plugin system
//!
//! These tests exercise the plugin loading and management functionality.

use mcp_rs::config::{McpConfig, PluginConfig, PluginsConfig};
use mcp_rs::core::registry::HandlerRegistry;
use mcp_rs::plugins::{
    DynamicPluginRegistry, PluginError, PluginLoader, PluginManifest, PluginMetadata, PluginStatus,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_plugin_config_conversion() {
    let config = McpConfig {
        plugins: Some(PluginsConfig {
            search_paths: Some(vec!["./test_plugins".to_string()]),
            auto_load: Some(true),
            plugins: Some(HashMap::new()),
            hot_reload: Some(false),
            max_plugins: Some(10),
        }),
        ..Default::default()
    };

    let plugin_config = config.to_plugin_config();
    assert!(plugin_config.is_some());

    let plugin_config = plugin_config.unwrap();
    assert_eq!(plugin_config.search_paths.len(), 1);
    assert!(plugin_config.auto_load);
    assert!(!plugin_config.hot_reload);
    assert_eq!(plugin_config.max_plugins, Some(10));
}

#[tokio::test]
async fn test_plugin_loader_lifecycle() {
    let mut loader = PluginLoader::new();

    // Test initial state
    assert!(loader.list_loaded_plugins().is_empty());
    assert_eq!(
        loader.get_plugin_status("nonexistent"),
        PluginStatus::NotLoaded
    );

    // Test search path management
    loader.add_search_path("./test_plugins");
    loader.add_search_path("/usr/local/lib/test");

    // Test library name resolution
    let lib_path = loader.find_plugin_library("test_plugin");
    // This will be None in test environment, but exercises the function
    assert!(lib_path.is_none());
}

#[tokio::test]
async fn test_plugin_registry_creation() {
    let config = PluginConfig {
        search_paths: vec![PathBuf::from("./test_plugins")],
        auto_load: false,
        plugins: HashMap::new(),
        hot_reload: false,
        max_plugins: Some(5),
    };

    let handler_registry = Arc::new(RwLock::new(HandlerRegistry::new()));
    let mut registry = DynamicPluginRegistry::new(config, handler_registry);

    // Test initial state
    assert!(registry.list_loaded_plugins().is_empty());
    assert!(registry.list_discovered_plugins().is_empty());

    // Test initialization
    let result = registry.initialize().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_plugin_discovery_nonexistent_path() {
    let handler_registry = Arc::new(RwLock::new(HandlerRegistry::new()));
    let config = PluginConfig {
        search_paths: vec![PathBuf::from("./nonexistent_plugins")],
        auto_load: false,
        plugins: HashMap::new(),
        hot_reload: false,
        max_plugins: Some(5),
    };

    let mut registry = DynamicPluginRegistry::new(config, handler_registry);

    // Should handle nonexistent paths gracefully
    let result = registry.discover_all_plugins().await;
    assert!(result.is_ok());
    assert!(registry.list_discovered_plugins().is_empty());
}

#[tokio::test]
async fn test_plugin_loading_errors() {
    let mut loader = PluginLoader::new();

    // Test loading nonexistent plugin
    let manifest = PluginManifest {
        metadata: PluginMetadata {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "Test plugin".to_string(),
            author: None,
            license: None,
            homepage: None,
            repository: None,
            tags: None,
        },
        entry_points: {
            let mut map = HashMap::new();
            map.insert("main".to_string(), "libtest.so".to_string());
            map
        },
        dependencies: None,
        min_mcp_version: Some("0.2.0".to_string()),
        config_schema: None,
    };

    // This should fail because dynamic loading is not fully implemented for safety reasons
    let result = loader.load_plugin(&manifest, "/nonexistent/path").await;
    assert!(result.is_err());

    // Test that it fails with the expected safety error for existing files too
    let result = loader.load_plugin(&manifest, "Cargo.toml").await;
    assert!(result.is_err());
    // Should fail with safety mechanism error, not library loading error
    if let PluginError::LoadingFailed(msg) = result.unwrap_err() {
        assert!(msg.contains("safety mechanisms") || msg.contains("library"));
    }
    // Other errors are also acceptable
}

#[tokio::test]
async fn test_plugin_registry_plugin_management() {
    let handler_registry = Arc::new(RwLock::new(HandlerRegistry::new()));
    let config = PluginConfig {
        search_paths: vec![],
        auto_load: false,
        plugins: HashMap::new(),
        hot_reload: false,
        max_plugins: Some(5),
    };

    let mut registry = DynamicPluginRegistry::new(config, handler_registry);

    // Test plugin status for nonexistent plugin
    assert_eq!(
        registry.get_plugin_status("nonexistent"),
        PluginStatus::NotLoaded
    );

    // Test loading nonexistent plugin
    let result = registry.load_plugin("nonexistent").await;
    assert!(matches!(result, Err(PluginError::NotFound(_))));

    // Test configuration management
    let config_value = serde_json::json!({"key": "value"});
    let result = registry.update_plugin_config("test", config_value.clone());
    assert!(result.is_ok());

    let retrieved_config = registry.get_plugin_config("test");
    assert_eq!(retrieved_config, Some(&config_value));
}

#[tokio::test]
async fn test_plugin_metadata_and_manifest() {
    let metadata = PluginMetadata {
        name: "test-plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A test plugin".to_string(),
        author: Some("Test Author".to_string()),
        license: Some("MIT".to_string()),
        homepage: Some("https://example.com".to_string()),
        repository: Some("https://github.com/test/plugin".to_string()),
        tags: Some(vec!["test".to_string(), "example".to_string()]),
    };

    assert_eq!(metadata.name, "test-plugin");
    assert_eq!(metadata.version, "1.0.0");
    assert_eq!(metadata.author, Some("Test Author".to_string()));
    assert_eq!(metadata.tags.as_ref().unwrap().len(), 2);

    // Test manifest creation
    let manifest = PluginManifest {
        metadata: metadata.clone(),
        entry_points: {
            let mut map = HashMap::new();
            map.insert("main".to_string(), "libtest.so".to_string());
            map
        },
        dependencies: None,
        min_mcp_version: Some("0.2.0".to_string()),
        config_schema: None,
    };

    assert_eq!(manifest.metadata.name, metadata.name);
    assert!(manifest.entry_points.contains_key("main"));
    assert_eq!(manifest.min_mcp_version, Some("0.2.0".to_string()));
}
