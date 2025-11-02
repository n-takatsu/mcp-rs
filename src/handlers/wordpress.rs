use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{info, warn};

use crate::config::WordPressConfig;
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolCallParams,
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
    pub date: Option<String>,
    pub date_gmt: Option<String>,
    pub guid: Option<WordPressGuid>,
    pub modified: Option<String>,
    pub modified_gmt: Option<String>,
    pub slug: Option<String>,
    pub status: String,
    #[serde(rename = "type")]
    pub post_type: Option<String>,
    pub link: Option<String>,
    pub title: WordPressContent,
    pub content: WordPressContent,
    pub excerpt: Option<WordPressContent>,
    pub author: Option<u64>,
    pub featured_media: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressGuid {
    pub rendered: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressContent {
    pub rendered: String,
    #[serde(default)]
    pub protected: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressComment {
    pub id: Option<u64>,
    pub post: u64,
    pub content: HashMap<String, String>,
    pub author_name: String,
    pub author_email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressMedia {
    pub id: Option<u64>,
    pub date: Option<String>,
    pub date_gmt: Option<String>,
    pub guid: Option<WordPressGuid>,
    pub modified: Option<String>,
    pub modified_gmt: Option<String>,
    pub slug: Option<String>,
    pub status: String,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    pub link: Option<String>,
    pub title: Option<WordPressContent>,
    pub author: Option<u64>,
    pub comment_status: Option<String>,
    pub ping_status: Option<String>,
    pub template: Option<String>,
    pub description: Option<WordPressContent>,
    pub caption: Option<WordPressContent>,
    pub alt_text: Option<String>,
    pub mime_type: Option<String>,
    pub media_details: Option<serde_json::Value>,
    pub post: Option<u64>,
    pub source_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressHealthCheck {
    pub site_accessible: bool,
    pub rest_api_available: bool,
    pub authentication_valid: bool,
    pub permissions_adequate: bool,
    pub media_upload_possible: bool,
    pub error_details: Vec<String>,
    pub site_info: Option<WordPressSiteInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressSiteInfo {
    pub name: String,
    pub description: String,
    pub url: String,
    pub admin_email: Option<String>,
    pub timezone_string: Option<String>,
    pub date_format: Option<String>,
    pub time_format: Option<String>,
    pub start_of_week: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressCategory {
    pub id: Option<u64>,
    pub count: Option<u64>,
    pub description: String,
    pub link: Option<String>,
    pub name: String,
    pub slug: String,
    pub taxonomy: Option<String>,
    pub parent: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressTag {
    pub id: Option<u64>,
    pub count: Option<u64>,
    pub description: String,
    pub link: Option<String>,
    pub name: String,
    pub slug: String,
    pub taxonomy: Option<String>,
}

impl WordPressHandler {
    pub fn new(config: WordPressConfig) -> Self {
        // タイムアウト設定付きのHTTPクライアントを作成
        let timeout_secs = config.timeout_seconds.unwrap_or(30);
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs)) // 設定可能なタイムアウト
            .connect_timeout(Duration::from_secs(10)) // 接続タイムアウト: 10秒
            .user_agent("mcp-rs/1.0") // User-Agentを設定
            .build()
            .expect("HTTP client build failed");

        Self {
            client,
            base_url: config.url,
            username: Some(config.username),
            password: Some(config.password),
        }
    }

    /// リトライ機能付きでHTTPリクエストを実行
    async fn execute_request_with_retry<T>(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<T, McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_millis(1000);

        for attempt in 1..=MAX_RETRIES {
            let request = request_builder
                .try_clone()
                .ok_or_else(|| McpError::Other("Failed to clone request".to_string()))?;

            match request.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        // レスポンステキストを取得してデバッグ
                        let text = response.text().await.map_err(McpError::Http)?;
                        warn!(
                            "Response body (first 500 chars): {}",
                            text.chars().take(500).collect::<String>()
                        );

                        match serde_json::from_str::<T>(&text) {
                            Ok(data) => return Ok(data),
                            Err(e) => {
                                warn!("JSON parse error on attempt {}: {}", attempt, e);
                                if attempt == MAX_RETRIES {
                                    return Err(McpError::ExternalApi(format!(
                                        "JSON parse error: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    } else if status.as_u16() >= 500 || status.as_u16() == 429 {
                        // サーバーエラーまたはレート制限の場合はリトライ
                        warn!("HTTP error {} on attempt {}, retrying...", status, attempt);
                        if attempt == MAX_RETRIES {
                            return Err(McpError::ExternalApi(format!(
                                "WordPress API error after {} attempts: {}",
                                MAX_RETRIES, status
                            )));
                        }
                    } else {
                        // クライアントエラー（4xx）はリトライしない
                        return Err(McpError::ExternalApi(format!(
                            "WordPress API client error: {}",
                            status
                        )));
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        warn!("Request timeout on attempt {}: {}", attempt, e);
                    } else if e.is_connect() {
                        warn!("Connection error on attempt {}: {}", attempt, e);
                    } else {
                        warn!("Request error on attempt {}: {}", attempt, e);
                    }

                    if attempt == MAX_RETRIES {
                        return Err(McpError::Http(e));
                    }
                }
            }

            // リトライ前に少し待機
            if attempt < MAX_RETRIES {
                tokio::time::sleep(RETRY_DELAY * attempt).await;
            }
        }

        unreachable!()
    }

    async fn get_posts(&self) -> Result<Vec<WordPressPost>, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts", self.base_url);

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Fetching WordPress posts from: {}", url);
        self.execute_request_with_retry(request).await
    }

    async fn create_post(&self, title: String, content: String) -> Result<WordPressPost, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts", self.base_url);

        let post = WordPressPost {
            id: None,
            date: None,
            date_gmt: None,
            guid: None,
            modified: None,
            modified_gmt: None,
            slug: None,
            status: "publish".to_string(),
            post_type: Some("post".to_string()),
            link: None,
            title: WordPressContent {
                rendered: title,
                protected: false,
            },
            content: WordPressContent {
                rendered: content,
                protected: false,
            },
            excerpt: None,
            author: None,
            featured_media: None,
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

    /// Upload media file to WordPress
    async fn upload_media(&self, file_data: &[u8], filename: &str, mime_type: &str) -> Result<WordPressMedia, McpError> {
        let url = format!("{}/wp-json/wp/v2/media", self.base_url);

        let form = reqwest::multipart::Form::new()
            .part("file", reqwest::multipart::Part::bytes(file_data.to_vec())
                .file_name(filename.to_string())
                .mime_str(mime_type)
                .map_err(|e| McpError::Other(format!("Failed to set MIME type: {}", e)))?);

        let mut request = self.client.post(&url).multipart(form);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Uploading media file: {} ({})", filename, mime_type);
        self.execute_request_with_retry(request).await
    }

    /// Create post with featured image
    async fn create_post_with_featured_image(
        &self, 
        title: String, 
        content: String, 
        featured_media_id: u64
    ) -> Result<WordPressPost, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts", self.base_url);

        let post = WordPressPost {
            id: None,
            date: None,
            date_gmt: None,
            guid: None,
            modified: None,
            modified_gmt: None,
            slug: None,
            status: "publish".to_string(),
            post_type: Some("post".to_string()),
            link: None,
            title: WordPressContent {
                rendered: title,
                protected: false,
            },
            content: WordPressContent {
                rendered: content,
                protected: false,
            },
            excerpt: None,
            author: None,
            featured_media: Some(featured_media_id),
        };

        let mut request = self.client.post(&url).json(&post);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Creating post with featured image: {}", featured_media_id);
        self.execute_request_with_retry(request).await
    }

    /// Set featured image for existing post
    async fn set_featured_image(&self, post_id: u64, media_id: u64) -> Result<WordPressPost, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts/{}", self.base_url, post_id);

        let update_data = serde_json::json!({
            "featured_media": media_id
        });

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Setting featured image {} for post {}", media_id, post_id);
        self.execute_request_with_retry(request).await
    }

    /// Get all categories
    pub async fn get_categories(&self) -> Result<Vec<WordPressCategory>, McpError> {
        let url = format!("{}/wp-json/wp/v2/categories", self.base_url);

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Fetching WordPress categories");
        self.execute_request_with_retry(request).await
    }

    /// Create a new category
    pub async fn create_category(&self, name: &str, description: Option<&str>, parent: Option<u64>) -> Result<WordPressCategory, McpError> {
        let url = format!("{}/wp-json/wp/v2/categories", self.base_url);

        let mut category_data = serde_json::json!({
            "name": name
        });

        if let Some(desc) = description {
            category_data["description"] = serde_json::Value::String(desc.to_string());
        }

        if let Some(parent_id) = parent {
            category_data["parent"] = serde_json::Value::Number(serde_json::Number::from(parent_id));
        }

        let mut request = self.client.post(&url).json(&category_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Creating category: {}", name);
        self.execute_request_with_retry(request).await
    }

    /// Update an existing category
    pub async fn update_category(&self, category_id: u64, name: Option<&str>, description: Option<&str>) -> Result<WordPressCategory, McpError> {
        let url = format!("{}/wp-json/wp/v2/categories/{}", self.base_url, category_id);

        let mut update_data = serde_json::Map::new();

        if let Some(name) = name {
            update_data.insert("name".to_string(), serde_json::Value::String(name.to_string()));
        }

        if let Some(desc) = description {
            update_data.insert("description".to_string(), serde_json::Value::String(desc.to_string()));
        }

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Updating category: {}", category_id);
        self.execute_request_with_retry(request).await
    }

    /// Delete a category
    pub async fn delete_category(&self, category_id: u64, force: bool) -> Result<serde_json::Value, McpError> {
        let url = format!("{}/wp-json/wp/v2/categories/{}", self.base_url, category_id);

        let mut request = self.client.delete(&url);

        if force {
            request = request.query(&[("force", "true")]);
        }

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Deleting category: {} (force: {})", category_id, force);
        self.execute_request_with_retry(request).await
    }

    /// Get all tags
    pub async fn get_tags(&self) -> Result<Vec<WordPressTag>, McpError> {
        let url = format!("{}/wp-json/wp/v2/tags", self.base_url);

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Fetching WordPress tags");
        self.execute_request_with_retry(request).await
    }

    /// Create a new tag
    pub async fn create_tag(&self, name: &str, description: Option<&str>) -> Result<WordPressTag, McpError> {
        let url = format!("{}/wp-json/wp/v2/tags", self.base_url);

        let mut tag_data = serde_json::json!({
            "name": name
        });

        if let Some(desc) = description {
            tag_data["description"] = serde_json::Value::String(desc.to_string());
        }

        let mut request = self.client.post(&url).json(&tag_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Creating tag: {}", name);
        self.execute_request_with_retry(request).await
    }

    /// Update an existing tag
    pub async fn update_tag(&self, tag_id: u64, name: Option<&str>, description: Option<&str>) -> Result<WordPressTag, McpError> {
        let url = format!("{}/wp-json/wp/v2/tags/{}", self.base_url, tag_id);

        let mut update_data = serde_json::Map::new();

        if let Some(name) = name {
            update_data.insert("name".to_string(), serde_json::Value::String(name.to_string()));
        }

        if let Some(desc) = description {
            update_data.insert("description".to_string(), serde_json::Value::String(desc.to_string()));
        }

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Updating tag: {}", tag_id);
        self.execute_request_with_retry(request).await
    }

    /// Delete a tag
    pub async fn delete_tag(&self, tag_id: u64, force: bool) -> Result<serde_json::Value, McpError> {
        let url = format!("{}/wp-json/wp/v2/tags/{}", self.base_url, tag_id);

        let mut request = self.client.delete(&url);

        if force {
            request = request.query(&[("force", "true")]);
        }

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Deleting tag: {} (force: {})", tag_id, force);
        self.execute_request_with_retry(request).await
    }

    /// Perform comprehensive health check of WordPress environment
    pub async fn health_check(&self) -> WordPressHealthCheck {
        let mut health = WordPressHealthCheck {
            site_accessible: false,
            rest_api_available: false,
            authentication_valid: false,
            permissions_adequate: false,
            media_upload_possible: false,
            error_details: Vec::new(),
            site_info: None,
        };

        info!("Starting WordPress health check for: {}", self.base_url);

        // 1. Check site accessibility
        match self.check_site_accessibility().await {
            Ok(site_info) => {
                health.site_accessible = true;
                health.site_info = Some(site_info);
                info!("✅ Site accessibility: OK");
            }
            Err(e) => {
                health.error_details.push(format!("Site accessibility failed: {}", e));
                warn!("❌ Site accessibility: FAILED - {}", e);
                return health; // If site is not accessible, skip other checks
            }
        }

        // 2. Check REST API availability
        if let Err(e) = self.check_rest_api().await {
            health.error_details.push(format!("REST API check failed: {}", e));
            warn!("❌ REST API availability: FAILED - {}", e);
            return health;
        } else {
            health.rest_api_available = true;
            info!("✅ REST API availability: OK");
        }

        // 3. Check authentication
        if let Err(e) = self.check_authentication().await {
            health.error_details.push(format!("Authentication failed: {}", e));
            warn!("❌ Authentication: FAILED - {}", e);
            return health;
        } else {
            health.authentication_valid = true;
            info!("✅ Authentication: OK");
        }

        // 4. Check permissions
        if let Err(e) = self.check_permissions().await {
            health.error_details.push(format!("Permissions check failed: {}", e));
            warn!("❌ Permissions: FAILED - {}", e);
        } else {
            health.permissions_adequate = true;
            info!("✅ Permissions: OK");
        }

        // 5. Check media upload capability
        if let Err(e) = self.check_media_upload_capability().await {
            health.error_details.push(format!("Media upload check failed: {}", e));
            warn!("❌ Media upload capability: FAILED - {}", e);
        } else {
            health.media_upload_possible = true;
            info!("✅ Media upload capability: OK");
        }

        if health.error_details.is_empty() {
            info!("🎉 WordPress health check completed successfully!");
        } else {
            warn!("⚠️ WordPress health check completed with {} issues", health.error_details.len());
        }

        health
    }

    /// Check if WordPress site is accessible
    async fn check_site_accessibility(&self) -> Result<WordPressSiteInfo, McpError> {
        let url = format!("{}/wp-json/wp/v2/settings", self.base_url);
        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .map_err(|e| McpError::ExternalApi(format!("Failed to connect to WordPress: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress site not accessible. Status: {}", 
                response.status()
            )));
        }

        let settings: serde_json::Value = response.json().await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse site info: {}", e)))?;

        Ok(WordPressSiteInfo {
            name: settings.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
            description: settings.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            url: settings.get("url").and_then(|v| v.as_str()).unwrap_or(&self.base_url).to_string(),
            admin_email: settings.get("admin_email").and_then(|v| v.as_str()).map(|s| s.to_string()),
            timezone_string: settings.get("timezone_string").and_then(|v| v.as_str()).map(|s| s.to_string()),
            date_format: settings.get("date_format").and_then(|v| v.as_str()).map(|s| s.to_string()),
            time_format: settings.get("time_format").and_then(|v| v.as_str()).map(|s| s.to_string()),
            start_of_week: settings.get("start_of_week").and_then(|v| v.as_u64()).map(|n| n as u8),
        })
    }

    /// Check if WordPress REST API is available
    async fn check_rest_api(&self) -> Result<(), McpError> {
        let url = format!("{}/wp-json/wp/v2", self.base_url);
        
        let response = self.client.get(&url).send().await
            .map_err(|e| McpError::ExternalApi(format!("REST API check failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress REST API not available. Status: {}", 
                response.status()
            )));
        }

        // Check if response contains expected namespace
        let api_info: serde_json::Value = response.json().await
            .map_err(|e| McpError::ExternalApi(format!("Invalid REST API response: {}", e)))?;

        if !api_info.get("namespaces").and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|ns| ns.as_str() == Some("wp/v2")))
            .unwrap_or(false) {
            return Err(McpError::ExternalApi("WordPress REST API v2 not available".to_string()));
        }

        Ok(())
    }

    /// Check if authentication credentials are valid
    async fn check_authentication(&self) -> Result<(), McpError> {
        let url = format!("{}/wp-json/wp/v2/users/me", self.base_url);
        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        } else {
            return Err(McpError::ExternalApi("No authentication credentials provided".to_string()));
        }

        let response = request.send().await
            .map_err(|e| McpError::ExternalApi(format!("Authentication check failed: {}", e)))?;

        match response.status().as_u16() {
            200 => Ok(()),
            401 => Err(McpError::ExternalApi("Invalid credentials".to_string())),
            403 => Err(McpError::ExternalApi("Authentication forbidden".to_string())),
            _ => Err(McpError::ExternalApi(format!("Authentication failed with status: {}", response.status())))
        }
    }

    /// Check if user has adequate permissions
    async fn check_permissions(&self) -> Result<(), McpError> {
        // Check if user can read posts
        let posts_url = format!("{}/wp-json/wp/v2/posts?per_page=1", self.base_url);
        let mut request = self.client.get(&posts_url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .map_err(|e| McpError::ExternalApi(format!("Posts permission check failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "No permission to read posts. Status: {}", 
                response.status()
            )));
        }

        // Check if user can access media
        let media_url = format!("{}/wp-json/wp/v2/media?per_page=1", self.base_url);
        let mut request = self.client.get(&media_url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .map_err(|e| McpError::ExternalApi(format!("Media permission check failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "No permission to access media. Status: {}", 
                response.status()
            )));
        }

        Ok(())
    }

    /// Check if media upload is possible
    async fn check_media_upload_capability(&self) -> Result<(), McpError> {
        // Check if user can access media endpoint with GET request
        let url = format!("{}/wp-json/wp/v2/media?per_page=1", self.base_url);
        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await
            .map_err(|e| McpError::ExternalApi(format!("Media upload capability check failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "Cannot access media endpoint. Status: {}", 
                response.status()
            )));
        }

        // If we can read media, we can likely upload (assuming proper permissions)
        // We could also check upload_max_filesize but that requires admin access
        Ok(())
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
                name: "wordpress_health_check".to_string(),
                description: "Perform comprehensive WordPress environment health check".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
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
            Tool {
                name: "upload_media".to_string(),
                description: "Upload media file to WordPress".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_data": {
                            "type": "string",
                            "description": "Base64 encoded file data"
                        },
                        "filename": {
                            "type": "string",
                            "description": "Original filename"
                        },
                        "mime_type": {
                            "type": "string",
                            "description": "MIME type of the file (e.g., 'image/jpeg')"
                        }
                    },
                    "required": ["file_data", "filename", "mime_type"]
                }),
            },
            Tool {
                name: "create_post_with_featured_image".to_string(),
                description: "Create a new WordPress post with featured image".to_string(),
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
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Media ID for featured image"
                        }
                    },
                    "required": ["title", "content", "featured_media_id"]
                }),
            },
            Tool {
                name: "set_featured_image".to_string(),
                description: "Set featured image for existing post".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post ID to update"
                        },
                        "media_id": {
                            "type": "number",
                            "description": "Media ID for featured image"
                        }
                    },
                    "required": ["post_id", "media_id"]
                }),
            },
            Tool {
                name: "get_categories".to_string(),
                description: "Retrieve WordPress categories".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "create_category".to_string(),
                description: "Create a new WordPress category".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Category name"
                        },
                        "description": {
                            "type": "string",
                            "description": "Category description (optional)"
                        },
                        "parent": {
                            "type": "number",
                            "description": "Parent category ID (optional)"
                        }
                    },
                    "required": ["name"]
                }),
            },
            Tool {
                name: "update_category".to_string(),
                description: "Update an existing WordPress category".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category_id": {
                            "type": "number",
                            "description": "Category ID to update"
                        },
                        "name": {
                            "type": "string",
                            "description": "New category name (optional)"
                        },
                        "description": {
                            "type": "string",
                            "description": "New category description (optional)"
                        }
                    },
                    "required": ["category_id"]
                }),
            },
            Tool {
                name: "delete_category".to_string(),
                description: "Delete a WordPress category".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category_id": {
                            "type": "number",
                            "description": "Category ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force delete (bypass trash)"
                        }
                    },
                    "required": ["category_id"]
                }),
            },
            Tool {
                name: "get_tags".to_string(),
                description: "Retrieve WordPress tags".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "create_tag".to_string(),
                description: "Create a new WordPress tag".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Tag name"
                        },
                        "description": {
                            "type": "string",
                            "description": "Tag description (optional)"
                        }
                    },
                    "required": ["name"]
                }),
            },
            Tool {
                name: "update_tag".to_string(),
                description: "Update an existing WordPress tag".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tag_id": {
                            "type": "number",
                            "description": "Tag ID to update"
                        },
                        "name": {
                            "type": "string",
                            "description": "New tag name (optional)"
                        },
                        "description": {
                            "type": "string",
                            "description": "New tag description (optional)"
                        }
                    },
                    "required": ["tag_id"]
                }),
            },
            Tool {
                name: "delete_tag".to_string(),
                description: "Delete a WordPress tag".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tag_id": {
                            "type": "number",
                            "description": "Tag ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force delete (bypass trash)"
                        }
                    },
                    "required": ["tag_id"]
                }),
            },
        ])
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
        match params.name.as_str() {
            "wordpress_health_check" => {
                info!("Performing WordPress health check...");
                let health = self.health_check().await;
                
                let status_emoji = if health.error_details.is_empty() { "✅" } else { "⚠️" };
                let status_text = if health.error_details.is_empty() { "HEALTHY" } else { "ISSUES DETECTED" };
                
                let mut report = format!("{} WordPress Health Check: {}\n\n", status_emoji, status_text);
                
                if let Some(site_info) = &health.site_info {
                    report.push_str(&format!("🌐 Site: {} ({})\n", site_info.name, site_info.url));
                    report.push_str(&format!("📝 Description: {}\n\n", site_info.description));
                }
                
                report.push_str("📊 Health Status:\n");
                report.push_str(&format!("  • Site Accessible: {}\n", if health.site_accessible { "✅" } else { "❌" }));
                report.push_str(&format!("  • REST API Available: {}\n", if health.rest_api_available { "✅" } else { "❌" }));
                report.push_str(&format!("  • Authentication Valid: {}\n", if health.authentication_valid { "✅" } else { "❌" }));
                report.push_str(&format!("  • Permissions Adequate: {}\n", if health.permissions_adequate { "✅" } else { "❌" }));
                report.push_str(&format!("  • Media Upload Possible: {}\n", if health.media_upload_possible { "✅" } else { "❌" }));
                
                if !health.error_details.is_empty() {
                    report.push_str("\n🚨 Issues Found:\n");
                    for (i, error) in health.error_details.iter().enumerate() {
                        report.push_str(&format!("  {}. {}\n", i + 1, error));
                    }
                }

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": report
                    }],
                    "isError": !health.error_details.is_empty()
                }))
            }
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
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;

                let post = self
                    .create_post(title.to_string(), content.to_string())
                    .await?;
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
                let post_id = args.get("post_id").and_then(|v| v.as_u64());

                let comments = self.get_comments(post_id).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} comments", comments.len())
                    }],
                    "isError": false
                }))
            }
            "upload_media" => {
                let args = params.arguments.unwrap_or_default();
                let file_data_b64 = args
                    .get("file_data")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing file_data".to_string()))?;
                let filename = args
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing filename".to_string()))?;
                let mime_type = args
                    .get("mime_type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing mime_type".to_string()))?;

                // Decode base64 file data
                use base64::{engine::general_purpose, Engine as _};
                let file_data = general_purpose::STANDARD
                    .decode(file_data_b64)
                    .map_err(|e| McpError::InvalidParams(format!("Invalid base64 data: {}", e)))?;

                let media = self.upload_media(&file_data, filename, mime_type).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Uploaded media with ID: {:?}", media.id)
                    }],
                    "isError": false
                }))
            }
            "create_post_with_featured_image" => {
                let args = params.arguments.unwrap_or_default();
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;
                let featured_media_id = args
                    .get("featured_media_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing featured_media_id".to_string()))?;

                let post = self
                    .create_post_with_featured_image(title.to_string(), content.to_string(), featured_media_id)
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post with featured image. Post ID: {:?}", post.id)
                    }],
                    "isError": false
                }))
            }
            "set_featured_image" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args
                    .get("post_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let media_id = args
                    .get("media_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing media_id".to_string()))?;

                let post = self.set_featured_image(post_id, media_id).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Set featured image {} for post {}. Updated post ID: {:?}", media_id, post_id, post.id)
                    }],
                    "isError": false
                }))
            }
            "get_categories" => {
                let categories = self.get_categories().await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} categories:\n{}", 
                            categories.len(),
                            serde_json::to_string_pretty(&categories)
                                .unwrap_or_else(|_| "Failed to serialize categories".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "create_category" => {
                let args = params.arguments.unwrap_or_default();
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing name".to_string()))?;
                let description = args.get("description").and_then(|v| v.as_str());
                let parent = args.get("parent").and_then(|v| v.as_u64());

                let category = self.create_category(name, description, parent).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created category '{}' with ID: {:?}", name, category.id)
                    }],
                    "isError": false
                }))
            }
            "update_category" => {
                let args = params.arguments.unwrap_or_default();
                let category_id = args
                    .get("category_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing category_id".to_string()))?;
                let name = args.get("name").and_then(|v| v.as_str());
                let description = args.get("description").and_then(|v| v.as_str());

                let category = self.update_category(category_id, name, description).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated category ID {} to '{}'", category_id, category.name)
                    }],
                    "isError": false
                }))
            }
            "delete_category" => {
                let args = params.arguments.unwrap_or_default();
                let category_id = args
                    .get("category_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing category_id".to_string()))?;
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

                self.delete_category(category_id, force).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Deleted category ID {} (force: {})", category_id, force)
                    }],
                    "isError": false
                }))
            }
            "get_tags" => {
                let tags = self.get_tags().await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} tags:\n{}", 
                            tags.len(),
                            serde_json::to_string_pretty(&tags)
                                .unwrap_or_else(|_| "Failed to serialize tags".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "create_tag" => {
                let args = params.arguments.unwrap_or_default();
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing name".to_string()))?;
                let description = args.get("description").and_then(|v| v.as_str());

                let tag = self.create_tag(name, description).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created tag '{}' with ID: {:?}", name, tag.id)
                    }],
                    "isError": false
                }))
            }
            "update_tag" => {
                let args = params.arguments.unwrap_or_default();
                let tag_id = args
                    .get("tag_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing tag_id".to_string()))?;
                let name = args.get("name").and_then(|v| v.as_str());
                let description = args.get("description").and_then(|v| v.as_str());

                let tag = self.update_tag(tag_id, name, description).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated tag ID {} to '{}'", tag_id, tag.name)
                    }],
                    "isError": false
                }))
            }
            "delete_tag" => {
                let args = params.arguments.unwrap_or_default();
                let tag_id = args
                    .get("tag_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing tag_id".to_string()))?;
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

                self.delete_tag(tag_id, force).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Deleted tag ID {} (force: {})", tag_id, force)
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
            Resource {
                uri: "wordpress://categories".to_string(),
                name: "WordPress Categories".to_string(),
                description: Some("All WordPress categories".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            Resource {
                uri: "wordpress://tags".to_string(),
                name: "WordPress Tags".to_string(),
                description: Some("All WordPress tags".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ])
    }

    async fn read_resource(
        &self,
        params: ResourceReadParams,
    ) -> Result<serde_json::Value, McpError> {
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
            "wordpress://categories" => {
                let categories = self.get_categories().await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&categories)?
                    }]
                }))
            }
            "wordpress://tags" => {
                let tags = self.get_tags().await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&tags)?
                    }]
                }))
            }
            _ => Err(McpError::ResourceNotFound(params.uri)),
        }
    }
}
