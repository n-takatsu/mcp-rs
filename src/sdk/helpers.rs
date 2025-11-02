//! Helper functions for common plugin development tasks

use crate::core::{Content, McpError};
use serde_json::Value;
use std::collections::HashMap;

/// Plugin development utilities
pub struct PluginUtils;

impl PluginUtils {
    /// Validate required parameters
    pub fn validate_required_params(
        args: &HashMap<String, Value>,
        required: &[&str],
    ) -> Result<(), McpError> {
        for param in required {
            if !args.contains_key(*param) {
                return Err(McpError::InvalidParams {
                    message: format!("Missing required parameter: {}", param),
                });
            }
        }
        Ok(())
    }

    /// Safely extract string parameter
    pub fn get_string_param(args: &HashMap<String, Value>, key: &str) -> Option<String> {
        args.get(key)?.as_str().map(|s| s.to_string())
    }

    /// Safely extract integer parameter
    pub fn get_int_param(args: &HashMap<String, Value>, key: &str) -> Option<i64> {
        args.get(key)?.as_i64()
    }

    /// Safely extract boolean parameter
    pub fn get_bool_param(args: &HashMap<String, Value>, key: &str) -> Option<bool> {
        args.get(key)?.as_bool()
    }

    /// Safely extract array parameter
    pub fn get_array_param<'a>(
        args: &'a HashMap<String, Value>,
        key: &str,
    ) -> Option<&'a Vec<Value>> {
        args.get(key)?.as_array()
    }

    /// Create text content
    pub fn text_content(text: impl Into<String>) -> Content {
        Content::Text { text: text.into() }
    }

    /// Create image content
    pub fn image_content(data: impl Into<String>, mime_type: impl Into<String>) -> Content {
        Content::Image {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Format error message
    pub fn format_error(error: impl std::fmt::Display) -> String {
        format!("Error: {}", error)
    }

    /// Convert result to JSON value with error handling
    pub fn to_json_value<T: serde::Serialize>(value: T) -> Result<Value, McpError> {
        serde_json::to_value(value).map_err(McpError::from)
    }

    /// Parse JSON string safely
    pub fn parse_json_string(json_str: &str) -> Result<Value, McpError> {
        serde_json::from_str(json_str).map_err(McpError::from)
    }

    /// Merge two JSON objects
    pub fn merge_json_objects(base: &mut Value, overlay: Value) -> Result<(), McpError> {
        match (base, overlay) {
            (Value::Object(base_map), Value::Object(overlay_map)) => {
                for (key, value) in overlay_map {
                    base_map.insert(key, value);
                }
                Ok(())
            }
            _ => Err(McpError::InvalidParams {
                message: "Both values must be JSON objects".to_string(),
            }),
        }
    }
}

/// HTTP client helpers
#[cfg(feature = "http")]
pub struct HttpUtils;

#[cfg(feature = "http")]
impl HttpUtils {
    /// Create a configured HTTP client
    pub fn create_client(timeout_secs: Option<u64>) -> reqwest::Client {
        let mut builder =
            reqwest::Client::builder().user_agent(format!("mcp-rs/{}", env!("CARGO_PKG_VERSION")));

        if let Some(timeout) = timeout_secs {
            builder = builder.timeout(std::time::Duration::from_secs(timeout));
        }

        builder.build().unwrap_or_default()
    }

    /// Make a GET request with error handling
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        client: &reqwest::Client,
        url: &str,
        headers: Option<reqwest::header::HeaderMap>,
    ) -> Result<T, McpError> {
        let mut request = client.get(url);

        if let Some(headers) = headers {
            request = request.headers(headers);
        }

        let response = request.send().await.map_err(McpError::from)?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        response.json().await.map_err(McpError::from)
    }

    /// Make a POST request with JSON body
    pub async fn post_json<T: serde::de::DeserializeOwned>(
        client: &reqwest::Client,
        url: &str,
        body: &impl serde::Serialize,
        headers: Option<reqwest::header::HeaderMap>,
    ) -> Result<T, McpError> {
        let mut request = client.post(url).json(body);

        if let Some(headers) = headers {
            request = request.headers(headers);
        }

        let response = request.send().await.map_err(McpError::from)?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        response.json().await.map_err(McpError::from)
    }
}

/// Configuration helpers
pub struct ConfigUtils;

impl ConfigUtils {
    /// Load plugin configuration with validation
    pub fn load_plugin_config<T: serde::de::DeserializeOwned>(
        config: &crate::config::PluginConfig,
    ) -> Result<T, McpError> {
        serde_json::from_value(config.config.clone()).map_err(|e| McpError::InvalidParams {
            message: format!("Invalid plugin configuration: {}", e),
        })
    }

    /// Get environment variable with fallback
    pub fn get_env_var(key: &str, default: Option<&str>) -> Option<String> {
        std::env::var(key)
            .ok()
            .or_else(|| default.map(|s| s.to_string()))
    }

    /// Validate URL format
    pub fn validate_url(url: &str) -> Result<(), McpError> {
        url::Url::parse(url).map_err(|_| McpError::InvalidParams {
            message: format!("Invalid URL format: {}", url),
        })?;
        Ok(())
    }

    /// Sanitize string for use in URIs
    pub fn sanitize_uri_component(input: &str) -> String {
        input
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c,
                _ => '_',
            })
            .collect()
    }
}

/// Async utilities
pub struct AsyncUtils;

impl AsyncUtils {
    /// Run multiple futures concurrently with timeout
    pub async fn timeout_all<T>(
        futures: Vec<impl std::future::Future<Output = T>>,
        timeout: std::time::Duration,
    ) -> Result<Vec<T>, McpError> {
        let combined = futures::future::join_all(futures);

        tokio::time::timeout(timeout, combined)
            .await
            .map_err(|_| McpError::Other {
                message: "Operation timed out".to_string(),
            })
    }

    /// Retry an operation with exponential backoff
    pub async fn retry_with_backoff<T, F, Fut>(
        mut operation: F,
        max_retries: usize,
        base_delay: std::time::Duration,
    ) -> Result<T, McpError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, McpError>>,
    {
        let mut delay = base_delay;

        for attempt in 0..=max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt == max_retries => return Err(e),
                Err(_) => {
                    tokio::time::sleep(delay).await;
                    delay *= 2; // Exponential backoff
                }
            }
        }

        unreachable!()
    }
}
