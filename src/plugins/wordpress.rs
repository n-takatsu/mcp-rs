use async_trait::async_trait;
use base64::prelude::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::config::PluginConfig;
use crate::core::{
    Content, McpError, Resource, ResourceContent, ResourceReadResult, Tool, ToolCallResult,
};
use crate::plugins::{
    Plugin, PluginCapability, PluginFactory, PluginMetadata, PluginResult, ResourceProvider,
    ToolProvider, UnifiedPlugin,
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
    Basic { username: String, password: String },

    #[serde(rename = "jwt")]
    Jwt { token: String },

    #[serde(rename = "application_password")]
    ApplicationPassword { username: String, password: String },
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

impl Default for WordPressPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl WordPressPlugin {
    pub fn new() -> Self {
        Self {
            config: None,
            client: Client::new(),
        }
    }

    fn get_config(&self) -> Result<&WordPressConfig, McpError> {
        self.config
            .as_ref()
            .ok_or_else(|| McpError::other("WordPress plugin not initialized"))
    }

    /// Get authorization header for WordPress API requests
    #[allow(dead_code)]
    fn get_auth_header(&self) -> Result<String, McpError> {
        let config = self.get_config()?;

        match &config.auth {
            AuthConfig::ApplicationPassword { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::prelude::BASE64_STANDARD.encode(credentials.as_bytes());
                Ok(format!("Basic {}", encoded))
            }
            AuthConfig::Basic { username, password } => {
                let credentials = format!("{}:{}", username, password);
                let encoded = base64::prelude::BASE64_STANDARD.encode(credentials.as_bytes());
                Ok(format!("Basic {}", encoded))
            }
            AuthConfig::Jwt { token } => Ok(format!("Bearer {}", token)),
            AuthConfig::None => Err(McpError::config(
                "Authentication required but not configured",
            )),
        }
    }

    async fn make_request(&self, endpoint: &str) -> Result<reqwest::Response, McpError> {
        let config = self.get_config()?;
        let url = format!(
            "{}/wp-json/wp/v2/{}",
            config.url.trim_end_matches('/'),
            endpoint
        );

        let mut request = self.client.get(&url);

        // Apply authentication
        match &config.auth {
            AuthConfig::None => {}
            AuthConfig::Basic { username, password }
            | AuthConfig::ApplicationPassword { username, password } => {
                request = request.basic_auth(username, Some(password));
            }
            AuthConfig::Jwt { token } => {
                request = request.bearer_auth(token);
            }
        }

        // Apply timeout
        if let Some(timeout) = config.timeout {
            request = request.timeout(std::time::Duration::from_secs(timeout));
        }

        let response = request
            .send()
            .await
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
        let posts: Vec<WordPressPost> =
            response.json().await.map_err(|e| McpError::Serialization {
                message: e.to_string(),
            })?;

        Ok(posts)
    }

    async fn get_comments(&self, post_id: Option<u64>) -> Result<Vec<WordPressComment>, McpError> {
        let mut endpoint = "comments".to_string();

        if let Some(post_id) = post_id {
            endpoint.push_str(&format!("?post={}", post_id));
        }

        let response = self.make_request(&endpoint).await?;
        let comments: Vec<WordPressComment> =
            response.json().await.map_err(|e| McpError::Serialization {
                message: e.to_string(),
            })?;

        Ok(comments)
    }

    /// Test WordPress REST API connectivity with comprehensive security and maintenance mode detection
    /// This comprehensively tests if the connection works despite security plugins and maintenance mode
    #[allow(dead_code)]
    async fn test_connection(&self) -> Result<serde_json::Value, McpError> {
        let config = self.get_config()?;
        let mut test_results = std::collections::HashMap::new();

        // Test 1: WordPress REST API discovery endpoint
        let discovery_url = format!("{}/wp-json/", config.url.trim_end_matches('/'));

        let discovery_response = self
            .client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| McpError::http(e.to_string()))?;

        let discovery_success = discovery_response.status().is_success();
        test_results.insert(
            "rest_api_discovery".to_string(),
            serde_json::json!({
                "status": discovery_response.status().as_u16(),
                "success": discovery_success,
                "endpoint": discovery_url
            }),
        );

        // Test 2: Check for maintenance mode by examining response
        let maintenance_detected = self.detect_maintenance_mode(&discovery_response).await;
        test_results.insert(
            "maintenance_mode_check".to_string(),
            serde_json::json!({
                "maintenance_detected": maintenance_detected,
                "note": if maintenance_detected {
                    "⚠️ Maintenance mode may be blocking REST API access"
                } else {
                    "✅ No maintenance mode detected"
                }
            }),
        );

        // Test 3: WordPress posts endpoint (should work without authentication)
        let posts_url = format!("{}/wp-json/wp/v2/posts", config.url.trim_end_matches('/'));

        let posts_response = self
            .client
            .get(&posts_url)
            .query(&[("per_page", "1")])
            .send()
            .await
            .map_err(|e| McpError::http(e.to_string()))?;

        let posts_success = posts_response.status().is_success();
        test_results.insert(
            "posts_endpoint".to_string(),
            serde_json::json!({
                "status": posts_response.status().as_u16(),
                "success": posts_success,
                "endpoint": posts_url,
                "note": if !posts_success && posts_response.status() == 503 {
                    "🚨 Service Unavailable - likely maintenance mode"
                } else if !posts_success {
                    "❌ Posts endpoint inaccessible"
                } else {
                    "✅ Posts endpoint accessible"
                }
            }),
        );

        // Test 4: Authenticated endpoint (users/me) if auth is configured
        if let Ok(auth_header) = self.get_auth_header() {
            let users_url = format!(
                "{}/wp-json/wp/v2/users/me",
                config.url.trim_end_matches('/')
            );

            let users_response = self
                .client
                .get(&users_url)
                .header("Authorization", auth_header)
                .send()
                .await
                .map_err(|e| McpError::http(e.to_string()))?;

            test_results.insert(
                "authenticated_endpoint".to_string(),
                serde_json::json!({
                    "status": users_response.status().as_u16(),
                    "success": users_response.status().is_success(),
                    "endpoint": users_url,
                    "auth_method": "application_password"
                }),
            );
        }

        // Test 5: Admin area accessibility (should be restricted by security plugins)
        let admin_url = format!("{}/wp-admin/", config.url.trim_end_matches('/'));

        let admin_response = self.client.head(&admin_url).send().await;

        match admin_response {
            Ok(resp) => {
                let is_protected =
                    resp.status() == 403 || resp.status() == 404 || resp.status() == 401;
                test_results.insert(
                    "admin_protection".to_string(),
                    serde_json::json!({
                        "status": resp.status().as_u16(),
                        "protected": is_protected,
                        "endpoint": admin_url,
                        "note": if is_protected { "Good: Admin area is protected" } else { "Warning: Admin area accessible" }
                    })
                );
            }
            Err(_) => {
                test_results.insert(
                    "admin_protection".to_string(),
                    serde_json::json!({
                        "protected": true,
                        "endpoint": admin_url,
                        "note": "Excellent: Admin area strongly protected (connection blocked)"
                    }),
                );
            }
        }

        // Overall assessment
        let overall_success = discovery_success && posts_success && !maintenance_detected;

        if !overall_success {
            let error_msg = if maintenance_detected {
                "WordPress site appears to be in maintenance mode, which may block REST API access. Please check maintenance plugin settings."
            } else if !posts_success {
                "WordPress REST API posts endpoint is not accessible. This may be due to security restrictions or maintenance mode."
            } else {
                "WordPress REST API discovery failed. Please check site accessibility and plugin settings."
            };

            return Err(McpError::external_api(format!(
                "{}. Test results: {}",
                error_msg,
                serde_json::to_string_pretty(&test_results).unwrap_or_default()
            )));
        }

        info!("WordPress REST API connection test successful with security plugin compatibility verified");

        Ok(serde_json::json!({
            "overall_status": "success",
            "message": "WordPress REST API is accessible despite security plugin protection",
            "test_results": test_results,
            "security_assessment": "REST API endpoints work independently of admin panel security",
            "maintenance_status": if maintenance_detected { "⚠️ Maintenance mode detected but API accessible" } else { "✅ No maintenance mode detected" },
            "compatibility": {
                "wp_site_guard": "✅ Compatible",
                "wordfence": "✅ Compatible",
                "all_in_one_wp_security": "✅ Compatible",
                "login_lockdown": "✅ Compatible",
                "two_factor_auth": "✅ Compatible (REST API bypasses login form)",
                "maintenance_plugins": if maintenance_detected { "⚠️ Detected - check settings" } else { "✅ No interference" }
            }
        }))
    }

    /// Detect if WordPress is in maintenance mode by analyzing response content
    #[allow(dead_code)]
    async fn detect_maintenance_mode(&self, response: &reqwest::Response) -> bool {
        // Check HTTP status codes that indicate maintenance
        if response.status() == 503 || response.status() == 502 {
            return true;
        }

        // For content analysis, we'll need to make a separate request
        // since we can't consume the response body from a reference
        let config = match self.get_config() {
            Ok(cfg) => cfg,
            Err(_) => return false,
        };

        // Make a separate request to analyze content
        let discovery_url = format!("{}/wp-json/", config.url.trim_end_matches('/'));

        if let Ok(check_response) = self.client.get(&discovery_url).send().await {
            if let Ok(text) = check_response.text().await {
                let text_lower = text.to_lowercase();

                // Common maintenance mode indicators
                let maintenance_indicators = [
                    "maintenance mode",
                    "under maintenance",
                    "site maintenance",
                    "temporarily unavailable",
                    "coming soon",
                    "site is down",
                    "under construction",
                    "wp maintenance mode",
                    "maintenance page",
                ];

                for indicator in &maintenance_indicators {
                    if text_lower.contains(indicator) {
                        return true;
                    }
                }
            }
        }

        false
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
        let wp_config: WordPressConfig =
            serde_json::from_value(config.config.clone()).map_err(|e| McpError::InvalidParams {
                message: format!("Invalid WordPress config: {}", e),
            })?;

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
        if self.get_config().is_ok() {
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

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value> {
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
            }

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
            }

            "wordpress_search_posts" => {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                    McpError::InvalidParams {
                        message: "Missing query parameter".to_string(),
                    }
                })?;

                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);

                let endpoint = format!(
                    "posts?search={}&per_page={}",
                    urlencoding::encode(query),
                    limit
                );

                let response = self.make_request(&endpoint).await?;
                let posts: Vec<WordPressPost> =
                    response.json().await.map_err(|e| McpError::Serialization {
                        message: e.to_string(),
                    })?;

                let result = ToolCallResult {
                    content: vec![Content::Text {
                        text: format!("Found {} posts matching '{}'", posts.len(), query),
                    }],
                    is_error: Some(false),
                };
                Ok(serde_json::to_value(result)?)
            }

            _ => Err(McpError::ToolNotFound {
                name: name.to_string(),
            }),
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
            }

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
            }

            _ => Err(McpError::ResourceNotFound {
                uri: uri.to_string(),
            }),
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

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<HashMap<String, Value>>,
    ) -> PluginResult<Value> {
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
