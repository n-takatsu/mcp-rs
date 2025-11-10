//! HTTP Transport implementation for MCP-RS
//!
//! This module provides HTTP-based JSON-RPC transport for MCP communication.

use crate::{
    error::{Error, Result},
    transport::{
        ConnectionStats, Transport, TransportCapabilities, TransportError, TransportInfo,
        TransportType,
    },
    types::{JsonRpcRequest, JsonRpcResponse},
};
use async_trait::async_trait;
use axum::{extract::State, http::StatusCode, response::Json, routing::post, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info};

/// HTTP Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Bind address for HTTP server
    pub bind_addr: SocketAddr,
    /// Enable CORS
    pub cors_enabled: bool,
    /// Maximum request size in bytes
    pub max_request_size: usize,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8081".parse().unwrap(),
            cors_enabled: true,
            max_request_size: 1024 * 1024, // 1MB
            timeout_ms: 30000,             // 30 seconds
        }
    }
}

/// HTTP Transport implementation
#[derive(Debug)]
pub struct HttpTransport {
    config: HttpConfig,
    receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<String>>>,
    sender: tokio::sync::mpsc::Sender<String>,
}

impl HttpTransport {
    /// Create a new HTTP transport
    pub fn new(config: HttpConfig) -> Result<Self> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

        Ok(Self {
            config,
            receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
            sender,
        })
    }

    /// Start the HTTP server
    pub async fn start_server(&self) -> Result<()> {
        let app = Router::new()
            .route("/", post(handle_jsonrpc_request))
            .route("/mcp", post(handle_jsonrpc_request))
            .layer(if self.config.cors_enabled {
                CorsLayer::permissive()
            } else {
                CorsLayer::new()
            })
            .with_state(self.sender.clone());

        info!(
            "Starting HTTP transport server on {}",
            self.config.bind_addr
        );

        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| Error::Internal(format!("Failed to bind HTTP server: {}", e)))?;

        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                error!("HTTP server error: {}", e);
            }
        });

        Ok(())
    }
}

#[async_trait]
impl Transport for HttpTransport {
    type Error = TransportError;

    async fn start(&mut self) -> std::result::Result<(), Self::Error> {
        debug!("Starting HTTP transport server");
        self.start_server()
            .await
            .map_err(|e| TransportError::Internal(e.to_string()))?;
        Ok(())
    }

    async fn stop(&mut self) -> std::result::Result<(), Self::Error> {
        debug!("Stopping HTTP transport server");
        // HTTP server stops automatically when dropped
        Ok(())
    }

    async fn send_message(
        &mut self,
        message: JsonRpcResponse,
    ) -> std::result::Result<(), Self::Error> {
        let message_str = serde_json::to_string(&message).map_err(|e| {
            TransportError::Internal(format!("Failed to serialize response: {}", e))
        })?;

        debug!("HTTP transport sending message: {}", message_str);

        self.sender
            .send(message_str)
            .await
            .map_err(|e| TransportError::Internal(format!("HTTP send failed: {}", e)))?;

        Ok(())
    }

    async fn receive_message(
        &mut self,
    ) -> std::result::Result<Option<JsonRpcRequest>, Self::Error> {
        let mut receiver = self.receiver.lock().await;

        match receiver.recv().await {
            Some(message_str) => {
                debug!("HTTP transport received message: {}", message_str);
                let request: JsonRpcRequest = serde_json::from_str(&message_str).map_err(|e| {
                    TransportError::Internal(format!("Failed to parse request: {}", e))
                })?;
                Ok(Some(request))
            }
            None => Ok(None), // Channel closed, no more messages
        }
    }

    fn is_connected(&self) -> bool {
        // HTTP server is always "connected" when running
        true
    }

    fn transport_info(&self) -> TransportInfo {
        TransportInfo {
            transport_type: TransportType::Http {
                addr: self.config.bind_addr,
            },
            description: "HTTP JSON-RPC transport for MCP communication".to_string(),
            capabilities: TransportCapabilities {
                bidirectional: true,
                multiplexing: true,
                compression: false,
                max_message_size: Some(self.config.max_request_size),
                framing_methods: vec![],
            },
        }
    }

    fn connection_stats(&self) -> ConnectionStats {
        // TODO: Implement proper statistics tracking
        ConnectionStats::default()
    }
}

/// Handle incoming JSON-RPC HTTP requests
async fn handle_jsonrpc_request(
    State(sender): State<tokio::sync::mpsc::Sender<String>>,
    Json(request): Json<Value>,
) -> std::result::Result<Json<Value>, StatusCode> {
    debug!("Received HTTP JSON-RPC request: {}", request);

    // Convert request to string and send through channel
    let request_str = serde_json::to_string(&request).map_err(|_| StatusCode::BAD_REQUEST)?;

    sender
        .send(request_str)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // TODO: Implement proper request-response correlation mechanism
    // For now, return a simple accepted status
    // In a full implementation, we would:
    // 1. Create a response channel for this specific request ID
    // 2. Wait for the actual response from the MCP handler
    // 3. Return the real response data

    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "result": {
            "status": "accepted",
            "message": "Request received and queued for processing"
        },
        "id": request.get("id")
    });

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_config_default() {
        let config = HttpConfig::default();
        assert_eq!(config.bind_addr.port(), 8081);
        assert!(config.cors_enabled);
        assert_eq!(config.max_request_size, 1024 * 1024);
        assert_eq!(config.timeout_ms, 30000);
    }

    #[tokio::test]
    async fn test_http_transport_creation() {
        let config = HttpConfig::default();
        let transport = HttpTransport::new(config);
        assert!(transport.is_ok());
    }
}
