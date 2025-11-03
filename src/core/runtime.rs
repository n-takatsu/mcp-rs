//! Runtime Management
//!
//! Manages the application lifecycle, configuration, and global state.

use crate::config::McpConfig;
use crate::mcp::McpError;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

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

    /// Shutdown the runtime gracefully
    pub async fn shutdown(&self) -> Result<(), McpError> {
        info!("Shutting down runtime...");

        // Update state to stopping
        {
            let mut state = self.state.write().await;
            *state = RuntimeState::Stopping;
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
            let metrics_guard = metrics.read().await;
            Some(RuntimeMetrics {
                requests_total: metrics_guard.requests_total,
                requests_successful: metrics_guard.requests_successful,
                requests_failed: metrics_guard.requests_failed,
                active_handlers: metrics_guard.active_handlers,
            })
        } else {
            None
        }
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
