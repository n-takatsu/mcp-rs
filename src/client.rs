//! MCP client for making requests to MCP servers.

use crate::{
    error::{Error, Result},
    types::{JsonRpcRequest, JsonRpcResponse, RequestId},
};
use reqwest::Client;
use serde_json::Value;

/// MCP Client for communicating with MCP servers
pub struct McpClient {
    client: Client,
    base_url: String,
    next_id: std::sync::atomic::AtomicI64,
}

impl McpClient {
    /// Create a new MCP client
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            next_id: std::sync::atomic::AtomicI64::new(1),
        }
    }

    /// Make a JSON-RPC request to the server
    pub async fn request(&self, method: impl Into<String>, params: Option<Value>) -> Result<Value> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
            id: RequestId::Number(id),
        };

        let response = self
            .client
            .post(&self.base_url)
            .json(&request)
            .send()
            .await?;

        let json_response: JsonRpcResponse = response.json().await?;

        if let Some(error) = json_response.error {
            return Err(Error::InternalError(format!(
                "Server error: {} (code: {})",
                error.message, error.code
            )));
        }

        json_response
            .result
            .ok_or_else(|| Error::InternalError("No result in response".to_string()))
    }

    /// Initialize connection with the server
    pub async fn initialize(&self) -> Result<Value> {
        self.request("initialize", None).await
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<Value> {
        self.request("tools/list", None).await
    }

    /// Call a tool
    pub async fn call_tool(&self, name: impl Into<String>, arguments: Option<Value>) -> Result<Value> {
        let params = serde_json::json!({
            "name": name.into(),
            "arguments": arguments
        });
        self.request("tools/call", Some(params)).await
    }

    /// List available resources
    pub async fn list_resources(&self) -> Result<Value> {
        self.request("resources/list", None).await
    }

    /// Read a resource
    pub async fn read_resource(&self, uri: impl Into<String>) -> Result<Value> {
        let params = serde_json::json!({
            "uri": uri.into()
        });
        self.request("resources/read", Some(params)).await
    }

    /// List available prompts
    pub async fn list_prompts(&self) -> Result<Value> {
        self.request("prompts/list", None).await
    }

    /// Get a prompt
    pub async fn get_prompt(&self, name: impl Into<String>, arguments: Option<Value>) -> Result<Value> {
        let params = serde_json::json!({
            "name": name.into(),
            "arguments": arguments
        });
        self.request("prompts/get", Some(params)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = McpClient::new("http://localhost:3000");
        assert_eq!(client.base_url, "http://localhost:3000");
    }

    #[test]
    fn test_id_increment() {
        let client = McpClient::new("http://localhost:3000");
        let id1 = client
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let id2 = client
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(id2, id1 + 1);
    }
}
