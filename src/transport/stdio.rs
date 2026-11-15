//! Standard I/O transport settings.
//!
//! The server reads and writes STDIO itself (`McpServer::run_stdio`). The
//! `StdioTransport` that used to be here was not used by it, and was
//! removed (#543).

use super::FramingMethod;
use serde::{Deserialize, Serialize};

/// Standard I/O transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioConfig {
    /// Buffer size for I/O operations
    pub buffer_size: usize,
    /// Operation timeout in milliseconds
    pub timeout_ms: u64,
    /// Enable Content-Length header framing
    pub content_length_header: bool,
    /// Message framing method
    pub framing_method: FramingMethod,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Enable JSON pretty printing for outgoing messages
    pub pretty_print: bool,
}

impl Default for StdioConfig {
    fn default() -> Self {
        Self {
            buffer_size: 8192,
            timeout_ms: 30_000,
            content_length_header: true,
            framing_method: FramingMethod::ContentLength,
            max_message_size: 1_048_576, // 1MB
            pretty_print: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_config_default() {
        let config = StdioConfig::default();
        assert_eq!(config.buffer_size, 8192);
        assert_eq!(config.timeout_ms, 30_000);
        assert!(config.content_length_header);
        assert_eq!(config.max_message_size, 1_048_576);
        assert!(!config.pretty_print);
    }
}
