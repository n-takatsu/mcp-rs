//! CLI configuration management

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// MCP server endpoint
    pub server_endpoint: String,
    
    /// API key for authentication
    pub api_key: Option<String>,
    
    /// Default namespace
    pub default_namespace: String,
    
    /// Timeout in seconds
    pub timeout_secs: u64,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server_endpoint: "http://localhost:3000".to_string(),
            api_key: None,
            default_namespace: "default".to_string(),
            timeout_secs: 30,
        }
    }
}

pub fn default_config() -> CliConfig {
    CliConfig::default()
}

pub fn load_config(path: &Path) -> Result<CliConfig, Box<dyn Error>> {
    let content = std::fs::read_to_string(path)?;
    
    if path.extension().and_then(|s| s.to_str()) == Some("yaml") || 
       path.extension().and_then(|s| s.to_str()) == Some("yml") {
        let config: CliConfig = serde_yaml_ng::from_str(&content)?;
        Ok(config)
    } else {
        let config: CliConfig = toml::from_str(&content)?;
        Ok(config)
    }
}

pub fn save_config(path: &Path, config: &CliConfig) -> Result<(), Box<dyn Error>> {
    let content = if path.extension().and_then(|s| s.to_str()) == Some("yaml") ||
                     path.extension().and_then(|s| s.to_str()) == Some("yml") {
        serde_yaml_ng::to_string(config)?
    } else {
        toml::to_string_pretty(config)?
    };
    
    std::fs::write(path, content)?;
    Ok(())
}
