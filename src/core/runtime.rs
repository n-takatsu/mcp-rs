//! Runtime Management
//!
//! Manages the application lifecycle, configuration, and global state.

use crate::{
    config::McpConfig,
    core::context::ExecutionContext,
    mcp::McpError,
    transport::{Transport, TransportError, TransportFactory},
    types::{JsonRpcRequest, JsonRpcResponse},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// MCP configuration
    pub mcp_config: McpConfig,
    /// Runtime-specific settings
    pub max_concurrent_requests: usize,
    /// Request timeout in seconds
    pub default_timeout_seconds: u64,
    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            mcp_config: McpConfig::default(),
            max_concurrent_requests: 100,
            default_timeout_seconds: 30,
            enable_metrics: false,
        }
    }
}

/// Runtime state
#[derive(Debug, Clone)]
pub enum RuntimeState {
    /// Runtime is initializing
    Initializing,
    /// Runtime is running and ready to handle requests
    Running,
    /// Runtime is shutting down gracefully
    Stopping,
    /// Runtime has stopped
    Stopped,
}

/// Core Runtime that manages the application lifecycle
pub struct Runtime {
    /// Runtime configuration
    config: RuntimeConfig,
    /// Current runtime state
    state: Arc<RwLock<RuntimeState>>,
    /// Handler registry for managing plugins
    handler_registry: Arc<RwLock<crate::core::HandlerRegistry>>,
    /// Transport layer for communication
    transport: Arc<RwLock<Option<Box<dyn Transport<Error = TransportError>>>>>,
    /// Metrics collector (optional)
    metrics: Option<Arc<RwLock<RuntimeMetrics>>>,
}

#[derive(Debug, Default)]
pub struct RuntimeMetrics {
    pub requests_total: u64,
    pub requests_successful: u64,
    pub requests_failed: u64,
    pub active_handlers: u64,
}

impl Runtime {
    /// Create a new runtime with the given configuration
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(RuntimeState::Initializing)),
            handler_registry: Arc::new(RwLock::new(crate::core::HandlerRegistry::new())),
            transport: Arc::new(RwLock::new(None)),
            metrics: None,
        }
    }

    /// Initialize the runtime and all registered handlers
    pub async fn initialize(&self) -> Result<(), McpError> {
        info!("Initializing MCP-RS Runtime...");

        // Update state to running
        {
            let mut state = self.state.write().await;
            *state = RuntimeState::Running;
        }

        // Initialize transport
        self.initialize_transport().await?;

        // Initialize metrics if enabled
        if self.config.enable_metrics {
            // TODO: Initialize metrics collection
            info!("Metrics collection enabled");
        }

        // Initialize handler registry
        {
            let mut registry = self.handler_registry.write().await;
            registry.initialize().await?;
        }

        info!("Runtime initialization complete");
        Ok(())
    }

    /// Initialize transport layer
    async fn initialize_transport(&self) -> Result<(), McpError> {
        info!("Initializing transport layer...");

        let transport_config = self.config.mcp_config.to_transport_config();
        let mut transport = TransportFactory::create_transport(&transport_config)
            .map_err(|e| McpError::InternalError(format!("Failed to create transport: {}", e)))?;

        transport
            .start()
            .await
            .map_err(|e| McpError::InternalError(format!("Failed to start transport: {}", e)))?;

        info!("Transport initialized: {:?}", transport.transport_info());

        // Store transport
        {
            let mut transport_lock = self.transport.write().await;
            *transport_lock = Some(transport);
        }

        Ok(())
    }

    /// Shutdown the runtime gracefully
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down runtime...");

        // Update state to stopping
        {
            let mut state = self.state.write().await;
            *state = RuntimeState::Stopping;
        }

        // Shutdown transport
        {
            let mut transport_lock = self.transport.write().await;
            if let Some(transport) = transport_lock.as_mut() {
                if let Err(e) = transport.stop().await {
                    warn!("Error stopping transport: {}", e);
                }
            }
            *transport_lock = None;
        }

        // Shutdown all handlers
        {
            let mut registry = self.handler_registry.write().await;
            registry.shutdown_all().await?;
        }

        // Update state to stopped
        {
            let mut state = self.state.write().await;
            *state = RuntimeState::Stopped;
        }

        info!("Runtime shutdown complete");
        Ok(())
    }

    /// Get current runtime state
    pub async fn state(&self) -> RuntimeState {
        let state = self.state.read().await;
        state.clone()
    }

    /// Get runtime configuration
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Get handler registry
    pub fn handler_registry(&self) -> Arc<RwLock<crate::core::HandlerRegistry>> {
        Arc::clone(&self.handler_registry)
    }

    /// Check if runtime is ready to handle requests
    pub async fn is_ready(&self) -> bool {
        matches!(*self.state.read().await, RuntimeState::Running)
    }

    /// Update metrics (if enabled)
    pub async fn record_request(&self, success: bool) {
        if let Some(metrics) = &self.metrics {
            let mut m = metrics.write().await;
            m.requests_total += 1;
            if success {
                m.requests_successful += 1;
            } else {
                m.requests_failed += 1;
            }
        }
    }

    /// Get current metrics snapshot
    pub async fn metrics(&self) -> Option<RuntimeMetrics> {
        if let Some(metrics) = &self.metrics {
            let m = metrics.read().await;
            Some(RuntimeMetrics {
                requests_total: m.requests_total,
                requests_successful: m.requests_successful,
                requests_failed: m.requests_failed,
                active_handlers: m.active_handlers,
            })
        } else {
            None
        }
    }

    /// Run the main message processing loop
    pub async fn run(&self) -> Result<(), McpError> {
        info!("Starting main message processing loop...");

        loop {
            // Check if we should continue running
            if !self.is_ready().await {
                info!("Runtime is not ready, stopping message loop");
                break;
            }

            // Process incoming messages
            match self.process_next_message().await {
                Ok(should_continue) => {
                    if !should_continue {
                        info!("Message processing signaled to stop");
                        break;
                    }
                }
                Err(e) => {
                    error!("Error processing message: {}", e);
                    // Continue processing despite errors
                }
            }
        }

        info!("Message processing loop ended");
        Ok(())
    }

    /// Process the next incoming message
    async fn process_next_message(&self) -> Result<bool, McpError> {
        let mut transport_lock = self.transport.write().await;
        let transport = transport_lock
            .as_mut()
            .ok_or_else(|| McpError::InternalError("Transport not initialized".to_string()))?;

        // Receive message (non-blocking)
        match transport.receive_message().await {
            Ok(Some(request)) => {
                drop(transport_lock); // Release lock early
                self.handle_request(request).await?;
                Ok(true)
            }
            Ok(None) => {
                // No message available, continue
                Ok(true)
            }
            Err(e) => {
                error!("Transport error: {}", e);
                // For transport errors, we might want to continue or reconnect
                // For now, continue processing
                Ok(true)
            }
        }
    }

    /// Handle a JSON-RPC request
    async fn handle_request(&self, request: JsonRpcRequest) -> Result<(), McpError> {
        info!(
            "Handling request: method={}, id={:?}",
            request.method, request.id
        );

        // Create execution context
        let timeout = std::time::Duration::from_secs(self.config.default_timeout_seconds);
        let mut context = ExecutionContext::new(timeout);
        context.set_handler("runtime".to_string());

        // Route request to appropriate handler
        let response = match self.route_request(&request, &mut context).await {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(result),
                error: None,
                id: request.id,
            },
            Err(e) => {
                error!("Request handling failed: {}", e);
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(crate::types::JsonRpcError {
                        code: -32603,
                        message: e.to_string(),
                        data: None,
                    }),
                    id: request.id,
                }
            }
        };

        // Send response
        let success = response.error.is_none();
        self.send_response(response).await?;

        // Update metrics
        self.record_request(success).await;

        Ok(())
    }

    /// Route request to appropriate handler
    async fn route_request(
        &self,
        request: &JsonRpcRequest,
        _context: &mut ExecutionContext,
    ) -> Result<serde_json::Value, McpError> {
        let registry = self.handler_registry.read().await;

        match request.method.as_str() {
            "initialize" => {
                // Handle initialization
                Ok(serde_json::json!({
                    "server": {
                        "name": "mcp-rs",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "capabilities": {
                        "tools": { "list_changed": false },
                        "resources": { "subscribe": false, "list_changed": false }
                    }
                }))
            }
            "tools/list" => {
                // Delegate to handler registry
                registry
                    .list_all_tools()
                    .await
                    .map_err(|e| McpError::InternalError(e.to_string()))
            }
            method => Err(McpError::MethodNotFound(format!(
                "Unknown method: {}",
                method
            ))),
        }
    }

    /// Send response through transport
    async fn send_response(&self, response: JsonRpcResponse) -> Result<(), McpError> {
        let mut transport_lock = self.transport.write().await;
        let transport = transport_lock
            .as_mut()
            .ok_or_else(|| McpError::InternalError("Transport not initialized".to_string()))?;

        transport
            .send_message(response)
            .await
            .map_err(|e| McpError::InternalError(format!("Failed to send response: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runtime_lifecycle() {
        let config = RuntimeConfig::default();
        let runtime = Runtime::new(config);

        // Initially should be in Initializing state
        assert!(matches!(runtime.state().await, RuntimeState::Initializing));

        // Initialize
        runtime.initialize().await.unwrap();
        assert!(matches!(runtime.state().await, RuntimeState::Running));
        assert!(runtime.is_ready().await);

        // Shutdown
        runtime.shutdown().await.unwrap();
        assert!(matches!(runtime.state().await, RuntimeState::Stopped));
        assert!(!runtime.is_ready().await);
    }
}
