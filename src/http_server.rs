//! HTTP JSON-RPC Server for MCP-RS
//! 
//! This module provides an HTTP server that accepts JSON-RPC requests
//! and forwards them to the MCP handlers.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::post,
    Router,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

use crate::{
    mcp::{JsonRpcRequest, JsonRpcResponse, McpHandler, McpError, ResourceReadParams, InitializeParams, ToolCallParams},
};

pub struct HttpJsonRpcServer {
    handlers: HashMap<String, Arc<dyn McpHandler>>,
}

impl HttpJsonRpcServer {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn add_handler(&mut self, name: String, handler: Arc<dyn McpHandler>) {
        self.handlers.insert(name, handler);
    }

    pub async fn serve(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let app = Router::new()
            .route("/", post(handle_json_rpc))
            .route("/mcp", post(handle_json_rpc))
            .layer(CorsLayer::permissive())
            .with_state(Arc::new(self.handlers.clone()));

        info!("Starting HTTP JSON-RPC server on {}", addr);
        
        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
        
        Ok(())
    }
}

async fn handle_json_rpc(
    State(handlers): State<Arc<HashMap<String, Arc<dyn McpHandler>>>>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, StatusCode> {
    info!("Received JSON-RPC request: method={}", request.method);
    
    let result = process_mcp_request(&request, &handlers).await;
    
    match result {
        Ok(response) => {
            info!("JSON-RPC request processed successfully");
            Ok(Json(response))
        }
        Err(e) => {
            error!("Error processing JSON-RPC request: {}", e);
            let error_response = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(e.into()),
                id: request.id,
            };
            Ok(Json(error_response))
        }
    }
}

async fn process_mcp_request(
    request: &JsonRpcRequest,
    handlers: &HashMap<String, Arc<dyn McpHandler>>,
) -> Result<JsonRpcResponse, McpError> {
    let handler = handlers
        .values()
        .next()
        .ok_or_else(|| McpError::InvalidMethod("No handlers available".to_string()))?;

    let result = match request.method.as_str() {
        "initialize" => {
            let params: InitializeParams = if let Some(params) = &request.params {
                serde_json::from_value(params.clone())?
            } else {
                InitializeParams {
                    protocol_version: "2024-11-05".to_string(),
                    capabilities: json!({}),
                    client_info: json!({}),
                }
            };
            handler.initialize(params).await
        }
        "tools/list" => {
            let tools = handler.list_tools().await?;
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => {
            let params: ToolCallParams = serde_json::from_value(
                request.params.clone().unwrap_or_default()
            )?;
            handler.call_tool(params).await
        }
        "resources/list" => {
            let resources = handler.list_resources().await?;
            Ok(json!({ "resources": resources }))
        }
        "resources/read" => {
            let params: ResourceReadParams = serde_json::from_value(
                request.params.clone().unwrap_or_default()
            )?;
            handler.read_resource(params).await
        }
        _ => Err(McpError::InvalidMethod(request.method.clone())),
    };

    match result {
        Ok(result) => Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id: request.id.clone(),
        }),
        Err(e) => Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(e.into()),
            id: request.id.clone(),
        }),
    }
}