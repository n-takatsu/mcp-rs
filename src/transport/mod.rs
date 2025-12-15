//! Transport abstraction layer for MCP-RS
//!
//! This module provides a unified interface for different types of
//! communication protocols including stdio, HTTP, and WebSocket.
//!
//! Each transport implements the Transport trait to provide consistent
//! message handling across different communication channels.

pub mod config;
pub mod connection;
pub mod dynamic;
pub mod error;
pub mod factory;
pub mod http;
pub mod stdio;
pub mod transport_trait;
pub mod types;
pub mod websocket;

pub use config::TransportConfig;
pub use connection::{ConnectionInfo, ConnectionStats};
pub use dynamic::{DynamicTransportManager, TransportSwitcher};
pub use error::TransportError;
pub use factory::TransportFactory;
pub use http::{HttpConfig, HttpTransport};
pub use stdio::{StdioConfig, StdioTransport};
pub use transport_trait::Transport;
pub use types::{FramingMethod, TransportCapabilities, TransportInfo, TransportType};
pub use websocket::{WebSocketConfig, WebSocketTransport};

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
        assert!(stats.last_activity.is_none());
    }
}
