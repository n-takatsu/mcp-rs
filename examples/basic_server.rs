//! Basic MCP server example
//!
//! This example demonstrates how to create a simple MCP server with a few tools.
//!
//! Run with: cargo run --example basic_server

use async_trait::async_trait;
use mcp_rs::{
    protocol::McpProtocol,
    types::{Prompt, Resource, ServerCapabilities, ServerInfo, Tool},
    Error, McpServer, Result,
};
use serde_json::{json, Value};
use tracing_subscriber;

/// Basic MCP Protocol implementation
struct BasicProtocol {
    server_name: String,
    server_version: String,
}

impl BasicProtocol {
    fn new() -> Self {
        Self {
            server_name: "basic-mcp-server".to_string(),
            server_version: "0.1.0".to_string(),
        }
    }
}

#[async_trait]
impl McpProtocol for BasicProtocol {
    async fn initialize(&self) -> Result<(ServerInfo, ServerCapabilities)> {
        let info = ServerInfo {
            name: self.server_name.clone(),
            version: self.server_version.clone(),
        };

        let capabilities = ServerCapabilities {
            tools: Some(mcp_rs::types::ToolsCapability {
                list_changed: false,
            }),
            resources: None,
            prompts: None,
        };

        Ok((info, capabilities))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>> {
        Ok(vec![
            Tool {
                name: "echo".to_string(),
                description: "Echo back the input message".to_string(),
                input_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "The message to echo"
                        }
                    },
                    "required": ["message"]
                })),
            },
            Tool {
                name: "add".to_string(),
                description: "Add two numbers".to_string(),
                input_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "a": {
                            "type": "number",
                            "description": "First number"
                        },
                        "b": {
                            "type": "number",
                            "description": "Second number"
                        }
                    },
                    "required": ["a", "b"]
                })),
            },
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<Value> {
        match name {
            "echo" => {
                let args = arguments
                    .ok_or_else(|| Error::InvalidParams("Missing arguments".to_string()))?;

                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidParams("Missing message".to_string()))?;

                Ok(json!({
                    "echoed": message
                }))
            }
            "add" => {
                let args = arguments
                    .ok_or_else(|| Error::InvalidParams("Missing arguments".to_string()))?;

                let a = args
                    .get("a")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| Error::InvalidParams("Missing or invalid 'a'".to_string()))?;

                let b = args
                    .get("b")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| Error::InvalidParams("Missing or invalid 'b'".to_string()))?;

                Ok(json!({
                    "result": a + b
                }))
            }
            _ => Err(Error::MethodNotFound(format!(
                "Tool '{}' not found",
                name
            ))),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>> {
        Ok(vec![])
    }

    async fn read_resource(&self, uri: &str) -> Result<Value> {
        Err(Error::MethodNotFound(format!(
            "Resource '{}' not found",
            uri
        )))
    }

    async fn list_prompts(&self) -> Result<Vec<Prompt>> {
        Ok(vec![])
    }

    async fn get_prompt(&self, name: &str, _arguments: Option<Value>) -> Result<Value> {
        Err(Error::MethodNotFound(format!(
            "Prompt '{}' not found",
            name
        )))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("Starting Basic MCP Server");
    println!("Server listening on http://0.0.0.0:3000");

    // Create the protocol implementation
    let protocol = BasicProtocol::new();

    // Create and start the server
    let server = McpServer::new(protocol);
    server.serve(([0, 0, 0, 0], 3000)).await?;

    Ok(())
}
