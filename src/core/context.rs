//! Execution Context
//!
//! Provides request-scoped context for handler execution.

use crate::mcp::McpError;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use uuid::Uuid;

/// Execution context for MCP operations
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExecutionContext {
    /// Unique request ID
    pub request_id: String,
    /// Request start time
    pub start_time: Instant,
    /// Request timeout
    pub timeout: Duration,
    /// Handler name being executed
    pub handler_name: Option<String>,
    /// Tool name being called
    pub tool_name: Option<String>,
    /// Request metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Execution metrics
    pub metrics: ExecutionMetrics,
}

/// Execution metrics for performance tracking
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ExecutionMetrics {
    /// Total execution time
    pub total_duration: Option<Duration>,
    /// Handler execution time
    pub handler_duration: Option<Duration>,
    /// Number of external API calls made
    pub external_calls: u32,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Whether request completed successfully
    pub success: bool,
    /// Error message if failed
    pub error_message: Option<String>,
}

#[allow(dead_code)]
impl ExecutionContext {
    /// Create a new execution context
    pub fn new(timeout: Duration) -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            start_time: Instant::now(),
            timeout,
            handler_name: None,
            tool_name: None,
            metadata: HashMap::new(),
            metrics: ExecutionMetrics::default(),
        }
    }

    /// Create a new execution context with a specific request ID
    pub fn with_id(request_id: String, timeout: Duration) -> Self {
        Self {
            request_id,
            start_time: Instant::now(),
            timeout,
            handler_name: None,
            tool_name: None,
            metadata: HashMap::new(),
            metrics: ExecutionMetrics::default(),
        }
    }

    /// Set the handler name for this execution
    pub fn set_handler(&mut self, handler_name: String) {
        self.handler_name = Some(handler_name);
    }

    /// Set the tool name for this execution
    pub fn set_tool(&mut self, tool_name: String) {
        self.tool_name = Some(tool_name);
    }

    /// Add metadata to the context
    pub fn add_metadata(&mut self, key: String, value: serde_json::Value) {
        self.metadata.insert(key, value);
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Check if request has timed out
    pub fn is_timed_out(&self) -> bool {
        self.start_time.elapsed() > self.timeout
    }

    /// Get remaining time before timeout
    pub fn remaining_time(&self) -> Duration {
        let elapsed = self.start_time.elapsed();
        if elapsed >= self.timeout {
            Duration::ZERO
        } else {
            self.timeout - elapsed
        }
    }

    /// Record the start of handler execution
    pub fn start_handler_execution(&mut self) {
        if self.is_timed_out() {
            warn!(
                "Request {} already timed out before handler execution",
                self.request_id
            );
        }
    }

    /// Record the end of handler execution
    pub fn end_handler_execution(&mut self, success: bool, error: Option<&str>) {
        self.metrics.handler_duration = Some(self.start_time.elapsed());
        self.metrics.success = success;
        if let Some(err) = error {
            self.metrics.error_message = Some(err.to_string());
        }
    }

    /// Record an external API call
    pub fn record_external_call(&mut self, bytes: u64) {
        self.metrics.external_calls += 1;
        self.metrics.bytes_transferred += bytes;
    }

    /// Finalize the execution context
    pub fn finalize(&mut self) -> ExecutionMetrics {
        self.metrics.total_duration = Some(self.start_time.elapsed());

        info!(
            "Request {} completed in {:?} (handler: {:?}, tool: {:?}, success: {})",
            self.request_id,
            self.metrics.total_duration,
            self.handler_name,
            self.tool_name,
            self.metrics.success
        );

        self.metrics.clone()
    }

    /// Create a timeout error for this context
    pub fn timeout_error(&self) -> McpError {
        McpError::InvalidRequest(format!(
            "Request {} timed out after {:?}",
            self.request_id, self.timeout
        ))
    }

    /// Clone context for sub-operations (with new request ID)
    pub fn spawn_child(&self, operation: &str) -> Self {
        let mut child = Self::new(self.remaining_time());
        child.add_metadata(
            "parent_request_id".to_string(),
            serde_json::Value::String(self.request_id.clone()),
        );
        child.add_metadata(
            "operation".to_string(),
            serde_json::Value::String(operation.to_string()),
        );

        // Inherit handler and tool names if set
        if let Some(ref handler) = self.handler_name {
            child.handler_name = Some(handler.clone());
        }

        child
    }

    /// Get execution summary for logging
    pub fn summary(&self) -> String {
        format!(
            "RequestId: {}, Elapsed: {:?}, Handler: {:?}, Tool: {:?}, TimeRemaining: {:?}",
            self.request_id,
            self.start_time.elapsed(),
            self.handler_name,
            self.tool_name,
            self.remaining_time()
        )
    }
}

/// Context manager for controlling execution contexts
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ContextManager {
    /// Default timeout for requests
    default_timeout: Duration,
    /// Maximum allowed timeout
    max_timeout: Duration,
}

#[allow(dead_code)]
impl ContextManager {
    /// Create a new context manager
    pub fn new(default_timeout: Duration, max_timeout: Duration) -> Self {
        Self {
            default_timeout,
            max_timeout,
        }
    }

    /// Create a new execution context with default timeout
    pub fn create_context(&self) -> ExecutionContext {
        ExecutionContext::new(self.default_timeout)
    }

    /// Create a new execution context with specific timeout
    pub fn create_context_with_timeout(
        &self,
        timeout: Duration,
    ) -> Result<ExecutionContext, McpError> {
        if timeout > self.max_timeout {
            return Err(McpError::InvalidParams(format!(
                "Timeout {:?} exceeds maximum allowed {:?}",
                timeout, self.max_timeout
            )));
        }

        Ok(ExecutionContext::new(timeout))
    }

    /// Get default timeout
    pub fn default_timeout(&self) -> Duration {
        self.default_timeout
    }

    /// Get maximum timeout
    pub fn max_timeout(&self) -> Duration {
        self.max_timeout
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new(
            Duration::from_secs(30),  // 30 second default
            Duration::from_secs(300), // 5 minute maximum
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_execution_context_lifecycle() {
        let mut ctx = ExecutionContext::new(Duration::from_millis(100));

        // Verify initial state
        assert!(!ctx.request_id.to_string().is_empty());

        // Set handler and tool
        ctx.set_handler("test_handler".to_string());
        ctx.set_tool("test_tool".to_string());

        // Add metadata
        ctx.add_metadata(
            "key1".to_string(),
            serde_json::Value::String("value1".to_string()),
        );

        // Check metadata
        assert_eq!(
            ctx.get_metadata("key1"),
            Some(&serde_json::Value::String("value1".to_string()))
        );

        // Record execution
        ctx.start_handler_execution();
        sleep(Duration::from_millis(10));
        ctx.end_handler_execution(true, None);

        // Finalize
        let metrics = ctx.finalize();
        assert!(metrics.total_duration.is_some());
        assert!(metrics.handler_duration.is_some());
        assert!(metrics.success);
    }

    #[test]
    fn test_timeout_detection() {
        let ctx = ExecutionContext::new(Duration::from_millis(50));

        // Should not be timed out initially
        assert!(!ctx.is_timed_out());
        assert!(ctx.remaining_time() > Duration::ZERO);

        // Wait for timeout
        sleep(Duration::from_millis(60));

        // Should be timed out now
        assert!(ctx.is_timed_out());
        assert_eq!(ctx.remaining_time(), Duration::ZERO);
    }

    #[test]
    fn test_child_context() {
        let mut parent = ExecutionContext::new(Duration::from_secs(10));
        parent.set_handler("parent_handler".to_string());

        let child = parent.spawn_child("sub_operation");

        // Child should inherit handler
        assert_eq!(child.handler_name, Some("parent_handler".to_string()));

        // Child should have parent request ID in metadata
        assert!(child.get_metadata("parent_request_id").is_some());
        assert!(child.get_metadata("operation").is_some());
    }

    #[test]
    fn test_context_manager() {
        let manager = ContextManager::new(Duration::from_secs(30), Duration::from_secs(300));

        // Create default context
        let ctx = manager.create_context();
        assert!(ctx.remaining_time() <= Duration::from_secs(30));

        // Create context with custom timeout
        let ctx = manager
            .create_context_with_timeout(Duration::from_secs(60))
            .unwrap();
        assert!(ctx.remaining_time() <= Duration::from_secs(60));

        // Try to create context with excessive timeout
        let result = manager.create_context_with_timeout(Duration::from_secs(400));
        assert!(result.is_err());
    }
}
