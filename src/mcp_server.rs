//! MCP server implementation using axum.

use crate::{
    error::{Error, Result},
    protocol::McpProtocol,
    types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
};
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use serde_json::json;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

/// MCP Server that handles JSON-RPC requests
pub struct McpServer<P: McpProtocol> {
    protocol: Arc<P>,
}

impl<P: McpProtocol + 'static> McpServer<P> {
    /// Create a new MCP server with the given protocol implementation
    pub fn new(protocol: P) -> Self {
        Self {
            protocol: Arc::new(protocol),
        }
    }

    /// Create an axum Router for the server
    pub fn router(self) -> Router {
        let state = Arc::clone(&self.protocol);
        Router::new()
            .route("/", post(handle_request::<P>))
            .layer(TraceLayer::new_for_http())
            .with_state(state)
    }

    /// Start the server on the given address
    pub async fn serve(self, addr: impl Into<std::net::SocketAddr>) -> Result<()> {
        let addr = addr.into();
        info!("Starting MCP server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, self.router())
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(())
    }
}

/// Handle incoming JSON-RPC requests
async fn handle_request<P: McpProtocol>(
    State(protocol): State<Arc<P>>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    info!("Received request: method={}", request.method);

    let result =
        match request.method.as_str() {
            "initialize" => match protocol.initialize().await {
                Ok((info, capabilities)) => Ok(json!({
                    "serverInfo": info,
                    "capabilities": capabilities,
                })),
                Err(e) => Err(e),
            },
            "tools/list" => match protocol.list_tools().await {
                Ok(tools) => Ok(json!({ "tools": tools })),
                Err(e) => Err(e),
            },
            "tools/call" => match request.params.clone() {
                None => Err(Error::InvalidParams(
                    "Missing parameters for tools/call".to_string(),
                )),
                Some(params) => {
                    let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                        Error::InvalidParams("Missing 'name' parameter".to_string())
                    });

                    match name {
                        Err(e) => Err(e),
                        Ok(name) => {
                            let arguments = params.get("arguments").cloned();
                            protocol.call_tool(name, arguments).await
                        }
                    }
                }
            },
            "resources/list" => match protocol.list_resources().await {
                Ok(resources) => Ok(json!({ "resources": resources })),
                Err(e) => Err(e),
            },
            "resources/read" => match request.params.clone() {
                None => Err(Error::InvalidParams(
                    "Missing parameters for resources/read".to_string(),
                )),
                Some(params) => {
                    let uri = params
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| Error::InvalidParams("Missing 'uri' parameter".to_string()));

                    match uri {
                        Err(e) => Err(e),
                        Ok(uri) => protocol.read_resource(uri).await,
                    }
                }
            },
            "prompts/list" => match protocol.list_prompts().await {
                Ok(prompts) => Ok(json!({ "prompts": prompts })),
                Err(e) => Err(e),
            },
            "prompts/get" => match request.params.clone() {
                None => Err(Error::InvalidParams(
                    "Missing parameters for prompts/get".to_string(),
                )),
                Some(params) => {
                    let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                        Error::InvalidParams("Missing 'name' parameter".to_string())
                    });

                    match name {
                        Err(e) => Err(e),
                        Ok(name) => {
                            let arguments = params.get("arguments").cloned();
                            protocol.get_prompt(name, arguments).await
                        }
                    }
                }
            },
            _ => Err(Error::MethodNotFound(request.method.clone())),
        };

    let response = match result {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id: request.id,
        },
        Err(e) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: e.to_json_rpc_code(),
                message: e.to_string(),
                data: None,
            }),
            id: request.id,
        },
    };

    Json(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::BasicMcpProtocol;

    #[tokio::test]
    async fn test_server_creation() {
        let protocol = BasicMcpProtocol::new("test-server", "0.1.0");
        let server = McpServer::new(protocol);
        let _router = server.router();
    }

    #[tokio::test]
    async fn test_protocol_initialize() {
        let protocol = BasicMcpProtocol::new("test-server", "0.1.0");
        let result = protocol.initialize().await;
        assert!(result.is_ok());

        let (info, _capabilities) = result.unwrap();
        assert_eq!(info.name, "test-server");
        assert_eq!(info.version, "0.1.0");
    }

    #[tokio::test]
    async fn test_protocol_list_tools() {
        let protocol = BasicMcpProtocol::new("test-server", "0.1.0");
        let tools = protocol.list_tools().await.unwrap();
        assert_eq!(tools.len(), 0);
    }
}
