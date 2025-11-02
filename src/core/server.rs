use crate::config::McpConfig;
use crate::core::protocol::*;
use crate::core::transport::Transport;
use crate::plugins::PluginRegistry;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

pub struct McpServer {
    config: Arc<McpConfig>,
    plugin_registry: Arc<PluginRegistry>,
}

impl McpServer {
    pub fn new(config: Arc<McpConfig>, plugin_registry: Arc<PluginRegistry>) -> Self {
        Self {
            config,
            plugin_registry,
        }
    }

    pub async fn run(&self, transport: Transport) -> Result<()> {
        match transport {
            Transport::Stdio => self.run_stdio().await,
            Transport::Tcp { address, port } => self.run_tcp(&address, port).await,
        }
    }

    async fn run_stdio(&self) -> Result<()> {
        info!("Starting MCP server on STDIO");
        
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut stdout = io::stdout();

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    debug!("Received: {}", line.trim());
                    
                    if let Some(response) = self.handle_message(&line).await {
                        let response_str = serde_json::to_string(&response)?;
                        stdout.write_all(response_str.as_bytes()).await?;
                        stdout.write_all(b"\n").await?;
                        stdout.flush().await?;
                    }
                }
                Err(e) => {
                    error!("Error reading from stdin: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn run_tcp(&self, address: &str, port: u16) -> Result<()> {
        let bind_addr = format!("{}:{}", address, port);
        info!("Starting MCP server on TCP {}", bind_addr);

        let listener = TcpListener::bind(&bind_addr).await?;
        
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New connection from: {}", addr);
                    let server = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = server.handle_tcp_connection(stream).await {
                            error!("Error handling connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }
    }

    async fn handle_tcp_connection(&self, mut stream: TcpStream) -> Result<()> {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // Connection closed
                Ok(_) => {
                    debug!("Received: {}", line.trim());
                    
                    if let Some(response) = self.handle_message(&line).await {
                        let response_str = serde_json::to_string(&response)?;
                        writer.write_all(response_str.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                    }
                }
                Err(e) => {
                    error!("Error reading from connection: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_message(&self, message: &str) -> Option<Value> {
        let request: JsonRpcRequest = match serde_json::from_str(message.trim()) {
            Ok(req) => req,
            Err(e) => {
                warn!("Failed to parse JSON-RPC request: {}", e);
                return Some(json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32700,
                        "message": "Parse error"
                    }
                }));
            }
        };

        match request.method.as_str() {
            "initialize" => self.handle_initialize(&request).await,
            "tools/list" => self.handle_tools_list(&request).await,
            "tools/call" => self.handle_tools_call(&request).await,
            "resources/list" => self.handle_resources_list(&request).await,
            "resources/read" => self.handle_resources_read(&request).await,
            "prompts/list" => self.handle_prompts_list(&request).await,
            "prompts/get" => self.handle_prompts_get(&request).await,
            _ => Some(json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {
                    "code": -32601,
                    "message": "Method not found"
                }
            }))
        }
    }

    async fn handle_initialize(&self, request: &JsonRpcRequest) -> Option<Value> {
        info!("Client initialized with MCP server");
        
        Some(json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {},
                    "resources": {},
                    "prompts": {}
                },
                "serverInfo": {
                    "name": "mcp-rs",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }))
    }

    async fn handle_tools_list(&self, request: &JsonRpcRequest) -> Option<Value> {
        let tools = self.plugin_registry.list_tools().await;
        
        Some(json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {
                "tools": tools
            }
        }))
    }

    async fn handle_tools_call(&self, request: &JsonRpcRequest) -> Option<Value> {
        let params = request.params.as_ref()?;
        let tool_name = params.get("name")?.as_str()?;
        let arguments = params.get("arguments")?;

        match self.plugin_registry.call_tool(tool_name, arguments).await {
            Ok(result) => Some(json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": result
                        }
                    ]
                }
            })),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {
                    "code": -32603,
                    "message": e.to_string()
                }
            }))
        }
    }

    async fn handle_resources_list(&self, request: &JsonRpcRequest) -> Option<Value> {
        let resources = self.plugin_registry.list_resources().await;
        
        Some(json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {
                "resources": resources
            }
        }))
    }

    async fn handle_resources_read(&self, request: &JsonRpcRequest) -> Option<Value> {
        let params = request.params.as_ref()?;
        let uri = params.get("uri")?.as_str()?;

        match self.plugin_registry.read_resource(uri).await {
            Ok(content) => Some(json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": {
                    "contents": [
                        {
                            "uri": uri,
                            "mimeType": "text/plain",
                            "text": content
                        }
                    ]
                }
            })),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {
                    "code": -32603,
                    "message": e.to_string()
                }
            }))
        }
    }

    async fn handle_prompts_list(&self, request: &JsonRpcRequest) -> Option<Value> {
        let prompts = self.plugin_registry.list_prompts().await;
        
        Some(json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {
                "prompts": prompts
            }
        }))
    }

    async fn handle_prompts_get(&self, request: &JsonRpcRequest) -> Option<Value> {
        let params = request.params.as_ref()?;
        let name = params.get("name")?.as_str()?;
        let arguments = params.get("arguments");

        match self.plugin_registry.get_prompt(name, arguments).await {
            Ok(messages) => Some(json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "result": {
                    "messages": messages
                }
            })),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": request.id,
                "error": {
                    "code": -32603,
                    "message": e.to_string()
                }
            }))
        }
    }
}

impl Clone for McpServer {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            plugin_registry: Arc::clone(&self.plugin_registry),
        }
    }
}