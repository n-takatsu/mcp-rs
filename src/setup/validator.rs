//! Configuration Validator
//!
//! Validates and tests MCP-RS configuration settings

use crate::config::WordPressConfig;
use crate::handlers::wordpress::WordPressHandler;
use crate::error::Error;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use url::Url;

pub struct ConfigValidator {
    http_client: Client,
}

impl ConfigValidator {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("MCP-RS/1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { http_client }
    }

    /// Test WordPress connection with provided configuration
    pub async fn test_wordpress_connection(&self, config: &WordPressConfig) -> Result<WordPressConnectionInfo, Error> {
        // URL validation
        self.validate_wordpress_url(&config.url)?;
        
        // Basic connectivity test
        self.test_basic_connectivity(&config.url).await?;
        
        // WordPress API test
        self.test_wordpress_api(config).await?;
        
        // Authentication test
        self.test_wordpress_auth(config).await?;
        
        Ok(WordPressConnectionInfo {
            url: config.url.clone(),
            version: self.get_wordpress_version(config).await?,
            api_available: true,
            auth_valid: true,
        })
    }

    fn validate_wordpress_url(&self, url: &str) -> Result<(), Error> {
        // Parse URL
        let parsed_url = Url::parse(url)
            .map_err(|e| Error::Config(format!("Invalid URL format: {}", e)))?;

        // Check scheme
        match parsed_url.scheme() {
            "https" => Ok(()),
            "http" => {
                // Allow HTTP but warn
                Ok(())
            },
            _ => Err(Error::Config(format!(
                "Unsupported URL scheme: {}. Use http:// or https://",
                parsed_url.scheme()
            ))),
        }?;

        // Check if host is present
        if parsed_url.host().is_none() {
            return Err(Error::Config("URL must contain a valid host".to_string()));
        }

        Ok(())
    }

    async fn test_basic_connectivity(&self, url: &str) -> Result<(), Error> {
        let test_url = format!("{}/wp-json/wp/v2", url.trim_end_matches('/'));
        
        match self.http_client.get(&test_url).send().await {
            Ok(response) => {
                if response.status().is_success() || response.status().as_u16() == 401 {
                    // 200 OK or 401 Unauthorized both indicate the API endpoint exists
                    Ok(())
                } else {
                    Err(Error::Config(format!(
                        "WordPress API endpoint not accessible. Status: {}",
                        response.status()
                    )))
                }
            },
            Err(e) => Err(Error::Config(format!(
                "Failed to connect to WordPress site: {}",
                e
            ))),
        }
    }

    async fn test_wordpress_api(&self, config: &WordPressConfig) -> Result<(), Error> {
        let api_url = format!("{}/wp-json/wp/v2", config.url.trim_end_matches('/'));
        
        match self.http_client.get(&api_url).send().await {
            Ok(response) => {
                if response.status().is_success() || response.status().as_u16() == 401 {
                    Ok(())
                } else {
                    Err(Error::Config(format!(
                        "WordPress REST API not available. Status: {}. Make sure WordPress REST API is enabled.",
                        response.status()
                    )))
                }
            },
            Err(e) => Err(Error::Config(format!(
                "Failed to access WordPress REST API: {}",
                e
            ))),
        }
    }

    async fn test_wordpress_auth(&self, config: &WordPressConfig) -> Result<(), Error> {
        let auth_test_url = format!("{}/wp-json/wp/v2/users/me", config.url.trim_end_matches('/'));
        
        let response = self.http_client
            .get(&auth_test_url)
            .basic_auth(&config.username, Some(&config.password))
            .send()
            .await
            .map_err(|e| Error::Config(format!("Authentication test failed: {}", e)))?;

        match response.status() {
            reqwest::StatusCode::OK => {
                // Try to parse user info
                match response.json::<Value>().await {
                    Ok(_user_info) => Ok(()),
                    Err(e) => Err(Error::Config(format!(
                        "Authentication successful but failed to parse user info: {}",
                        e
                    ))),
                }
            },
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(Error::Config(
                    "Authentication failed. Please check your username and Application Password.".to_string()
                ))
            },
            reqwest::StatusCode::FORBIDDEN => {
                Err(Error::Config(
                    "Access forbidden. The user may not have sufficient permissions.".to_string()
                ))
            },
            status => {
                Err(Error::Config(format!(
                    "Unexpected response during authentication test: {}",
                    status
                )))
            },
        }
    }

    async fn get_wordpress_version(&self, config: &WordPressConfig) -> Result<String, Error> {
        let version_url = format!("{}/wp-json/", config.url.trim_end_matches('/'));
        
        match self.http_client.get(&version_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<Value>().await {
                        Ok(json) => {
                            if let Some(_gmt_offset) = json.get("gmt_offset") {
                                // This is a rough indication that we're talking to WordPress
                                Ok("WordPress (version detection limited)".to_string())
                            } else {
                                Ok("Unknown WordPress version".to_string())
                            }
                        },
                        Err(_) => Ok("Unknown WordPress version".to_string()),
                    }
                } else {
                    Ok("Unknown WordPress version".to_string())
                }
            },
            Err(_) => Ok("Unknown WordPress version".to_string()),
        }
    }

    /// Validate server configuration
    pub fn validate_server_config(&self, bind_addr: &str, stdio: bool) -> Result<(), Error> {
        if !stdio {
            // Validate bind address format
            match bind_addr.parse::<std::net::SocketAddr>() {
                Ok(_) => Ok(()),
                Err(_) => {
                    // Try parsing as "host:port"
                    if bind_addr.contains(':') {
                        Ok(()) // Basic validation passed
                    } else {
                        Err(Error::Config(format!(
                            "Invalid bind address format: {}. Use format like '127.0.0.1:8080'",
                            bind_addr
                        )))
                    }
                }
            }
        } else {
            Ok(()) // STDIO mode doesn't need bind address validation
        }
    }

    /// Test if a port is available
    pub async fn test_port_availability(&self, bind_addr: &str) -> Result<bool, Error> {
        use tokio::net::TcpListener;
        
        match TcpListener::bind(bind_addr).await {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    Ok(false) // Port is in use
                } else {
                    Err(Error::Config(format!("Failed to test port availability: {}", e)))
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct WordPressConnectionInfo {
    pub url: String,
    pub version: String,
    pub api_available: bool,
    pub auth_valid: bool,
}

impl Default for ConfigValidator {
    fn default() -> Self {
        Self::new()
    }
}