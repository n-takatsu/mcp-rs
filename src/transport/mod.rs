//! Transport abstraction layer for MCP communication.
//!
//! This module provides a pluggable transport system that supports multiple
//! communication protocols including stdio, HTTP, and WebSocket.

use crate::{
    error::Result,
    types::{JsonRpcRequest, JsonRpcResponse},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub mod stdio;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Connection statistics
#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub connection_errors: u64,
    pub last_activity: Option<std::time::Instant>,
}

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

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Transport not supported: {0}")]
    NotSupported(String),

    #[error("Buffer overflow: message too large")]
    BufferOverflow,
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
    ) -> Result<Box<dyn Transport<Error = TransportError>>> {
        match &config.transport_type {
            TransportType::Stdio => {
                let stdio_transport = stdio::StdioTransport::new(config.stdio.clone())?;
                Ok(Box::new(stdio_transport))
            }
            TransportType::Http { .. } => Err(crate::error::Error::NotSupported(
                "HTTP transport not yet implemented".to_string(),
            )),
            TransportType::WebSocket { .. } => Err(crate::error::Error::NotSupported(
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
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::Stdio,
            stdio: stdio::StdioConfig::default(),
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
        assert_eq!(stats.connection_errors, 0);
        assert!(stats.last_activity.is_none());
    }
}
