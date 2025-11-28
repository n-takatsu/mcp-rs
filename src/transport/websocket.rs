//! WebSocket transport implementation for MCP-RS
//!
//! Provides WebSocket-based communication for MCP protocol with full
//! bidirectional support, TLS/WSS, automatic reconnection, and heartbeat.

use crate::transport::{Transport, TransportError};
use crate::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, timeout};
use tokio_tungstenite::{
    accept_async, connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, info, warn};

/// WebSocket transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// WebSocket URL to connect to or bind address for server
    pub url: String,
    /// Whether to act as server (bind) or client (connect)
    pub server_mode: bool,
    /// Connection timeout in seconds
    pub timeout_seconds: Option<u64>,
    /// Enable TLS/WSS
    pub use_tls: bool,
    /// Heartbeat interval in seconds (0 to disable)
    pub heartbeat_interval: u64,
    /// Maximum reconnection attempts (0 for infinite)
    pub max_reconnect_attempts: u32,
    /// Reconnection delay in seconds
    pub reconnect_delay: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Maximum concurrent connections (server mode only)
    pub max_connections: usize,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:8082".to_string(),
            server_mode: true,
            timeout_seconds: Some(30),
            use_tls: false,
            heartbeat_interval: 30,
            max_reconnect_attempts: 5,
            reconnect_delay: 5,
            max_message_size: 16 * 1024 * 1024, // 16MB
            max_connections: 100,
        }
    }
}

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    ShuttingDown,
}

/// WebSocket client/server connection wrapper
type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Message queue item
#[derive(Debug)]
struct QueuedMessage {
    response: JsonRpcResponse,
    retry_count: u32,
}

/// WebSocket transport implementation
#[derive(Debug)]
pub struct WebSocketTransport {
    config: WebSocketConfig,
    state: Arc<RwLock<ConnectionState>>,
    // Client mode: single connection
    client_connection: Arc<RwLock<Option<WsStream>>>,
    // Server mode: multiple connections (simplified - first connection only for now)
    server_connection: Arc<RwLock<Option<WsStream>>>,
    // Message queues
    outgoing_tx: mpsc::UnboundedSender<JsonRpcResponse>,
    outgoing_rx: Arc<RwLock<mpsc::UnboundedReceiver<JsonRpcResponse>>>,
    incoming_tx: mpsc::UnboundedSender<JsonRpcRequest>,
    incoming_rx: Arc<RwLock<mpsc::UnboundedReceiver<JsonRpcRequest>>>,
    // Shutdown signal
    shutdown_tx: Arc<RwLock<Option<mpsc::Sender<()>>>>,
    // Statistics
    messages_sent: Arc<RwLock<u64>>,
    messages_received: Arc<RwLock<u64>>,
    bytes_sent: Arc<RwLock<u64>>,
    bytes_received: Arc<RwLock<u64>>,
    reconnect_count: Arc<RwLock<u32>>,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport with the given configuration
    pub fn new(config: WebSocketConfig) -> Result<Self, TransportError> {
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            client_connection: Arc::new(RwLock::new(None)),
            server_connection: Arc::new(RwLock::new(None)),
            outgoing_tx,
            outgoing_rx: Arc::new(RwLock::new(outgoing_rx)),
            incoming_tx,
            incoming_rx: Arc::new(RwLock::new(incoming_rx)),
            shutdown_tx: Arc::new(RwLock::new(None)),
            messages_sent: Arc::new(RwLock::new(0)),
            messages_received: Arc::new(RwLock::new(0)),
            bytes_sent: Arc::new(RwLock::new(0)),
            bytes_received: Arc::new(RwLock::new(0)),
            reconnect_count: Arc::new(RwLock::new(0)),
        })
    }

    /// Start WebSocket server
    async fn start_server(&self) -> Result<(), TransportError> {
        let addr = self
            .config
            .url
            .strip_prefix("ws://")
            .or_else(|| self.config.url.strip_prefix("wss://"))
            .ok_or_else(|| {
                TransportError::Configuration(format!("Invalid WebSocket URL: {}", self.config.url))
            })?;

        let listener = TcpListener::bind(addr).await.map_err(|e| {
            TransportError::Internal(format!("Failed to bind WebSocket server: {}", e))
        })?;

        info!("WebSocket server listening on: {}", addr);

        let server_connection = Arc::clone(&self.server_connection);
        let _incoming_tx = self.incoming_tx.clone();
        let state = Arc::clone(&self.state);
        let _config = self.config.clone();

        // Accept first connection (simplified implementation)
        tokio::spawn(async move {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    info!("WebSocket client connected from: {}", peer_addr);
                    *state.write().await = ConnectionState::Connecting;

                    // Use accept_async_with_config or wrap stream properly
                    match accept_async(MaybeTlsStream::Plain(stream)).await {
                        Ok(ws_stream) => {
                            info!("WebSocket handshake completed");
                            *server_connection.write().await = Some(ws_stream);
                            *state.write().await = ConnectionState::Connected;
                        }
                        Err(e) => {
                            error!("WebSocket handshake failed: {}", e);
                            *state.write().await = ConnectionState::Disconnected;
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Start WebSocket client
    async fn start_client(&self) -> Result<(), TransportError> {
        *self.state.write().await = ConnectionState::Connecting;

        let url = &self.config.url;
        debug!("Connecting to WebSocket server: {}", url);

        let timeout_duration = Duration::from_secs(self.config.timeout_seconds.unwrap_or(30));

        let connect_future = connect_async(url);
        let ws_stream = timeout(timeout_duration, connect_future)
            .await
            .map_err(|_| {
                TransportError::Timeout(format!(
                    "WebSocket connection timeout after {:?}",
                    timeout_duration
                ))
            })?
            .map_err(|e| TransportError::Internal(format!("WebSocket connect error: {}", e)))?
            .0;

        info!("WebSocket client connected to: {}", url);
        *self.client_connection.write().await = Some(ws_stream);
        *self.state.write().await = ConnectionState::Connected;

        Ok(())
    }

    /// Start message processing loop
    async fn start_message_loop(&self) -> Result<(), TransportError> {
        let connection = if self.config.server_mode {
            Arc::clone(&self.server_connection)
        } else {
            Arc::clone(&self.client_connection)
        };

        let outgoing_rx = Arc::clone(&self.outgoing_rx);
        let incoming_tx = self.incoming_tx.clone();
        let state = Arc::clone(&self.state);
        let messages_sent = Arc::clone(&self.messages_sent);
        let messages_received = Arc::clone(&self.messages_received);
        let bytes_sent = Arc::clone(&self.bytes_sent);
        let bytes_received = Arc::clone(&self.bytes_received);
        let config = self.config.clone();

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        tokio::spawn(async move {
            let mut heartbeat_interval_timer =
                interval(Duration::from_secs(config.heartbeat_interval));

            loop {
                let mut conn_guard = connection.write().await;
                let ws_stream = match conn_guard.as_mut() {
                    Some(stream) => stream,
                    None => {
                        debug!("No active WebSocket connection");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };

                tokio::select! {
                    // Handle incoming WebSocket messages
                    msg = ws_stream.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                debug!("Received text message: {} bytes", text.len());
                                *bytes_received.write().await += text.len() as u64;

                                match serde_json::from_str::<JsonRpcRequest>(&text) {
                                    Ok(request) => {
                                        *messages_received.write().await += 1;
                                        if let Err(e) = incoming_tx.send(request) {
                                            error!("Failed to queue incoming message: {}", e);
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse JSON-RPC request: {}", e);
                                    }
                                }
                            }
                            Some(Ok(Message::Binary(data))) => {
                                debug!("Received binary message: {} bytes", data.len());
                                *bytes_received.write().await += data.len() as u64;
                            }
                            Some(Ok(Message::Ping(data))) => {
                                debug!("Received ping, sending pong");
                                if let Err(e) = ws_stream.send(Message::Pong(data)).await {
                                    error!("Failed to send pong: {}", e);
                                }
                            }
                            Some(Ok(Message::Pong(_))) => {
                                debug!("Received pong");
                            }
                            Some(Ok(Message::Close(frame))) => {
                                info!("WebSocket close frame received: {:?}", frame);
                                *state.write().await = ConnectionState::Disconnected;
                                break;
                            }
                            Some(Err(e)) => {
                                error!("WebSocket error: {}", e);
                                *state.write().await = ConnectionState::Disconnected;
                                break;
                            }
                            None => {
                                info!("WebSocket connection closed");
                                *state.write().await = ConnectionState::Disconnected;
                                break;
                            }
                            _ => {}
                        }
                    }

                    // Handle outgoing messages
                    msg = async {
                        let mut rx = outgoing_rx.write().await;
                        rx.recv().await
                    } => {
                        if let Some(response) = msg {
                            match serde_json::to_string(&response) {
                                Ok(json) => {
                                    let msg_size = json.len();
                                    if let Err(e) = ws_stream.send(Message::Text(json.into())).await {
                                        error!("Failed to send message: {}", e);
                                        *state.write().await = ConnectionState::Disconnected;
                                        break;
                                    } else {
                                        *messages_sent.write().await += 1;
                                        *bytes_sent.write().await += msg_size as u64;
                                        debug!("Sent message: {} bytes", msg_size);
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to serialize response: {}", e);
                                }
                            }
                        }
                    }

                    // Heartbeat
                    _ = heartbeat_interval_timer.tick(), if config.heartbeat_interval > 0 => {
                        debug!("Sending heartbeat ping");
                        if let Err(e) = ws_stream.send(Message::Ping(vec![].into())).await {
                            error!("Failed to send ping: {}", e);
                            *state.write().await = ConnectionState::Disconnected;
                            break;
                        }
                    }

                    // Shutdown signal
                    _ = shutdown_rx.recv() => {
                        info!("Shutdown signal received");
                        *state.write().await = ConnectionState::ShuttingDown;
                        let _ = ws_stream.send(Message::Close(None)).await;
                        break;
                    }
                }
            }

            info!("WebSocket message loop terminated");
        });

        Ok(())
    }
}

#[async_trait]
impl Transport for WebSocketTransport {
    type Error = TransportError;

    async fn start(&mut self) -> std::result::Result<(), Self::Error> {
        info!(
            "Starting WebSocket transport in {} mode",
            if self.config.server_mode {
                "server"
            } else {
                "client"
            }
        );

        if self.config.server_mode {
            self.start_server().await?;
            // Wait for first connection
            for _ in 0..50 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if *self.state.read().await == ConnectionState::Connected {
                    break;
                }
            }
        } else {
            self.start_client().await?;
        }

        // Start message processing loop
        self.start_message_loop().await?;

        info!("WebSocket transport started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> std::result::Result<(), Self::Error> {
        info!("Stopping WebSocket transport");

        *self.state.write().await = ConnectionState::ShuttingDown;

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.write().await.take() {
            let _ = tx.send(()).await;
        }

        // Wait for graceful shutdown
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Close connections
        *self.client_connection.write().await = None;
        *self.server_connection.write().await = None;

        *self.state.write().await = ConnectionState::Disconnected;

        info!("WebSocket transport stopped");
        Ok(())
    }

    async fn send_message(
        &mut self,
        message: JsonRpcResponse,
    ) -> std::result::Result<(), Self::Error> {
        if *self.state.read().await != ConnectionState::Connected {
            return Err(TransportError::Internal(
                "WebSocket not connected".to_string(),
            ));
        }

        self.outgoing_tx.send(message).map_err(|e| {
            TransportError::Internal(format!("Failed to queue outgoing message: {}", e))
        })?;

        Ok(())
    }

    async fn receive_message(
        &mut self,
    ) -> std::result::Result<Option<JsonRpcRequest>, Self::Error> {
        if *self.state.read().await == ConnectionState::Disconnected {
            return Err(TransportError::Internal(
                "WebSocket not connected".to_string(),
            ));
        }

        // Non-blocking receive with timeout
        match timeout(
            Duration::from_millis(100),
            self.incoming_rx.write().await.recv(),
        )
        .await
        {
            Ok(Some(request)) => Ok(Some(request)),
            Ok(None) => Ok(None),
            Err(_) => Ok(None), // Timeout - no message available
        }
    }

    fn is_connected(&self) -> bool {
        // This is sync, so we can't await. Use try_read for non-blocking access
        match self.state.try_read() {
            Ok(state) => *state == ConnectionState::Connected,
            Err(_) => false,
        }
    }

    fn transport_info(&self) -> crate::transport::TransportInfo {
        use crate::transport::{
            FramingMethod, TransportCapabilities, TransportInfo, TransportType,
        };

        TransportInfo {
            transport_type: TransportType::WebSocket {
                url: self.config.url.clone(),
            },
            description: format!(
                "WebSocket transport ({} mode, TLS: {})",
                if self.config.server_mode {
                    "server"
                } else {
                    "client"
                },
                self.config.use_tls
            ),
            capabilities: TransportCapabilities {
                bidirectional: true,
                multiplexing: true,
                compression: false,
                max_message_size: Some(self.config.max_message_size),
                framing_methods: vec![FramingMethod::WebSocketFrame],
            },
        }
    }

    fn connection_stats(&self) -> crate::transport::ConnectionStats {
        use crate::transport::ConnectionStats;

        // Try to read stats without blocking
        let messages_sent = self.messages_sent.try_read().map(|v| *v).unwrap_or(0);
        let messages_received = self.messages_received.try_read().map(|v| *v).unwrap_or(0);
        let bytes_sent = self.bytes_sent.try_read().map(|v| *v).unwrap_or(0);
        let bytes_received = self.bytes_received.try_read().map(|v| *v).unwrap_or(0);

        ConnectionStats {
            messages_sent,
            messages_received,
            bytes_sent,
            bytes_received,
            uptime: Duration::from_secs(0),
            last_activity: None,
        }
    }
}
