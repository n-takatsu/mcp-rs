use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn, error};

use crate::core::{
    Tool, Resource, ToolCallResult, ResourceReadResult, ResourceContent, Content, McpError
};
use crate::config::PluginConfig;
use crate::plugins::{
    Plugin, PluginMetadata, PluginResult, ToolProvider, ResourceProvider, PluginFactory, 
    UnifiedPlugin, PluginCapability
};

/// WordPress plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordPressConfig {
    /// WordPress site URL
    pub url: String,
    
    /// Authentication method
    pub auth: AuthConfig,
    
    /// Request timeout in seconds
    pub timeout: Option<u64>,
    
    /// Maximum posts to fetch
    pub max_posts: Option<usize>,
    
    /// Cache TTL in seconds
    pub cache_ttl: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthConfig {
    #[serde(rename = "none")]
    None,
    
    #[serde(rename = "basic")]
    Basic {
        username: String,
        password: String,
    },
    
    #[serde(rename = "jwt")]
    Jwt {
        token: String,
    },
    
    #[serde(rename = "application_password")]
    ApplicationPassword {
        username: String,
        password: String,
    },
}

/// WordPress API models
#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressPost {
    pub id: Option<u64>,
    pub title: HashMap<String, String>,
    pub content: HashMap<String, String>,
    pub excerpt: Option<HashMap<String, String>>,
    pub status: String,
    pub author: Option<u64>,
    pub date: Option<String>,
    pub modified: Option<String>,
    pub slug: Option<String>,
    pub link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressComment {
    pub id: Option<u64>,
    pub post: u64,
    pub content: HashMap<String, String>,
    pub author_name: String,
    pub author_email: String,
    pub author_url: Option<String>,
    pub date: Option<String>,
    pub status: Option<String>,
}

/// WordPress plugin
pub struct WordPressPlugin {
    config: Option<WordPressConfig>,
    client: Client,
}

impl WordPressPlugin {
    pub fn new() -> Self {
        Self {
            config: None,
            client: Client::new(),
        }
    }
    
    fn get_config(&self) -> Result<&WordPressConfig, McpError> {
        self.config.as_ref()
            .ok_or_else(|| McpError::other("WordPress plugin not initialized"))
    }
    
    async fn make_request(&self, endpoint: &str) -> Result<reqwest::Response, McpError> {
        let config = self.get_config()?;
        let url = format!("{}/wp-json/wp/v2/{}", config.url.trim_end_matches('/'), endpoint);
        
        let mut request = self.client.get(&url);
        
        // Apply authentication
        match &config.auth {
            AuthConfig::None => {},
            AuthConfig::Basic { username, password } |
            AuthConfig::ApplicationPassword { username, password } => {
                request = request.basic_auth(username, Some(password));
            },
            AuthConfig::Jwt { token } => {
                request = request.bearer_auth(token);
            },
        }
        
        // Apply timeout
        if let Some(timeout) = config.timeout {
            request = request.timeout(std::time::Duration::from_secs(timeout));
        }
        
        let response = request.send().await
            .map_err(|e| McpError::http(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(McpError::external_api(format!(
                "WordPress API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }
        
        Ok(response)
    }
    
    async fn get_posts(&self) -> Result<Vec<WordPressPost>, McpError> {
        let config = self.get_config()?;
        let mut endpoint = "posts".to_string();
        
        if let Some(max_posts) = config.max_posts {
            endpoint.push_str(&format!("?per_page={}", max_posts));
        }
        
        let response = self.make_request(&endpoint).await?;
        let posts: Vec<WordPressPost> = response.json().await
            .map_err(|e| McpError::Serialization(serde_json::Error::custom(e)))?;
        
        Ok(posts)
    }
    
    async fn get_comments(&self, post_id: Option<u64>) -> Result<Vec<WordPressComment>, McpError> {
        let mut endpoint = "comments".to_string();
        
        if let Some(post_id) = post_id {
            endpoint.push_str(&format!("?post={}", post_id));
        }
        
        let response = self.make_request(&endpoint).await?;
        let comments: Vec<WordPressComment> = response.json().await
            .map_err(|e| McpError::Serialization(serde_json::Error::custom(e)))?;
        
        Ok(comments)
    }
}

#[async_trait]
impl Plugin for WordPressPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "wordpress".to_string(),
            version: "0.1.0".to_string(),
            description: "WordPress REST API integration for MCP".to_string(),
            author: "n-takatsu".to_string(),
            homepage: Some("https://github.com/n-takatsu/mcp-rs".to_string()),
            dependencies: vec!["http".to_string()],
        }
    }
    
    async fn initialize(&mut self, config: &PluginConfig) -> PluginResult {
        let wp_config: WordPressConfig = serde_json::from_value(config.config.clone())
            .map_err(|e| McpError::InvalidParams(format!("Invalid WordPress config: {}", e)))?;
        
        info!("Initializing WordPress plugin with URL: {}", wp_config.url);
        self.config = Some(wp_config);
        
        Ok(())
    }
    
    async fn shutdown(&mut self) -> PluginResult {
        info!("Shutting down WordPress plugin");
        self.config = None;
        Ok(())
    }
    
    async fn health_check(&self) -> PluginResult<bool> {
        if let Ok(_) = self.get_config() {
            // Try to make a simple request to check connectivity
            match self.make_request("posts?per_page=1").await {
                Ok(_) => Ok(true),
                Err(e) => {
                    warn!("WordPress health check failed: {}", e);
                    Ok(false)
                }
            }
        } else {
            Ok(false)
        }
    }
}

#[async_trait]
impl ToolProvider for WordPressPlugin {
    async fn list_tools(&self) -> PluginResult<Vec<Tool>> {
        Ok(vec![
            Tool {
                name: "wordpress_get_posts".to_string(),
                description: "Retrieve WordPress posts".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of posts to retrieve",
                            "minimum": 1,
                            "maximum": 100,
                            "default": 10
                        }
                    }
                }),
            },
            Tool {
                name: "wordpress_get_comments".to_string(),
                description: "Retrieve WordPress comments".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "integer",
                            "description": "Optional post ID to filter comments"
                        }
                    }
                }),
            },
            Tool {
                name: "wordpress_search_posts".to_string(),
                description: "Search WordPress posts by keyword".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results",
                            "default": 10
                        }
                    },
                    "required": ["query"]
                }),
            },
        ])
    }
    
    async fn call_tool(&self, name: &str, arguments: Option<HashMap<String, Value>>) -> PluginResult<Value> {
        let args = arguments.unwrap_or_default();
        
        match name {
            "wordpress_get_posts" => {
                let posts = self.get_posts().await?;
                let result = ToolCallResult {
                    content: vec![Content::Text {
                        text: format!("Retrieved {} WordPress posts", posts.len()),
                    }],
                    is_error: Some(false),
                };
                Ok(serde_json::to_value(result)?)
            },
            
            "wordpress_get_comments" => {
                let post_id = args.get("post_id").and_then(|v| v.as_u64());
                let comments = self.get_comments(post_id).await?;
                let result = ToolCallResult {
                    content: vec![Content::Text {
                        text: format!("Retrieved {} WordPress comments", comments.len()),
                    }],
                    is_error: Some(false),
                };
                Ok(serde_json::to_value(result)?)
            },
            
            "wordpress_search_posts" => {
                let query = args.get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing query parameter".to_string()))?;
                
                let limit = args.get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10);
                
                let endpoint = format!("posts?search={}&per_page={}", 
                    urlencoding::encode(query), limit);
                
                let response = self.make_request(&endpoint).await?;
                let posts: Vec<WordPressPost> = response.json().await
                    .map_err(|e| McpError::Serialization(serde_json::Error::custom(e)))?;
                
                let result = ToolCallResult {
                    content: vec![Content::Text {
                        text: format!("Found {} posts matching '{}'", posts.len(), query),
                    }],
                    is_error: Some(false),
                };
                Ok(serde_json::to_value(result)?)
            },
            
            _ => Err(McpError::ToolNotFound(name.to_string())),
        }
    }
}

#[async_trait]
impl ResourceProvider for WordPressPlugin {
    async fn list_resources(&self) -> PluginResult<Vec<Resource>> {
        Ok(vec![
            Resource {
                uri: "wordpress://posts".to_string(),
                name: "WordPress Posts".to_string(),
                description: Some("All WordPress posts in JSON format".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            Resource {
                uri: "wordpress://comments".to_string(),
                name: "WordPress Comments".to_string(),
                description: Some("All WordPress comments in JSON format".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ])
    }
    
    async fn read_resource(&self, uri: &str) -> PluginResult<Value> {
        match uri {
            "wordpress://posts" => {
                let posts = self.get_posts().await?;
                let result = ResourceReadResult {
                    contents: vec![ResourceContent {
                        uri: uri.to_string(),
                        mime_type: Some("application/json".to_string()),
                        text: Some(serde_json::to_string_pretty(&posts)?),
                        blob: None,
                    }],
                };
                Ok(serde_json::to_value(result)?)
            },
            
            "wordpress://comments" => {
                let comments = self.get_comments(None).await?;
                let result = ResourceReadResult {
                    contents: vec![ResourceContent {
                        uri: uri.to_string(),
                        mime_type: Some("application/json".to_string()),
                        text: Some(serde_json::to_string_pretty(&comments)?),
                        blob: None,
                    }],
                };
                Ok(serde_json::to_value(result)?)
            },
            
            _ => Err(McpError::ResourceNotFound(uri.to_string())),
        }
    }
}

#[async_trait]
impl UnifiedPlugin for WordPressPlugin {
    fn capabilities(&self) -> Vec<PluginCapability> {
        vec![PluginCapability::Tools, PluginCapability::Resources]
    }
    
    async fn list_tools(&self) -> PluginResult<Vec<Tool>> {
        ToolProvider::list_tools(self).await
    }
    
    async fn call_tool(&self, name: &str, arguments: Option<HashMap<String, Value>>) -> PluginResult<Value> {
        ToolProvider::call_tool(self, name, arguments).await
    }
    
    async fn list_resources(&self) -> PluginResult<Vec<Resource>> {
        ResourceProvider::list_resources(self).await
    }
    
    async fn read_resource(&self, uri: &str) -> PluginResult<Value> {
        ResourceProvider::read_resource(self, uri).await
    }
}

/// WordPress plugin factory
pub struct WordPressPluginFactory;

impl PluginFactory for WordPressPluginFactory {
    fn create(&self) -> Box<dyn UnifiedPlugin> {
        Box::new(WordPressPlugin::new())
    }
    
    fn name(&self) -> &str {
        "wordpress"
    }
    
    fn capabilities(&self) -> Vec<PluginCapability> {
        vec![PluginCapability::Tools, PluginCapability::Resources]
    }
}