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
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Instant};
use tokio::{net::TcpListener, sync::RwLock};
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

/// Statistics for HTTP transport
#[derive(Debug, Clone, Default)]
struct HttpStats {
    total_requests: u64,
    total_responses: u64,
    total_errors: u64,
    total_bytes_sent: u64,
    total_bytes_received: u64,
    total_response_time_ms: u64,
    started_at: Option<Instant>,
}

/// Shared state for HTTP transport
#[derive(Clone)]
struct HttpTransportState {
    request_sender: tokio::sync::mpsc::Sender<String>,
    pending_responses: Arc<RwLock<HashMap<Value, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stats: Arc<RwLock<HttpStats>>,
}

/// HTTP Transport implementation
#[derive(Debug)]
pub struct HttpTransport {
    config: HttpConfig,
    receiver: Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<String>>>,
    sender: tokio::sync::mpsc::Sender<String>,
    pending_responses: Arc<RwLock<HashMap<Value, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
    stats: Arc<RwLock<HttpStats>>,
}

impl HttpTransport {
    /// Create a new HTTP transport
    pub fn new(config: HttpConfig) -> Result<Self> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1000);
        
        let stats = HttpStats {
            started_at: Some(Instant::now()),
            ..Default::default()
        };

        Ok(Self {
            config,
            receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
            sender,
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(stats)),
        })
    }

    /// Start the HTTP server
    pub async fn start_server(&self) -> Result<()> {
        let state = HttpTransportState {
            request_sender: self.sender.clone(),
            pending_responses: self.pending_responses.clone(),
            stats: self.stats.clone(),
        };
        
        let app = Router::new()
            .route("/", post(handle_jsonrpc_request))
            .route("/mcp", post(handle_jsonrpc_request))
            .layer(if self.config.cors_enabled {
                CorsLayer::permissive()
            } else {
                CorsLayer::new()
            })
            .with_state(state);

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

        // Check if this is a response to a pending request
        let id_value = serde_json::to_value(&message.id).ok();
        if let Some(id) = id_value {
            let mut pending = self.pending_responses.write().await;
            if let Some(response_tx) = pending.remove(&id) {
                // Send response through oneshot channel
                if response_tx.send(message).is_err() {
                    error!("Failed to send response - receiver dropped for ID: {:?}", id);
                }
                return Ok(());
            }
        }

        // Fallback: send through regular channel for notifications
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
        let stats = self.stats.blocking_read();
        
        let uptime = stats.started_at
            .map(|start| start.elapsed())
            .unwrap_or(std::time::Duration::from_secs(0));
        
        ConnectionStats {
            messages_sent: stats.total_responses,
            messages_received: stats.total_requests,
            bytes_sent: stats.total_bytes_sent,
            bytes_received: stats.total_bytes_received,
            uptime,
            last_activity: None, // TODO: Track last activity time
        }
    }
}

/// Handle incoming JSON-RPC HTTP requests
async fn handle_jsonrpc_request(
    State(state): State<HttpTransportState>,
    Json(request): Json<Value>,
) -> std::result::Result<Json<Value>, StatusCode> {
    let start_time = Instant::now();
    
    debug!("Received HTTP JSON-RPC request: {}", request);

    // Update statistics - request received
    {
        let mut stats = state.stats.write().await;
        stats.total_requests += 1;
        let request_size = serde_json::to_string(&request).unwrap_or_default().len() as u64;
        stats.total_bytes_received += request_size;
    }

    // Extract request ID for response correlation
    let request_id = request.get("id").cloned();

    // Convert request to string and send through channel
    let request_str = serde_json::to_string(&request).map_err(|_| {
        // Update error stats
        let mut stats = state.stats.blocking_write();
        stats.total_errors += 1;
        StatusCode::BAD_REQUEST
    })?;

    state
        .request_sender
        .send(request_str)
        .await
        .map_err(|_| {
            // Update error stats
            let mut stats = state.stats.blocking_write();
            stats.total_errors += 1;
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // If request has ID, wait for actual response
    if let Some(id) = request_id {
        // Create oneshot channel for response
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        // Register pending response
        {
            let mut pending = state.pending_responses.write().await;
            pending.insert(id.clone(), response_tx);
        }

        // Wait for response with timeout
        let result = match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            response_rx
        ).await {
            Ok(Ok(response)) => {
                // Update response stats
                let elapsed = start_time.elapsed().as_millis() as u64;
                {
                    let mut stats = state.stats.write().await;
                    stats.total_responses += 1;
                    stats.total_response_time_ms += elapsed;
                }
                
                // Convert JsonRpcResponse to JSON Value
                let response_value = serde_json::to_value(&response)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                let response_size = serde_json::to_string(&response_value).unwrap_or_default().len() as u64;
                
                {
                    let mut stats = state.stats.write().await;
                    stats.total_bytes_sent += response_size;
                }
                
                Ok(Json(response_value))
            }
            Ok(Err(_)) => {
                // Channel closed without response
                let mut stats = state.stats.write().await;
                stats.total_errors += 1;
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
            Err(_) => {
                // Timeout
                let mut pending = state.pending_responses.write().await;
                pending.remove(&id);
                let mut stats = state.stats.write().await;
                stats.total_errors += 1;
                Err(StatusCode::REQUEST_TIMEOUT)
            }
        };
        
        result
    } else {
        // Notification (no ID) - return immediate acknowledgment
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "status": "accepted",
                "message": "Notification received"
            }
        });
        
        let response_size = serde_json::to_string(&response).unwrap_or_default().len() as u64;
        {
            let mut stats = state.stats.write().await;
            stats.total_responses += 1;
            stats.total_bytes_sent += response_size;
        }
        
        Ok(Json(response))
    }
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
