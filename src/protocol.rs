//! MCP protocol trait abstraction.

use crate::{
    error::Result,
    types::{Prompt, Resource, ServerCapabilities, ServerInfo, Tool},
};
use async_trait::async_trait;
use serde_json::Value;

/// Core trait for implementing MCP protocol handlers
///
/// This trait provides the abstraction for handling MCP protocol methods.
/// Implementors can provide custom logic for tools, resources, and prompts.
#[async_trait]
pub trait McpProtocol: Send + Sync {
    /// Initialize the server and return server information and capabilities
    async fn initialize(&self) -> Result<(ServerInfo, ServerCapabilities)>;

    /// List available tools
    async fn list_tools(&self) -> Result<Vec<Tool>>;

    /// Call a tool with given arguments
    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<Value>;

    /// List available resources
    async fn list_resources(&self) -> Result<Vec<Resource>>;

    /// Read a resource by URI
    async fn read_resource(&self, uri: &str) -> Result<Value>;

    /// List available prompts
    async fn list_prompts(&self) -> Result<Vec<Prompt>>;

    /// Get a prompt by name with arguments
    async fn get_prompt(&self, name: &str, arguments: Option<Value>) -> Result<Value>;
}

/// Basic MCP protocol implementation with default behaviors
pub struct BasicMcpProtocol {
    server_name: String,
    server_version: String,
    tools: Vec<Tool>,
    resources: Vec<Resource>,
    prompts: Vec<Prompt>,
}

impl BasicMcpProtocol {
    /// Create a new basic protocol implementation
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            server_name: name.into(),
            server_version: version.into(),
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        }
    }

    /// Add a tool to the protocol
    pub fn add_tool(&mut self, tool: Tool) {
        self.tools.push(tool);
    }

    /// Add a resource to the protocol
    pub fn add_resource(&mut self, resource: Resource) {
        self.resources.push(resource);
    }

    /// Add a prompt to the protocol
    pub fn add_prompt(&mut self, prompt: Prompt) {
        self.prompts.push(prompt);
    }
}

#[async_trait]
impl McpProtocol for BasicMcpProtocol {
    async fn initialize(&self) -> Result<(ServerInfo, ServerCapabilities)> {
        let info = ServerInfo {
            name: self.server_name.clone(),
            version: self.server_version.clone(),
        };

        let capabilities = ServerCapabilities {
            tools: if !self.tools.is_empty() {
                Some(crate::types::ToolsCapability {
                    list_changed: false,
                })
            } else {
                None
            },
            resources: if !self.resources.is_empty() {
                Some(crate::types::ResourcesCapability {
                    subscribe: false,
                    list_changed: false,
                })
            } else {
                None
            },
            prompts: if !self.prompts.is_empty() {
                Some(crate::types::PromptsCapability {
                    list_changed: false,
                })
            } else {
                None
            },
        };

        Ok((info, capabilities))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>> {
        Ok(self.tools.clone())
    }

    async fn call_tool(&self, name: &str, _arguments: Option<Value>) -> Result<Value> {
        // Default implementation returns an error
        Err(crate::error::Error::MethodNotFound(format!(
            "Tool '{}' not implemented",
            name
        )))
    }

    async fn list_resources(&self) -> Result<Vec<Resource>> {
        Ok(self.resources.clone())
    }

    async fn read_resource(&self, uri: &str) -> Result<Value> {
        // Default implementation returns an error
        Err(crate::error::Error::MethodNotFound(format!(
            "Resource '{}' not found",
            uri
        )))
    }

    async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        Ok(self.prompts.clone())
    }

    async fn get_prompt(&self, name: &str, _arguments: Option<Value>) -> Result<Value> {
        // Default implementation returns an error
        Err(crate::error::Error::MethodNotFound(format!(
            "Prompt '{}' not found",
            name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_protocol_initialization() {
        let protocol = BasicMcpProtocol::new("test-server", "0.1.0");
        let result = protocol.initialize().await;
        assert!(result.is_ok());

        let (info, _capabilities) = result.unwrap();
        assert_eq!(info.name, "test-server");
        assert_eq!(info.version, "0.1.0");
    }

    #[tokio::test]
    async fn test_list_tools_empty() {
        let protocol = BasicMcpProtocol::new("test-server", "0.1.0");
        let tools = protocol.list_tools().await.unwrap();
        assert_eq!(tools.len(), 0);
    }

    #[tokio::test]
    async fn test_add_tool() {
        let mut protocol = BasicMcpProtocol::new("test-server", "0.1.0");
        protocol.add_tool(Tool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: None,
        });

        let tools = protocol.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test_tool");
    }
}
