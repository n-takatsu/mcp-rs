use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    /// Server configuration
    pub server: ServerConfig,
    
    /// Plugin configurations
    pub plugins: HashMap<String, PluginConfig>,
    
    /// Transport configurations
    pub transport: TransportConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
}

/// Server-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server name
    pub name: String,
    
    /// Server version
    pub version: String,
    
    /// Maximum concurrent connections
    pub max_connections: Option<usize>,
    
    /// Request timeout in seconds
    pub timeout: Option<u64>,
    
    /// Server capabilities
    pub capabilities: ServerCapabilities,
}

/// Server capabilities configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Tools capability
    pub tools: Option<ToolsCapability>,
    
    /// Resources capability
    pub resources: Option<ResourcesCapability>,
    
    /// Prompts capability
    pub prompts: Option<PromptsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// Support for listing tools
    pub list: bool,
    
    /// Support for tool notifications
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcesCapability {
    /// Support for listing resources
    pub list: bool,
    
    /// Support for subscribing to resources
    pub subscribe: bool,
    
    /// Support for resource notifications
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptsCapability {
    /// Support for listing prompts
    pub list: bool,
    
    /// Support for prompt notifications
    pub list_changed: bool,
}

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin enabled state
    pub enabled: bool,
    
    /// Plugin priority (lower number = higher priority)
    pub priority: Option<i32>,
    
    /// Plugin-specific configuration
    pub config: serde_json::Value,
}

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Stdio transport configuration
    pub stdio: Option<StdioConfig>,
    
    /// TCP transport configuration
    pub tcp: Option<TcpConfig>,
    
    /// WebSocket transport configuration
    pub websocket: Option<WebSocketConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdioConfig {
    /// Enable stdio transport
    pub enabled: bool,
    
    /// Buffer size for stdio operations
    pub buffer_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpConfig {
    /// Enable TCP transport
    pub enabled: bool,
    
    /// Bind address
    pub bind_address: String,
    
    /// Bind port
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Enable WebSocket transport
    pub enabled: bool,
    
    /// Bind address
    pub bind_address: String,
    
    /// Bind port
    pub port: u16,
    
    /// WebSocket path
    pub path: String,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (error, warn, info, debug, trace)
    pub level: String,
    
    /// Log format
    pub format: LogFormat,
    
    /// Log output destination
    pub output: LogOutput,
    
    /// Enable request-level logging
    #[serde(default = "default_true")]
    pub request_logging: bool,
    
    /// Enable performance metrics
    #[serde(default = "default_true")]
    pub performance_metrics: bool,
    
    /// Plugin-specific log levels
    #[serde(default)]
    pub plugins: HashMap<String, String>,
    
    /// Optional log file path
    pub file: Option<String>,
    
    /// Log rotation settings
    pub rotation: Option<LogRotation>,
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotation {
    /// Rotation strategy (daily, size, hourly)
    pub strategy: RotationStrategy,
    
    /// Maximum file size in bytes (for size-based rotation)
    pub max_size: Option<u64>,
    
    /// Number of files to keep
    pub keep_files: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationStrategy {
    #[serde(rename = "daily")]
    Daily,
    #[serde(rename = "hourly")]
    Hourly,
    #[serde(rename = "size")]
    Size,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    /// Pretty-printed format
    #[serde(rename = "pretty")]
    Pretty,
    
    /// JSON format
    #[serde(rename = "json")]
    Json,
    
    /// Compact format
    #[serde(rename = "compact")]
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    /// Output to stdout
    #[serde(rename = "stdout")]
    Stdout,
    
    /// Output to stderr
    #[serde(rename = "stderr")]
    Stderr,
    
    /// Output to file
    #[serde(rename = "file")]
    File { path: PathBuf },
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                name: "mcp-rs".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                max_connections: Some(100),
                timeout: Some(30),
                capabilities: ServerCapabilities {
                    tools: Some(ToolsCapability {
                        list: true,
                        list_changed: false,
                    }),
                    resources: Some(ResourcesCapability {
                        list: true,
                        subscribe: false,
                        list_changed: false,
                    }),
                    prompts: Some(PromptsCapability {
                        list: true,
                        list_changed: false,
                    }),
                },
            },
            plugins: HashMap::new(),
            transport: TransportConfig {
                stdio: Some(StdioConfig {
                    enabled: true,
                    buffer_size: Some(8192),
                }),
                tcp: Some(TcpConfig {
                    enabled: false,
                    bind_address: "127.0.0.1".to_string(),
                    port: 8080,
                }),
                websocket: Some(WebSocketConfig {
                    enabled: false,
                    bind_address: "127.0.0.1".to_string(),
                    port: 8081,
                    path: "/mcp".to_string(),
                }),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Pretty,
                output: LogOutput::Stderr,
                request_logging: true,
                performance_metrics: true,
                plugins: HashMap::new(),
                file: None,
                rotation: None,
            },
        }
    }
}