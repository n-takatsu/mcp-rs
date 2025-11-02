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
            _ => Err(McpError::ResourceNotFound(params.uri)),
        }
    }
}
