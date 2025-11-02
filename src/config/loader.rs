use super::types::McpConfig;
use anyhow::{Context, Result};
use config::{Config, Environment, File};

/// Configuration loader with builder pattern
pub struct ConfigLoader {
    config_file: Option<String>,
    load_env: bool,
    cli_override: Option<McpConfig>,
}

impl ConfigLoader {
    /// Create a new configuration loader
    pub fn new() -> Self {
        Self {
            config_file: None,
            load_env: false,
            cli_override: None,
        }
    }

    /// Load configuration from file
    pub fn load_from_file(mut self, path: Option<&str>) -> Self {
        self.config_file = path.map(String::from);
        self
    }

    /// Load configuration from environment variables
    pub fn load_from_env(mut self) -> Self {
        self.load_env = true;
        self
    }

    /// Load configuration from CLI arguments
    pub fn load_from_cli<T>(mut self, _cli: &T) -> Self {
        // For now, create a minimal config from CLI
        // This would be expanded to properly convert CLI args to config
        self.cli_override = Some(McpConfig::default());
        self
    }

    /// Build the final configuration
    pub fn build(self) -> Result<McpConfig> {
        let mut builder = Config::builder().add_source(Config::try_from(&McpConfig::default())?);

        // Add configuration file if specified
        if let Some(config_path) = &self.config_file {
            builder = builder.add_source(File::with_name(config_path).required(false));
        } else {
            // Try to load from standard locations
            builder = builder
                .add_source(File::with_name("mcp-rs").required(false))
                .add_source(File::with_name("config/mcp-rs").required(false));
        }

        // Add environment variables if requested
        if self.load_env {
            builder = builder.add_source(
                Environment::with_prefix("MCP")
                    .prefix_separator("_")
                    .separator("__"),
            );
        }

        // Build the configuration
        let config: McpConfig = builder
            .build()
            .context("Failed to build configuration")?
            .try_deserialize()
            .context("Failed to deserialize configuration")?;

        Ok(config)
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}
