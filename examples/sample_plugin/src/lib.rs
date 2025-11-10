//! Sample Plugin Implementation
//!
//! This is an example of how to create a plugin for mcp-rs.

use async_trait::async_trait;
use mcp_rs::mcp::{InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolCallParams};
use mcp_rs::plugins::{DynamicPlugin, PluginMetadata, PluginStatus};
use serde_json::json;
use std::sync::Arc;

/// Sample plugin implementation
pub struct SamplePlugin {
    metadata: PluginMetadata,
    initialized: bool,
    config: Option<serde_json::Value>,
}

impl SamplePlugin {
    pub fn new() -> Self {
        Self {
            metadata: PluginMetadata {
                name: "sample-plugin".to_string(),
                version: "1.0.0".to_string(),
                description: "A sample plugin for demonstration".to_string(),
                author: Some("MCP-RS Team".to_string()),
                license: Some("MIT OR Apache-2.0".to_string()),
                homepage: Some("https://github.com/n-takatsu/mcp-rs".to_string()),
                repository: Some("https://github.com/n-takatsu/mcp-rs".to_string()),
                tags: Some(vec!["sample".to_string(), "demo".to_string()]),
            },
            initialized: false,
            config: None,
        }
    }
}

impl DynamicPlugin for SamplePlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(&mut self, config: Option<serde_json::Value>) -> Result<(), McpError> {
        self.config = config;
        self.initialized = true;
        Ok(())
    }

    fn create_handlers(&self) -> Result<Vec<(String, Arc<dyn McpHandler>)>, McpError> {
        if !self.initialized {
            return Err(McpError::InvalidRequest(
                "Plugin not initialized".to_string(),
            ));
        }

        let handler = Arc::new(SampleHandler::new());
        Ok(vec![("sample-handler".to_string(), handler)])
    }

    fn shutdown(&mut self) -> Result<(), McpError> {
        self.initialized = false;
        Ok(())
    }

    fn validate_config(&self, _config: &serde_json::Value) -> Result<(), McpError> {
        // Sample validation - accept any valid JSON object
        Ok(())
    }

    fn status(&self) -> PluginStatus {
        if self.initialized {
            PluginStatus::Active
        } else {
            PluginStatus::Loaded
        }
    }
}

/// Sample handler implementation
struct SampleHandler {
    name: String,
}

impl SampleHandler {
    fn new() -> Self {
        Self {
            name: "Sample Handler".to_string(),
        }
    }
}

#[async_trait]
impl McpHandler for SampleHandler {
    async fn initialize(
        &self,
        _params: InitializeParams,
    ) -> Result<serde_json::Value, McpError> {
        Ok(json!({
            "status": "initialized",
            "handler": self.name,
            "capabilities": {
                "tools": true,
                "resources": true
            }
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool {
                name: "sample_echo".to_string(),
                description: Some("Echo the input text".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Text to echo"
                        }
                    },
                    "required": ["text"]
                }),
            },
            Tool {
                name: "sample_random".to_string(),
                description: Some("Generate a random number".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "min": {
                            "type": "number",
                            "description": "Minimum value"
                        },
                        "max": {
                            "type": "number",
                            "description": "Maximum value"
                        }
                    },
                    "required": ["min", "max"]
                }),
            },
        ])
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
        match params.name.as_str() {
            "sample_echo" => {
                let text = params.arguments
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing 'text' parameter".to_string()))?;

                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Echo: {}", text)
                    }]
                }))
            }
            "sample_random" => {
                let min = params.arguments
                    .get("min")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| McpError::InvalidParams("Missing 'min' parameter".to_string()))?;
                
                let max = params.arguments
                    .get("max")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| McpError::InvalidParams("Missing 'max' parameter".to_string()))?;

                if min >= max {
                    return Err(McpError::InvalidParams("min must be less than max".to_string()));
                }

                // Simple random number generation (in production, use a proper RNG)
                let random = min + (max - min) * 0.42; // Deterministic for demo

                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Random number between {} and {}: {}", min, max, random)
                    }]
                }))
            }
            _ => Err(McpError::ToolNotFound(format!("Tool '{}' not found", params.name))),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        Ok(vec![
            Resource {
                uri: "sample://info".to_string(),
                name: Some("Plugin Information".to_string()),
                description: Some("Information about the sample plugin".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ])
    }

    async fn read_resource(&self, params: ResourceReadParams) -> Result<serde_json::Value, McpError> {
        match params.uri.as_str() {
            "sample://info" => {
                Ok(json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&json!({
                            "plugin": "sample-plugin",
                            "version": "1.0.0",
                            "description": "A sample plugin for demonstration",
                            "handlers": ["sample-handler"],
                            "tools": ["sample_echo", "sample_random"],
                            "resources": ["sample://info"]
                        }))?
                    }]
                }))
            }
            _ => Err(McpError::InvalidRequest(format!("Resource '{}' not found", params.uri))),
        }
    }
}

// Export the plugin using the macro
mcp_rs::export_plugin!(SamplePlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sample_plugin() {
        let mut plugin = SamplePlugin::new();
        
        // Test metadata
        assert_eq!(plugin.metadata().name, "sample-plugin");
        assert_eq!(plugin.metadata().version, "1.0.0");
        assert_eq!(plugin.status(), PluginStatus::Loaded);

        // Test initialization
        plugin.initialize(None).unwrap();
        assert_eq!(plugin.status(), PluginStatus::Active);

        // Test handler creation
        let handlers = plugin.create_handlers().unwrap();
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].0, "sample-handler");

        // Test shutdown
        plugin.shutdown().unwrap();
        assert_eq!(plugin.status(), PluginStatus::Loaded);
    }

    #[tokio::test]
    async fn test_sample_handler() {
        let handler = SampleHandler::new();

        // Test tool listing
        let tools = handler.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "sample_echo");
        assert_eq!(tools[1].name, "sample_random");

        // Test echo tool
        let echo_params = ToolCallParams {
            name: "sample_echo".to_string(),
            arguments: json!({"text": "Hello, World!"}),
        };
        let result = handler.call_tool(echo_params).await.unwrap();
        assert!(result["content"][0]["text"].as_str().unwrap().contains("Hello, World!"));

        // Test resource listing
        let resources = handler.list_resources().await.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "sample://info");
    }
}