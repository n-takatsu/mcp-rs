use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

use crate::mcp::{
    McpHandler, McpError, Tool, Resource, InitializeParams, 
    ToolCallParams, ResourceReadParams
};

#[derive(Debug, Clone)]
pub struct WordPressHandler {
    client: Client,
    base_url: String,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressPost {
    pub id: Option<u64>,
    pub title: HashMap<String, String>,
    pub content: HashMap<String, String>,
    pub status: String,
    pub author: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressComment {
    pub id: Option<u64>,
    pub post: u64,
    pub content: HashMap<String, String>,
    pub author_name: String,
    pub author_email: String,
}

impl WordPressHandler {
    pub fn new(base_url: String, username: Option<String>, password: Option<String>) -> Self {
        Self {
            client: Client::new(),
            base_url,
            username,
            password,
        }
    }

    async fn get_posts(&self) -> Result<Vec<WordPressPost>, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts", self.base_url);
        
        let mut request = self.client.get(&url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await?;
        
        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress API error: {}",
                response.status()
            )));
        }

        let posts: Vec<WordPressPost> = response.json().await?;
        Ok(posts)
    }

    async fn create_post(&self, title: String, content: String) -> Result<WordPressPost, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts", self.base_url);
        
        let mut title_map = HashMap::new();
        title_map.insert("rendered".to_string(), title);
        
        let mut content_map = HashMap::new();
        content_map.insert("rendered".to_string(), content);
        
        let post = WordPressPost {
            id: None,
            title: title_map,
            content: content_map,
            status: "publish".to_string(),
            author: None,
        };

        let mut request = self.client.post(&url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.json(&post).send().await?;
        
        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress API error: {}",
                response.status()
            )));
        }

        let created_post: WordPressPost = response.json().await?;
        Ok(created_post)
    }

    async fn get_comments(&self, post_id: Option<u64>) -> Result<Vec<WordPressComment>, McpError> {
        let mut url = format!("{}/wp-json/wp/v2/comments", self.base_url);
        
        if let Some(post_id) = post_id {
            url = format!("{}?post={}", url, post_id);
        }
        
        let mut request = self.client.get(&url);
        
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await?;
        
        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress API error: {}",
                response.status()
            )));
        }

        let comments: Vec<WordPressComment> = response.json().await?;
        Ok(comments)
    }
}

#[async_trait]
impl McpHandler for WordPressHandler {
    async fn initialize(&self, _params: InitializeParams) -> Result<serde_json::Value, McpError> {
        info!("WordPress MCP Handler initialized");
        Ok(serde_json::json!({
            "protocol_version": "2024-11-05",
            "capabilities": {
                "tools": {
                    "list_changed": false
                },
                "resources": {
                    "subscribe": false,
                    "list_changed": false
                }
            },
            "server_info": {
                "name": "mcp-rs-wordpress",
                "version": "0.1.0"
            }
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool {
                name: "get_posts".to_string(),
                description: "Retrieve WordPress posts".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "create_post".to_string(),
                description: "Create a new WordPress post".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "The post content"
                        }
                    },
                    "required": ["title", "content"]
                }),
            },
            Tool {
                name: "get_comments".to_string(),
                description: "Retrieve WordPress comments".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Optional post ID to filter comments"
                        }
                    },
                    "required": []
                }),
            },
        ])
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
        match params.name.as_str() {
            "get_posts" => {
                let posts = self.get_posts().await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} posts", posts.len())
                    }],
                    "isError": false
                }))
            }
            "create_post" => {
                let args = params.arguments.unwrap_or_default();
                let title = args.get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args.get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;
                
                let post = self.create_post(title.to_string(), content.to_string()).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post with ID: {:?}", post.id)
                    }],
                    "isError": false
                }))
            }
            "get_comments" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args.get("post_id")
                    .and_then(|v| v.as_u64());
                
                let comments = self.get_comments(post_id).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} comments", comments.len())
                    }],
                    "isError": false
                }))
            }
            _ => Err(McpError::ToolNotFound(params.name)),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        Ok(vec![
            Resource {
                uri: "wordpress://posts".to_string(),
                name: "WordPress Posts".to_string(),
                description: Some("All WordPress posts".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            Resource {
                uri: "wordpress://comments".to_string(),
                name: "WordPress Comments".to_string(),
                description: Some("All WordPress comments".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ])
    }

    async fn read_resource(&self, params: ResourceReadParams) -> Result<serde_json::Value, McpError> {
        match params.uri.as_str() {
            "wordpress://posts" => {
                let posts = self.get_posts().await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&posts)?
                    }]
                }))
            }
            "wordpress://comments" => {
                let comments = self.get_comments(None).await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&comments)?
                    }]
                }))
            }
            _ => Err(McpError::ResourceNotFound(params.uri)),
        }
    }
}