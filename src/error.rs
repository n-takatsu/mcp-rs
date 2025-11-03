//! Error types for the MCP protocol implementation.

use thiserror::Error;

/// Result type alias for MCP operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for MCP protocol operations
#[derive(Debug, Error)]
pub enum Error {
    /// Invalid JSON-RPC request
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Method not found
    #[error("Method not found: {0}")]
    MethodNotFound(String),

    /// Invalid parameters
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    /// Internal server error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Parse error
    #[error("Parse error: {0}")]
    Parse(String),

    /// Network error
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON serialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Feature not supported
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// Transport error
    #[error("Transport error: {0}")]
    TransportError(#[from] crate::transport::TransportError),

    /// Security error
    #[error("Security error: {0}")]
    Security(#[from] SecurityError),
}

impl Error {
    /// Convert error to JSON-RPC error code
    pub fn to_json_rpc_code(&self) -> i32 {
        match self {
            Error::Parse(_) => -32700,
            Error::InvalidRequest(_) => -32600,
            Error::MethodNotFound(_) => -32601,
            Error::InvalidParams(_) => -32602,
            Error::Internal(_) => -32603,
            Error::Security(_) => -32000, // Security related server error
            _ => -32000, // Server error
        }
    }
}

/// Security-related errors
#[derive(Debug, Error)]
pub enum SecurityError {
    /// Encryption/decryption error
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Rate limiting error
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// TLS configuration error
    #[error("TLS error: {0}")]
    TlsError(String),

    /// Input validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    /// Authorization error
    #[error("Authorization error: {0}")]
    AuthorizationError(String),

    /// Security policy violation
    #[error("Security policy violation: {0}")]
    PolicyViolation(String),
}
