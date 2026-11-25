//! Transport Types
//!
//! Common types for transport implementations

use serde::{Deserialize, Serialize};
use std::fmt;

/// Information about a transport implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportInfo {
    pub transport_type: TransportType,
    pub description: String,
    pub capabilities: TransportCapabilities,
}

/// Types of supported transports
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransportType {
    /// Standard input/output transport
    Stdio,
    /// HTTP server transport
    Http { addr: std::net::SocketAddr },
    /// WebSocket transport
    WebSocket { url: String },
}

/// Transport-specific capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportCapabilities {
    /// Supports bidirectional communication
    pub bidirectional: bool,
    /// Supports connection multiplexing
    pub multiplexing: bool,
    /// Supports message compression
    pub compression: bool,
    /// Maximum message size (bytes)
    pub max_message_size: Option<usize>,
    /// Supported framing methods
    pub framing_methods: Vec<FramingMethod>,
}

/// Message framing methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FramingMethod {
    /// Content-Length header followed by JSON
    ContentLength,
    /// Line-based JSON messages
    LineBased,
    /// WebSocket frame-based
    WebSocketFrame,
}

impl fmt::Display for TransportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportType::Stdio => write!(f, "stdio"),
            TransportType::Http { addr } => write!(f, "http://{}", addr),
            // URLにスキームがあれば、そのまま出す（#760。以前は必ず`ws://`を
            // 付け、`ws://wss://...`になっていた）
            TransportType::WebSocket { url } => {
                if url.starts_with("ws://") || url.starts_with("wss://") {
                    write!(f, "{}", url)
                } else {
                    write!(f, "ws://{}", url)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WebSocketのURLのスキームを重ねない（#760）
    #[test]
    fn test_websocket_display_keeps_the_scheme() {
        let display = |url: &str| {
            TransportType::WebSocket {
                url: url.to_string(),
            }
            .to_string()
        };
        assert_eq!(display("wss://example.com/mcp"), "wss://example.com/mcp");
        assert_eq!(display("ws://example.com/mcp"), "ws://example.com/mcp");
        assert_eq!(display("example.com/mcp"), "ws://example.com/mcp");
    }
}
