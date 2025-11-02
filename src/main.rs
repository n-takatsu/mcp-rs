use clap::Parser;
use config::{ConfigLoader, McpConfig};
use core::{
    logging::{LogConfig, LogFormat, Logger},
    transport::Transport,
    McpServer,
};
use plugins::PluginRegistry;
use std::sync::Arc;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Use STDIO transport (for MCP clients)
    #[arg(long)]
    stdio: bool,

    /// Bind address for TCP transport
    #[arg(long, default_value = "127.0.0.1")]
    bind_address: String,

    /// Port for TCP transport  
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    /// Enable specific plugins
    #[arg(long)]
    enable_plugin: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Load configuration first
    let config = ConfigLoader::new()
        .load_from_file(cli.config.as_deref())
        .load_from_env()
        .load_from_cli(&cli)
        .build()?;

    // Initialize structured logging system
    let mut log_config = LogConfig {
        level: if cli.debug {
            "debug".to_string()
        } else {
            config.logging.level.clone()
        },
        format: match config.logging.format {
            config::LogFormat::Json => LogFormat::Json,
            _ => LogFormat::Human,
        },
        file: config.logging.file.clone(),
        request_logging: config.logging.request_logging,
        performance_metrics: config.logging.performance_metrics,
        plugins: config.logging.plugins.clone(),
    };

    // Override log level if debug flag is set
    if cli.debug {
        log_config.level = "debug".to_string();
    }

    let _logger = Logger::init(log_config)?;

    // Initialize plugin registry
    let mut plugin_registry = PluginRegistry::new();
    plugin_registry.load_from_config(&config).await?;

    // Create MCP server
    let server = McpServer::new(Arc::new(config), Arc::new(plugin_registry));

    // Determine transport based on CLI arguments
    let transport = if cli.stdio {
        Transport::Stdio
    } else {
        Transport::Tcp {
            address: cli.bind_address,
            port: cli.port,
        }
    };

    // Run server
    server.run(transport).await?;

    Ok(())
}
