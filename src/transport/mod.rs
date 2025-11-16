//! Transport abstraction layer for MCP-RS
//!
//! This module provides a unified interface for different types of
//! communication protocols including stdio, HTTP, and WebSocket.
//!
//! Each transport implements the Transport trait to provide consistent
//! message handling across different communication channels.

pub mod connection;
pub mod dynamic;
pub mod http;
pub mod stdio;
pub mod websocket;

use crate::{
    error::Result,
    types::{JsonRpcRequest, JsonRpcResponse},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub use connection::{ConnectionInfo, ConnectionStats};
pub use dynamic::{DynamicTransportManager, TransportSwitcher};
pub use http::{HttpConfig, HttpTransport};
pub use stdio::{StdioConfig, StdioTransport};
pub use websocket::{WebSocketConfig, WebSocketTransport};

/// Transport layer abstraction for MCP communication
#[async_trait]
pub trait Transport: Send + Sync + fmt::Debug {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start the transport and begin listening for connections
    async fn start(&mut self) -> std::result::Result<(), Self::Error>;

    /// Stop the transport and close all connections
    async fn stop(&mut self) -> std::result::Result<(), Self::Error>;

    /// Send a JSON-RPC response message
    async fn send_message(
        &mut self,
        message: JsonRpcResponse,
    ) -> std::result::Result<(), Self::Error>;

    /// Receive a JSON-RPC request message (non-blocking)
    async fn receive_message(&mut self)
        -> std::result::Result<Option<JsonRpcRequest>, Self::Error>;

    /// Check if the transport is currently connected/active
    fn is_connected(&self) -> bool;

    /// Get transport information and capabilities
    fn transport_info(&self) -> TransportInfo;

    /// Get current connection statistics
    fn connection_stats(&self) -> ConnectionStats;
}

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

// Use ConnectionStats from connection module

/// Transport-specific error types
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Message parsing error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Transport not supported: {0}")]
    NotSupported(String),

    #[error("Buffer overflow: message too large")]
    BufferOverflow,
}

impl From<crate::error::Error> for TransportError {
    fn from(err: crate::error::Error) -> Self {
        match err {
            crate::error::Error::Io(e) => TransportError::Io(e),
            crate::error::Error::Parse(e) => TransportError::Internal(e),
            crate::error::Error::Config(e) => TransportError::Configuration(e),
            crate::error::Error::Internal(e) => TransportError::Internal(e),
            crate::error::Error::NotSupported(e) => TransportError::NotSupported(e),
            _ => TransportError::Internal(format!("Converted error: {}", err)),
        }
    }
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

/// Transport factory for creating transport instances
pub struct TransportFactory;

impl TransportFactory {
    /// Create a transport instance based on configuration
    pub fn create_transport(
        config: &TransportConfig,
    ) -> std::result::Result<Box<dyn Transport<Error = TransportError>>, TransportError> {
        match &config.transport_type {
            TransportType::Stdio => {
                let stdio_transport = stdio::StdioTransport::new(config.stdio.clone())?;
                Ok(Box::new(stdio_transport))
            }
            TransportType::Http { addr } => {
                let http_config = http::HttpConfig {
                    bind_addr: *addr,
                    ..Default::default()
                };
                let http_transport = http::HttpTransport::new(http_config)
                    .map_err(|e| TransportError::Internal(e.to_string()))?;
                Ok(Box::new(http_transport))
            }
            TransportType::WebSocket { .. } => Err(TransportError::NotSupported(
                "WebSocket transport not yet implemented".to_string(),
            )),
        }
    }
}

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub transport_type: TransportType,
    pub stdio: stdio::StdioConfig,
    pub http: http::HttpConfig,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::Stdio,
            stdio: stdio::StdioConfig::default(),
            http: http::HttpConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_info_serialization() {
        let info = TransportInfo {
            transport_type: TransportType::Stdio,
            description: "Standard I/O transport".to_string(),
            capabilities: TransportCapabilities {
                bidirectional: true,
                multiplexing: false,
                compression: false,
                max_message_size: Some(1_048_576),
                framing_methods: vec![FramingMethod::ContentLength, FramingMethod::LineBased],
            },
        };

        let json = serde_json::to_string(&info).unwrap();
        let deserialized: TransportInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(format!("{:?}", info), format!("{:?}", deserialized));
    }

    #[test]
    fn test_transport_type_display() {
        assert_eq!(TransportType::Stdio.to_string(), "stdio");

        assert_eq!(
            TransportType::Http {
                addr: "127.0.0.1:8080".parse().unwrap()
            }
            .to_string(),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn test_connection_stats_default() {
        let stats = ConnectionStats::default();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.bytes_received, 0);
        // connection_errors フィールドは存在しないためコメントアウト
        // assert_eq!(stats.connection_errors, 0);
        assert!(stats.last_activity.is_none());
    }
}
