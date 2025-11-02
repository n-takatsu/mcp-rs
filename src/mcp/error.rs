use crate::mcp::JsonRpcError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),

    #[error("Invalid method: {0}")]
    InvalidMethod(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("External API error: {0}")]
    ExternalApi(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<McpError> for crate::mcp::JsonRpcError {
    fn from(err: McpError) -> Self {
        match err {
            McpError::JsonRpc(msg) => JsonRpcError {
                code: -32603,
                message: msg,
                data: None,
            },
            McpError::InvalidMethod(msg) => JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", msg),
                data: None,
            },
            McpError::InvalidParams(msg) => JsonRpcError {
                code: -32602,
                message: format!("Invalid params: {}", msg),
                data: None,
            },
            _ => JsonRpcError {
                code: -32603,
                message: err.to_string(),
                data: None,
            },
        }
    }
}
