use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

use crate::mcp::{
    JsonRpcRequest, JsonRpcResponse, McpError, Tool, Resource,
    InitializeParams, ToolCallParams, ResourceReadParams
};

#[async_trait]
pub trait McpHandler: Send + Sync {
    async fn initialize(&self, params: InitializeParams) -> Result<serde_json::Value, McpError>;
    async fn list_tools(&self) -> Result<Vec<Tool>, McpError>;
    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError>;
    async fn list_resources(&self) -> Result<Vec<Resource>, McpError>;
    async fn read_resource(&self, params: ResourceReadParams) -> Result<serde_json::Value, McpError>;
}

pub struct McpServer {
    handlers: HashMap<String, Arc<dyn McpHandler>>,
    capabilities: ServerCapabilities,
}

#[derive(Debug, Clone)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>,
}

#[derive(Debug, Clone)]
pub struct ToolsCapability {
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ResourcesCapability {
    pub subscribe: Option<bool>,
    pub list_changed: Option<bool>,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourcesCapability {
                    subscribe: Some(false),
                    list_changed: Some(false),
                }),
            },
        }
    }

    pub fn add_handler(&mut self, name: String, handler: Arc<dyn McpHandler>) {
        self.handlers.insert(name, handler);
    }

    pub async fn run(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        info!("MCP Server listening on {}", addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let handlers = self.handlers.clone();
            let capabilities = self.capabilities.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, handlers, capabilities).await {
                    error!("Error handling connection: {}", e);
                }
            });
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
        handlers: HashMap<String, Arc<dyn McpHandler>>,
        _capabilities: ServerCapabilities,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            
            if bytes_read == 0 {
                break; // Connection closed
            }

            let response = match Self::process_request(&line, &handlers).await {
                Ok(resp) => resp,
                Err(e) => {
                    warn!("Error processing request: {}", e);
                    continue;
                }
            };

            let response_str = serde_json::to_string(&response)?;
            writer.write_all(response_str.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }

        Ok(())
    }

    async fn process_request(
        line: &str,
        handlers: &HashMap<String, Arc<dyn McpHandler>>,
    ) -> Result<JsonRpcResponse, McpError> {
        let request: JsonRpcRequest = serde_json::from_str(line.trim())?;
        
        let result = match request.method.as_str() {
            "initialize" => {
                if let Some(handler) = handlers.values().next() {
                    let params: InitializeParams = serde_json::from_value(
                        request.params.unwrap_or_default()
                    )?;
                    handler.initialize(params).await
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "tools/list" => {
                if let Some(handler) = handlers.values().next() {
                    let tools = handler.list_tools().await?;
                    Ok(serde_json::json!({ "tools": tools }))
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "tools/call" => {
                if let Some(handler) = handlers.values().next() {
                    let params: ToolCallParams = serde_json::from_value(
                        request.params.unwrap_or_default()
                    )?;
                    handler.call_tool(params).await
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "resources/list" => {
                if let Some(handler) = handlers.values().next() {
                    let resources = handler.list_resources().await?;
                    Ok(serde_json::json!({ "resources": resources }))
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "resources/read" => {
                if let Some(handler) = handlers.values().next() {
                    let params: ResourceReadParams = serde_json::from_value(
                        request.params.unwrap_or_default()
                    )?;
                    handler.read_resource(params).await
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            _ => Err(McpError::InvalidMethod(request.method.clone())),
        };

        match result {
            Ok(result) => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(result),
                error: None,
                id: request.id,
            }),
            Err(e) => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(e.into()),
                id: request.id,
            }),
        }
    }

    pub async fn run_stdio(&self) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncWriteExt, BufReader};
        
        info!("MCP Server running on stdio");
        
        let stdin = stdin();
        let mut stdout = stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;
            
            if bytes_read == 0 {
                break; // EOF
            }

            let response = match Self::process_request(&line, &self.handlers).await {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Error processing request: {}", e);
                    continue;
                }
            };

            let response_str = serde_json::to_string(&response)?;
            stdout.write_all(response_str.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }

        Ok(())
    }
}