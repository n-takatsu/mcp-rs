//! Error types for the MCP protocol implementation.

use thiserror::Error;

/// Result type alias for MCP operations
pub type Result<T> = std::result::Result<T, Error>;

/// MCP Error type alias for plugin isolation system
pub type McpError = Error;

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

    /// Config library error
    #[error("Config error: {0}")]
    ConfigError(#[from] config::ConfigError),

    /// Generic boxed error
    #[error("Error: {0}")]
    Boxed(#[from] Box<dyn std::error::Error + Send + Sync>),

    /// Feature not supported
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Invalid configuration error
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    /// Transport error
    #[error("Transport error: {0}")]
    TransportError(#[from] crate::transport::TransportError),

    /// Security error (from SecurityError enum)
    #[error("Security error: {0}")]
    Security(#[from] SecurityError),

    /// Security error (direct)
    #[error("Security error: {0}")]
    SecurityFailure(String),

    /// Plugin error
    #[error("Plugin error: {0}")]
    Plugin(String),

    /// Isolation error
    #[error("Isolation error: {0}")]
    Isolation(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Canary deployment error
    #[error("Canary deployment error: {0}")]
    CanaryDeployment(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Metrics collection error
    #[error("Metrics error: {0}")]
    Metrics(String),

    /// Traffic splitting error
    #[error("Traffic splitting error: {0}")]
    TrafficSplitting(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Not found error
    #[error("Not found: {0}")]
    NotFound(String),

    /// Operation error
    #[error("Operation error: {0}")]
    Operation(String),

    /// Already exists error
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// Parse error (generic)
    #[error("Parse error: {0}")]
    #[allow(clippy::enum_variant_names)]
    ParseError(String),

    /// Stream error
    #[error("Stream error: {0}")]
    #[allow(clippy::enum_variant_names)]
    StreamError(String),

    /// Connection error
    #[error("Connection error: {0}")]
    #[allow(clippy::enum_variant_names)]
    ConnectionError(String),

    /// Invalid message error
    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    /// Timeout error
    #[error("Timeout")]
    Timeout,

    /// Server error
    #[error("Server error: {0}")]
    Server(String),

    /// Connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// Configuration error
    #[error("Configuration: {0}")]
    Configuration(String),

    /// Compression error
    #[error("Compression error: {0}")]
    Compression(String),
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
            _ => -32000,                  // Server error
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

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Session management error
    #[error("Session error: {0}")]
    Session(#[from] SessionError),
}

/// Session-specific error types
#[derive(Debug, Error)]
pub enum SessionError {
    /// Session not found
    #[error("Session not found: {0}")]
    NotFound(String),

    /// Session expired
    #[error("Session expired: {0}")]
    Expired(String),

    /// Invalid session state
    #[error("Invalid session state: {0}")]
    InvalidState(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Security violation
    #[error("Security violation: {0}")]
    SecurityViolation(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}
