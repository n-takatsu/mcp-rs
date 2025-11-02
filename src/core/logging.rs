use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, field, info, warn, Span};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use uuid::Uuid;

/// Global request counter for generating unique request IDs
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Log configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// Log level (error, warn, info, debug, trace)
    pub level: String,

    /// Log format (json, human)
    pub format: LogFormat,

    /// Optional log file path
    pub file: Option<String>,

    /// Enable request-level logging
    pub request_logging: bool,

    /// Enable performance metrics
    pub performance_metrics: bool,

    /// Plugin-specific log levels
    pub plugins: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Json,
    Human,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: if cfg!(debug_assertions) {
                LogFormat::Human
            } else {
                LogFormat::Json
            },
            file: None,
            request_logging: true,
            performance_metrics: true,
            plugins: HashMap::new(),
        }
    }
}

/// Request context for correlation and tracing
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub request_id: String,
    pub session_id: Option<String>,
    pub user_id: Option<String>,
    pub operation: Option<String>,
    pub plugin: Option<String>,
    pub start_time: SystemTime,
}

impl RequestContext {
    /// Create a new request context with generated request ID
    pub fn new() -> Self {
        let counter = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        Self {
            request_id: format!("req_{}_{}", timestamp, counter),
            session_id: None,
            user_id: None,
            operation: None,
            plugin: None,
            start_time: SystemTime::now(),
        }
    }

    /// Create context with specific request ID
    pub fn with_id(request_id: String) -> Self {
        Self {
            request_id,
            session_id: None,
            user_id: None,
            operation: None,
            plugin: None,
            start_time: SystemTime::now(),
        }
    }

    /// Set session ID
    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Set user ID
    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set operation name
    pub fn with_operation(mut self, operation: String) -> Self {
        self.operation = Some(operation);
        self
    }

    /// Set plugin name
    pub fn with_plugin(mut self, plugin: String) -> Self {
        self.plugin = Some(plugin);
        self
    }

    /// Calculate elapsed time since request start
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().unwrap_or_default().as_millis() as u64
    }
}

/// Logging utility for structured logging with context
pub struct Logger {
    config: LogConfig,
}

impl Logger {
    /// Initialize global logging with configuration
    pub fn init(config: LogConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let filter =
            EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(&config.level))?;

        let registry = tracing_subscriber::registry().with(filter);

        match config.format {
            LogFormat::Json => {
                let fmt_layer = fmt::layer()
                    .with_span_events(FmtSpan::CLOSE)
                    .with_thread_ids(true)
                    .with_thread_names(true);

                registry.with(fmt_layer).init();
            }
            LogFormat::Human => {
                let fmt_layer = fmt::layer()
                    .with_span_events(FmtSpan::CLOSE)
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_thread_names(true);

                registry.with(fmt_layer).init();
            }
        }

        info!(
            level = %config.level,
            format = ?config.format,
            request_logging = config.request_logging,
            performance_metrics = config.performance_metrics,
            "Logging system initialized"
        );

        Ok(Self { config })
    }

    /// Create a request span with context
    pub fn request_span(&self, ctx: &RequestContext) -> Span {
        let span = tracing::info_span!(
            "mcp_request",
            request_id = %ctx.request_id,
            session_id = field::Empty,
            user_id = field::Empty,
            operation = field::Empty,
            plugin = field::Empty,
            duration_ms = field::Empty,
        );

        if let Some(ref session_id) = ctx.session_id {
            span.record("session_id", session_id);
        }
        if let Some(ref user_id) = ctx.user_id {
            span.record("user_id", user_id);
        }
        if let Some(ref operation) = ctx.operation {
            span.record("operation", operation);
        }
        if let Some(ref plugin) = ctx.plugin {
            span.record("plugin", plugin);
        }

        span
    }

    /// Create a plugin operation span
    pub fn plugin_span(&self, plugin: &str, operation: &str, request_id: &str) -> Span {
        tracing::info_span!(
            "plugin_operation",
            plugin = plugin,
            operation = operation,
            request_id = request_id,
            duration_ms = field::Empty,
            error = field::Empty,
        )
    }

    /// Log request completion with metrics
    pub fn log_request_complete(&self, ctx: &RequestContext, success: bool) {
        let duration_ms = ctx.elapsed_ms();

        if success {
            info!(
                request_id = %ctx.request_id,
                operation = ?ctx.operation,
                plugin = ?ctx.plugin,
                duration_ms = duration_ms,
                "Request completed successfully"
            );
        } else {
            warn!(
                request_id = %ctx.request_id,
                operation = ?ctx.operation,
                plugin = ?ctx.plugin,
                duration_ms = duration_ms,
                "Request completed with error"
            );
        }
    }

    /// Log plugin operation with context
    pub fn log_plugin_operation(
        &self,
        plugin: &str,
        operation: &str,
        request_id: &str,
        params: Option<&serde_json::Value>,
    ) {
        debug!(
            plugin = plugin,
            operation = operation,
            request_id = request_id,
            params = ?params,
            "Plugin operation started"
        );
    }

    /// Log security event
    pub fn log_security_event(
        &self,
        event_type: &str,
        request_id: &str,
        details: serde_json::Value,
    ) {
        warn!(
            event_type = event_type,
            request_id = request_id,
            details = ?details,
            "Security event detected"
        );
    }

    /// Log performance metrics
    pub fn log_performance_metrics(
        &self,
        operation: &str,
        duration_ms: u64,
        success: bool,
        details: Option<serde_json::Value>,
    ) {
        if self.config.performance_metrics {
            info!(
                operation = operation,
                duration_ms = duration_ms,
                success = success,
                details = ?details,
                "Performance metric"
            );
        }
    }

    /// Log configuration change
    pub fn log_config_change(&self, change_type: &str, old_value: Option<&str>, new_value: &str) {
        info!(
            change_type = change_type,
            old_value = old_value,
            new_value = new_value,
            "Configuration changed"
        );
    }

    /// Get plugin-specific log level
    pub fn get_plugin_level(&self, plugin: &str) -> Option<&String> {
        self.config.plugins.get(plugin)
    }
}

/// Convenience macros for context-aware logging
#[macro_export]
macro_rules! log_with_context {
    ($level:ident, $ctx:expr, $($arg:tt)*) => {
        tracing::$level!(
            request_id = %$ctx.request_id,
            operation = ?$ctx.operation,
            plugin = ?$ctx.plugin,
            $($arg)*
        );
    };
}

#[macro_export]
macro_rules! log_plugin_error {
    ($plugin:expr, $operation:expr, $request_id:expr, $error:expr, $($arg:tt)*) => {
        tracing::error!(
            plugin = $plugin,
            operation = $operation,
            request_id = $request_id,
            error = %$error,
            $($arg)*
        );
    };
}

#[macro_export]
macro_rules! log_plugin_success {
    ($plugin:expr, $operation:expr, $request_id:expr, $duration_ms:expr, $($arg:tt)*) => {
        tracing::info!(
            plugin = $plugin,
            operation = $operation,
            request_id = $request_id,
            duration_ms = $duration_ms,
            success = true,
            $($arg)*
        );
    };
}

/// Error context for structured error logging
#[derive(Debug, Serialize)]
pub struct ErrorContext {
    pub request_id: String,
    pub operation: Option<String>,
    pub plugin: Option<String>,
    pub error_type: String,
    pub error_message: String,
    pub stack_trace: Option<String>,
    pub user_data: Option<serde_json::Value>,
}

impl ErrorContext {
    pub fn new(request_id: String, error: &dyn std::error::Error) -> Self {
        Self {
            request_id,
            operation: None,
            plugin: None,
            error_type: std::any::type_name_of_val(error).to_string(),
            error_message: error.to_string(),
            stack_trace: None,
            user_data: None,
        }
    }

    pub fn with_operation(mut self, operation: String) -> Self {
        self.operation = Some(operation);
        self
    }

    pub fn with_plugin(mut self, plugin: String) -> Self {
        self.plugin = Some(plugin);
        self
    }

    pub fn with_user_data(mut self, data: serde_json::Value) -> Self {
        self.user_data = Some(data);
        self
    }

    /// Log this error context
    pub fn log(&self) {
        error!(
            request_id = %self.request_id,
            operation = ?self.operation,
            plugin = ?self.plugin,
            error_type = %self.error_type,
            error_message = %self.error_message,
            stack_trace = ?self.stack_trace,
            user_data = ?self.user_data,
            "Error occurred with context"
        );
    }
}
