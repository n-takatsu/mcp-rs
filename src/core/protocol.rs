use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// MCP Error types
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum McpError {
    #[error("Protocol error: {message}")]
    Protocol { message: String },
    
    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },
    
    #[error("Method not found: {method}")]
    MethodNotFound { method: String },
    
    #[error("Invalid parameters: {message}")]
    InvalidParams { message: String },
    
    #[error("Internal error: {message}")]
    Internal { message: String },
    
    #[error("Plugin error: {plugin}: {message}")]
    Plugin { plugin: String, message: String },
    
    #[error("Configuration error: {message}")]
    Config { message: String },
    
    #[error("Transport error: {message}")]
    Transport { message: String },
    
    #[error("Tool not found: {name}")]
    ToolNotFound { name: String },
    
    #[error("Resource not found: {uri}")]
    ResourceNotFound { uri: String },
    
    #[error("External API error: {message}")]
    ExternalApi { message: String },
    
    #[error("Serialization error: {message}")]
    Serialization { message: String },
    
    #[error("HTTP error: {message}")]
    Http { message: String },
    
    #[error("IO error: {message}")]
    Io { message: String },
    
    #[error("Other error: {message}")]
    Other { message: String },
}

impl McpError {
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol { message: message.into() }
    }
    
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest { message: message.into() }
    }
    
    pub fn method_not_found(method: impl Into<String>) -> Self {
        Self::MethodNotFound { method: method.into() }
    }
    
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::InvalidParams { message: message.into() }
    }
    
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal { message: message.into() }
    }
    
    pub fn plugin(plugin: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Plugin { plugin: plugin.into(), message: message.into() }
    }
    
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config { message: message.into() }
    }
    
    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport { message: message.into() }
    }
    
    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::ToolNotFound { name: name.into() }
    }
    
    pub fn resource_not_found(uri: impl Into<String>) -> Self {
        Self::ResourceNotFound { uri: uri.into() }
    }
    
    pub fn external_api(message: impl Into<String>) -> Self {
        Self::ExternalApi { message: message.into() }
    }
    
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other { message: message.into() }
    }
    
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization { message: message.into() }
    }
    
    pub fn http(message: impl Into<String>) -> Self {
        Self::Http { message: message.into() }
    }
}

// Convert from external error types
impl From<serde_json::Error> for McpError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization { message: err.to_string() }
    }
}

impl From<reqwest::Error> for McpError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http { message: err.to_string() }
    }
}

impl From<std::io::Error> for McpError {
    fn from(err: std::io::Error) -> Self {
        Self::Io { message: err.to_string() }
    }
}

impl From<anyhow::Error> for McpError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other { message: err.to_string() }
    }
}

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
    pub id: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// MCP Protocol version
pub const MCP_VERSION: &str = "2024-11-05";

/// MCP Initialize parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

/// Client capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    pub experimental: Option<HashMap<String, serde_json::Value>>,
    pub sampling: Option<SamplingCapability>,
}

/// Sampling capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingCapability {}

/// Client information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Initialize result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// Server capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub experimental: Option<HashMap<String, serde_json::Value>>,
    pub logging: Option<LoggingCapability>,
    pub prompts: Option<PromptsCapability>,
    pub resources: Option<ResourcesCapability>,
    pub tools: Option<ToolsCapability>,
}

/// Logging capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingCapability {}

/// Prompts capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Resources capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    pub subscribe: Option<bool>,
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Tools capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Tool call parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: Option<HashMap<String, serde_json::Value>>,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<Content>,
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

/// Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Resource read parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReadParams {
    pub uri: String,
}

/// Resource read result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReadResult {
    pub contents: Vec<ResourceContent>,
}

/// Resource content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub text: Option<String>,
    pub blob: Option<String>, // Base64 encoded
}

/// Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    pub description: String,
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Prompt argument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    pub description: String,
    pub required: Option<bool>,
}

/// Prompt get parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptGetParams {
    pub name: String,
    pub arguments: Option<HashMap<String, serde_json::Value>>,
}

/// Prompt get result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptGetResult {
    pub description: Option<String>,
    pub messages: Vec<PromptMessage>,
}

/// Prompt message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    pub role: MessageRole,
    pub content: Content,
}

/// Message role
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    
    #[serde(rename = "image")]
    Image { 
        data: String, // Base64 encoded
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    
    #[serde(rename = "resource")]
    Resource {
        resource: EmbeddedResource,
    },
}

/// Embedded resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedResource {
    #[serde(rename = "type")]
    pub resource_type: String,
    pub resource: Resource,
}

/// Progress notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNotification {
    #[serde(rename = "progressToken")]
    pub progress_token: Option<serde_json::Value>,
    pub progress: f64,
    pub total: Option<f64>,
}

/// Log level
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

/// Logging message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingMessage {
    pub level: LogLevel,
    pub data: serde_json::Value,
    pub logger: Option<String>,
}