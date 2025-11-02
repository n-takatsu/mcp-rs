//! Example MCP server with WordPress integration
//!
//! This example demonstrates how to create an MCP server that provides
//! tools for interacting with WordPress REST API.
//!
//! Run with: cargo run --example wordpress_server

use async_trait::async_trait;
use mcp_rs::{
    protocol::McpProtocol,
    types::{Prompt, Resource, ServerCapabilities, ServerInfo, Tool},
    Error, McpServer, Result,
};
use reqwest::Client;
use serde_json::{json, Value};
use tracing_subscriber;

/// WordPress MCP Protocol implementation
struct WordPressMcpProtocol {
    server_name: String,
    server_version: String,
    wordpress_url: String,
    client: Client,
}

impl WordPressMcpProtocol {
    fn new(wordpress_url: String) -> Self {
        Self {
            server_name: "wordpress-mcp-server".to_string(),
            server_version: "0.1.0".to_string(),
            wordpress_url,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl McpProtocol for WordPressMcpProtocol {
    async fn initialize(&self) -> Result<(ServerInfo, ServerCapabilities)> {
        let info = ServerInfo {
            name: self.server_name.clone(),
            version: self.server_version.clone(),
        };

        let capabilities = ServerCapabilities {
            tools: Some(mcp_rs::types::ToolsCapability {
                list_changed: false,
            }),
            resources: Some(mcp_rs::types::ResourcesCapability {
                subscribe: false,
                list_changed: false,
            }),
            prompts: None,
        };

        Ok((info, capabilities))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>> {
        Ok(vec![
            Tool {
                name: "get_posts".to_string(),
                description: "Get WordPress posts".to_string(),
                input_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "per_page": {
                            "type": "number",
                            "description": "Number of posts to retrieve",
                            "default": 10
                        },
                        "page": {
                            "type": "number",
                            "description": "Page number",
                            "default": 1
                        }
                    }
                })),
            },
            Tool {
                name: "create_post".to_string(),
                description: "Create a new WordPress post".to_string(),
                input_schema: Some(json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Post content"
                        },
                        "status": {
                            "type": "string",
                            "description": "Post status (draft, publish)",
                            "default": "draft"
                        }
                    },
                    "required": ["title", "content"]
                })),
            },
        ])
    }

    async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<Value> {
        match name {
            "get_posts" => {
                let per_page = arguments
                    .as_ref()
                    .and_then(|v| v.get("per_page"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10);

                let page = arguments
                    .as_ref()
                    .and_then(|v| v.get("page"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1);

                let url = format!(
                    "{}/wp-json/wp/v2/posts?per_page={}&page={}",
                    self.wordpress_url, per_page, page
                );

                let response = self
                    .client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| Error::NetworkError(e))?;

                let posts: Value = response
                    .json()
                    .await
                    .map_err(|e| Error::NetworkError(e))?;

                Ok(json!({
                    "posts": posts
                }))
            }
            "create_post" => {
                let args = arguments
                    .ok_or_else(|| Error::InvalidParams("Missing arguments".to_string()))?;

                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidParams("Missing title".to_string()))?;

                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::InvalidParams("Missing content".to_string()))?;

                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("draft");

                let url = format!("{}/wp-json/wp/v2/posts", self.wordpress_url);

                let response = self
                    .client
                    .post(&url)
                    .json(&json!({
                        "title": title,
                        "content": content,
                        "status": status
                    }))
                    .send()
                    .await
                    .map_err(|e| Error::NetworkError(e))?;

                let post: Value = response
                    .json()
                    .await
                    .map_err(|e| Error::NetworkError(e))?;

                Ok(json!({
                    "post": post
                }))
            }
            _ => Err(Error::MethodNotFound(format!(
                "Tool '{}' not found",
                name
            ))),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>> {
        Ok(vec![
            Resource {
                uri: format!("{}/wp-json/wp/v2/posts", self.wordpress_url),
                name: "WordPress Posts".to_string(),
                description: Some("All WordPress posts".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            Resource {
                uri: format!("{}/wp-json/wp/v2/pages", self.wordpress_url),
                name: "WordPress Pages".to_string(),
                description: Some("All WordPress pages".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ])
    }

    async fn read_resource(&self, uri: &str) -> Result<Value> {
        let response = self
            .client
            .get(uri)
            .send()
            .await
            .map_err(|e| Error::NetworkError(e))?;

        let data: Value = response
            .json()
            .await
            .map_err(|e| Error::NetworkError(e))?;

        Ok(data)
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

    // Get WordPress URL from environment or use default
    let wordpress_url = std::env::var("WORDPRESS_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());

    println!("Starting WordPress MCP Server");
    println!("WordPress URL: {}", wordpress_url);
    println!("Server listening on http://0.0.0.0:3000");

    // Create the protocol implementation
    let protocol = WordPressMcpProtocol::new(wordpress_url);

    // Create and start the server
    let server = McpServer::new(protocol);
    server.serve(([0, 0, 0, 0], 3000)).await?;

    Ok(())
}
