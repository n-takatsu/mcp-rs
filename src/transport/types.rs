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
            TransportType::WebSocket { url } => write!(f, "ws://{}", url),
        }
    }
}
