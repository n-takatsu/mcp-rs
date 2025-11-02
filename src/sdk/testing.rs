//! Testing utilities for plugin development

use crate::config::PluginConfig;
use crate::core::{McpError, Resource, Tool};
use crate::plugins::{Plugin, ResourceProvider, ToolProvider};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Test harness for plugins
pub struct PluginTestHarness<P: Plugin> {
    plugin: P,
}

impl<P: Plugin> PluginTestHarness<P> {
    /// Create a new test harness
    pub fn new(plugin: P) -> Self {
        Self { plugin }
    }

    /// Initialize plugin with test configuration
    pub async fn initialize(&mut self, config: Value) -> Result<(), McpError> {
        let plugin_config = PluginConfig {
            enabled: true,
            priority: None,
            config,
        };

        self.plugin.initialize(&plugin_config).await
    }

    /// Get plugin reference
    pub fn plugin(&self) -> &P {
        &self.plugin
    }

    /// Get mutable plugin reference
    pub fn plugin_mut(&mut self) -> &mut P {
        &mut self.plugin
    }

    /// Test plugin health check
    pub async fn test_health(&self) -> Result<bool, McpError> {
        self.plugin.health_check().await
    }

    /// Test plugin shutdown
    pub async fn test_shutdown(&mut self) -> Result<(), McpError> {
        self.plugin.shutdown().await
    }
}

/// Test utilities for tool providers
impl<P: Plugin + ToolProvider> PluginTestHarness<P> {
    /// Test tool listing
    pub async fn test_list_tools(&self) -> Result<Vec<Tool>, McpError> {
        self.plugin.list_tools().await
    }

    /// Test tool call with mock arguments
    pub async fn test_call_tool(&self, name: &str, args: Value) -> Result<Value, McpError> {
        let args_map = match args {
            Value::Object(map) => {
                let mut hashmap = HashMap::new();
                for (k, v) in map {
                    hashmap.insert(k, v);
                }
                hashmap
            }
            _ => HashMap::new(),
        };

        self.plugin.call_tool(name, Some(args_map)).await
    }

    /// Test tool call without arguments
    pub async fn test_call_tool_no_args(&self, name: &str) -> Result<Value, McpError> {
        self.plugin.call_tool(name, None).await
    }
}

/// Test utilities for resource providers
impl<P: Plugin + ResourceProvider> PluginTestHarness<P> {
    /// Test resource listing
    pub async fn test_list_resources(&self) -> Result<Vec<Resource>, McpError> {
        self.plugin.list_resources().await
    }

    /// Test resource reading
    pub async fn test_read_resource(&self, uri: &str) -> Result<Value, McpError> {
        self.plugin.read_resource(uri).await
    }
}

/// Mock configuration builder
pub struct MockConfigBuilder {
    config: Value,
}

impl MockConfigBuilder {
    /// Create a new mock config builder
    pub fn new() -> Self {
        Self { config: json!({}) }
    }

    /// Add a string configuration value
    pub fn with_string(mut self, key: &str, value: &str) -> Self {
        self.config[key] = json!(value);
        self
    }

    /// Add an integer configuration value
    pub fn with_int(mut self, key: &str, value: i64) -> Self {
        self.config[key] = json!(value);
        self
    }

    /// Add a boolean configuration value
    pub fn with_bool(mut self, key: &str, value: bool) -> Self {
        self.config[key] = json!(value);
        self
    }

    /// Add a nested object
    pub fn with_object(mut self, key: &str, value: Value) -> Self {
        self.config[key] = value;
        self
    }

    /// Build the configuration
    pub fn build(self) -> Value {
        self.config
    }
}

impl Default for MockConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Assertion helpers for testing
pub struct TestAssertions;

impl TestAssertions {
    /// Assert that a tool has the expected schema
    pub fn assert_tool_schema(tool: &Tool, expected_properties: &[&str]) {
        let schema = &tool.input_schema;
        let properties = schema["properties"]
            .as_object()
            .expect("Tool schema should have properties object");

        for &prop in expected_properties {
            assert!(
                properties.contains_key(prop),
                "Tool '{}' should have property '{}'",
                tool.name,
                prop
            );
        }
    }

    /// Assert that a resource has the expected URI scheme
    pub fn assert_resource_uri_scheme(resource: &Resource, expected_scheme: &str) {
        assert!(
            resource.uri.starts_with(&format!("{}://", expected_scheme)),
            "Resource '{}' should have URI scheme '{}'",
            resource.name,
            expected_scheme
        );
    }

    /// Assert that a tool call result is successful
    pub fn assert_tool_success(result: &Value) {
        let is_error = result["is_error"].as_bool().unwrap_or(true);
        assert!(!is_error, "Tool call should be successful");

        let content = result["content"]
            .as_array()
            .expect("Tool result should have content array");
        assert!(!content.is_empty(), "Tool result should have content");
    }

    /// Assert that a tool call result is an error
    pub fn assert_tool_error(result: &Value) {
        let is_error = result["is_error"].as_bool().unwrap_or(false);
        assert!(is_error, "Tool call should be an error");
    }

    /// Assert that a resource read result has content
    pub fn assert_resource_content(result: &Value, expected_uri: &str) {
        let contents = result["contents"]
            .as_array()
            .expect("Resource result should have contents array");
        assert!(!contents.is_empty(), "Resource result should have content");

        let first_content = &contents[0];
        let uri = first_content["uri"]
            .as_str()
            .expect("Resource content should have URI");
        assert_eq!(uri, expected_uri, "Resource content URI should match");
    }
}

/// Integration test helpers
pub struct IntegrationTestUtils;

impl IntegrationTestUtils {
    /// Create a test plugin configuration file
    pub fn create_test_config(_plugin_name: &str, config: Value) -> PluginConfig {
        PluginConfig {
            enabled: true,
            priority: Some(1),
            config,
        }
    }

    /// Mock HTTP server for testing external API calls
    #[cfg(feature = "test-server")]
    pub fn mock_http_server() -> mockito::ServerGuard {
        mockito::Server::new()
    }

    /// Create test arguments for tool calls
    pub fn create_test_args(args: &[(&str, Value)]) -> HashMap<String, Value> {
        args.iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }
}

/// Async test helper macros
#[macro_export]
macro_rules! async_test {
    ($test_name:ident, $test_body:expr) => {
        #[tokio::test]
        async fn $test_name() {
            $test_body
        }
    };
}

#[macro_export]
macro_rules! test_plugin {
    ($plugin_type:ty, $config:expr, $test_body:expr) => {{
        let plugin = <$plugin_type>::new();
        let mut harness = $crate::sdk::testing::PluginTestHarness::new(plugin);
        harness
            .initialize($config)
            .await
            .expect("Plugin initialization failed");
        $test_body(harness).await
    }};
}
