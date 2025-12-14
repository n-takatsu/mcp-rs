//! Transport Factory
//!
//! Factory for creating transport instances

use super::config::TransportConfig;
use super::error::TransportError;
use super::http;
use super::stdio;
use super::transport_trait::Transport;
use super::types::TransportType;

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
