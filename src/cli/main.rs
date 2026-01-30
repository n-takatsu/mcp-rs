//! MCP CLI - Command Line Interface for MCP Plugin Management
//!
//! This module provides a command-line interface for managing MCP plugins,
//! deployments, and security policies.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod commands;
mod config;

use commands::{plugin, security, deployment};

/// MCP CLI - Model Context Protocol Plugin Management Tool
#[derive(Parser)]
#[command(name = "mcp-cli")]
#[command(version, about, long_about = None)]
#[command(author = "n-takatsu")]
struct Cli {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Verbosity level (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Output format (json, yaml, table)
    #[arg(short, long, default_value = "table")]
    format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Plugin management commands
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Security management commands
    Security {
        #[command(subcommand)]
        action: SecurityAction,
    },
    /// Deployment management commands
    Deployment {
        #[command(subcommand)]
        action: DeploymentAction,
    },
}

#[derive(Subcommand)]
enum PluginAction {
    /// List all plugins
    List {
        /// Filter by status (running, stopped, all)
        #[arg(short, long, default_value = "all")]
        status: String,
    },
    /// Deploy a plugin
    Deploy {
        /// Plugin ID
        plugin_id: String,
        
        /// Container image
        #[arg(short, long)]
        image: String,
        
        /// Configuration file
        #[arg(short, long)]
        config: Option<PathBuf>,
        
        /// Number of replicas
        #[arg(short, long, default_value = "1")]
        replicas: i32,
    },
    /// Show plugin logs
    Logs {
        /// Plugin ID
        plugin_id: String,
        
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        
        /// Number of lines to show
        #[arg(short, long)]
        tail: Option<usize>,
    },
    /// Scale a plugin deployment
    Scale {
        /// Plugin ID
        plugin_id: String,
        
        /// Number of replicas
        #[arg(short, long)]
        replicas: i32,
    },
    /// Stop a plugin
    Stop {
        /// Plugin ID
        plugin_id: String,
    },
    /// Start a plugin
    Start {
        /// Plugin ID
        plugin_id: String,
    },
    /// Remove a plugin
    Remove {
        /// Plugin ID
        plugin_id: String,
        
        /// Force removal
        #[arg(short, long)]
        force: bool,
    },
    /// Get plugin status
    Status {
        /// Plugin ID
        plugin_id: String,
    },
    /// Inspect plugin details
    Inspect {
        /// Plugin ID
        plugin_id: String,
    },
    /// Execute command in plugin container
    Exec {
        /// Plugin ID
        plugin_id: String,
        
        /// Command to execute
        command: Vec<String>,
    },
}

#[derive(Subcommand)]
enum SecurityAction {
    /// Scan container image for vulnerabilities
    Scan {
        /// Container image to scan
        image: String,
        
        /// Scanner to use (trivy, anchore, clair)
        #[arg(short, long, default_value = "trivy")]
        scanner: String,
        
        /// Output file for scan results
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List security policies
    ListPolicies,
    /// Apply a security policy
    ApplyPolicy {
        /// Policy file path
        policy_file: PathBuf,
    },
    /// Show security audit logs
    AuditLogs {
        /// Plugin ID filter
        #[arg(short, long)]
        plugin_id: Option<String>,
        
        /// Severity filter (info, warning, error, critical)
        #[arg(short, long)]
        severity: Option<String>,
        
        /// Number of log entries
        #[arg(short, long, default_value = "100")]
        limit: usize,
    },
    /// Generate security report
    Report {
        /// Report type (vulnerabilities, compliance, audit)
        #[arg(short, long, default_value = "vulnerabilities")]
        report_type: String,
        
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum DeploymentAction {
    /// Create a new deployment
    Create {
        /// Deployment manifest file
        manifest: PathBuf,
    },
    /// Update an existing deployment
    Update {
        /// Deployment name
        name: String,
        
        /// Updated manifest file
        manifest: PathBuf,
    },
    /// Delete a deployment
    Delete {
        /// Deployment name
        name: String,
        
        /// Force deletion
        #[arg(short, long)]
        force: bool,
    },
    /// List all deployments
    List {
        /// Namespace filter
        #[arg(short, long)]
        namespace: Option<String>,
    },
    /// Get deployment status
    Status {
        /// Deployment name
        name: String,
    },
    /// Rollback a deployment
    Rollback {
        /// Deployment name
        name: String,
        
        /// Revision number
        #[arg(short, long)]
        revision: Option<u32>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    // Load configuration
    let config = if let Some(config_path) = cli.config {
        config::load_config(&config_path)?
    } else {
        config::default_config()
    };

    // Execute command
    match cli.command {
        Commands::Plugin { action } => {
            plugin::execute(action, &config, &cli.format).await?;
        }
        Commands::Security { action } => {
            security::execute(action, &config, &cli.format).await?;
        }
        Commands::Deployment { action } => {
            deployment::execute(action, &config, &cli.format).await?;
        }
    }

    Ok(())
}
