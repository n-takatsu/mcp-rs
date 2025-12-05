//! CORS (Cross-Origin Resource Sharing) Implementation

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Enable/disable CORS
    pub enabled: bool,
    /// Allowed origins (e.g., "https://example.com" or "*")
    pub allowed_origins: Vec<String>,
    /// Allowed HTTP methods
    pub allowed_methods: Vec<String>,
    /// Allowed headers
    pub allowed_headers: Vec<String>,
    /// Exposed headers
    pub exposed_headers: Vec<String>,
    /// Allow credentials (cookies, authorization headers)
    pub allow_credentials: bool,
    /// Max age for preflight cache (seconds)
    pub max_age: u32,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
            exposed_headers: vec![],
            allow_credentials: false,
            max_age: 86400, // 24 hours
        }
    }
}

/// CORS handler
pub struct CorsHandler {
    config: CorsConfig,
    allowed_origins_set: HashSet<String>,
}

impl CorsHandler {
    /// Create a new CORS handler
    pub fn new(config: CorsConfig) -> Self {
        let allowed_origins_set = config
            .allowed_origins
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        Self {
            config,
            allowed_origins_set,
        }
    }

    /// Validate origin against allowed list
    pub fn validate_origin(&self, origin: &str) -> Result<bool, super::WafError> {
        if !self.config.enabled {
            return Ok(true);
        }

        // Wildcard allows all origins (but not with credentials)
        if self.allowed_origins_set.contains("*") {
            if self.config.allow_credentials {
                return Err(super::WafError::CorsViolation(
                    "Cannot use wildcard origin with credentials enabled".to_string(),
                ));
            }
            return Ok(true);
        }

        // Check if origin is in allowed list
        if self.allowed_origins_set.contains(origin) {
            return Ok(true);
        }

        // Check for subdomain wildcards (e.g., "*.example.com")
        for allowed_origin in &self.config.allowed_origins {
            if let Some(domain) = allowed_origin.strip_prefix("*.") {
                // Extract hostname from origin (remove protocol)
                let hostname = origin
                    .strip_prefix("https://")
                    .or_else(|| origin.strip_prefix("http://"))
                    .unwrap_or(origin);

                // Must have at least one subdomain (e.g., "app.example.com" matches "*.example.com", but "example.com" does not)
                if hostname.ends_with(domain) && hostname.len() > domain.len() {
                    let prefix = &hostname[..hostname.len() - domain.len()];
                    // Check that there's actually a subdomain (ends with a dot)
                    if prefix.ends_with('.') && prefix.len() > 1 {
                        return Ok(true);
                    }
                }
            }
        }

        Err(super::WafError::CorsViolation(format!(
            "Origin '{}' not allowed",
            origin
        )))
    }

    /// Check if method is allowed
    pub fn is_method_allowed(&self, method: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        self.config
            .allowed_methods
            .iter()
            .any(|m| m.eq_ignore_ascii_case(method))
    }

    /// Check if header is allowed
    pub fn is_header_allowed(&self, header: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        self.config
            .allowed_headers
            .iter()
            .any(|h| h.eq_ignore_ascii_case(header))
    }

    /// Get CORS headers for a valid origin
    pub fn get_cors_headers(&self, origin: &str) -> Vec<(String, String)> {
        if !self.config.enabled {
            return vec![];
        }

        let mut headers = vec![];

        // Access-Control-Allow-Origin
        if self.allowed_origins_set.contains("*") && !self.config.allow_credentials {
            headers.push(("Access-Control-Allow-Origin".to_string(), "*".to_string()));
        } else {
            headers.push((
                "Access-Control-Allow-Origin".to_string(),
                origin.to_string(),
            ));
        }

        // Access-Control-Allow-Methods
        headers.push((
            "Access-Control-Allow-Methods".to_string(),
            self.config.allowed_methods.join(", "),
        ));

        // Access-Control-Allow-Headers
        if !self.config.allowed_headers.is_empty() {
            headers.push((
                "Access-Control-Allow-Headers".to_string(),
                self.config.allowed_headers.join(", "),
            ));
        }

        // Access-Control-Expose-Headers
        if !self.config.exposed_headers.is_empty() {
            headers.push((
                "Access-Control-Expose-Headers".to_string(),
                self.config.exposed_headers.join(", "),
            ));
        }

        // Access-Control-Allow-Credentials
        if self.config.allow_credentials {
            headers.push((
                "Access-Control-Allow-Credentials".to_string(),
                "true".to_string(),
            ));
        }

        // Access-Control-Max-Age
        headers.push((
            "Access-Control-Max-Age".to_string(),
            self.config.max_age.to_string(),
        ));

        headers
    }

    /// Get configuration
    pub fn config(&self) -> &CorsConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_wildcard_origin() {
        let config = CorsConfig {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allow_credentials: false,
            ..Default::default()
        };
        let handler = CorsHandler::new(config);

        assert!(handler.validate_origin("https://example.com").is_ok());
        assert!(handler.validate_origin("https://any-domain.com").is_ok());
    }

    #[test]
    fn test_cors_specific_origin() {
        let config = CorsConfig {
            enabled: true,
            allowed_origins: vec!["https://example.com".to_string()],
            ..Default::default()
        };
        let handler = CorsHandler::new(config);

        assert!(handler.validate_origin("https://example.com").is_ok());
        assert!(handler.validate_origin("https://malicious.com").is_err());
    }

    #[test]
    fn test_cors_subdomain_wildcard() {
        let config = CorsConfig {
            enabled: true,
            allowed_origins: vec!["*.example.com".to_string()],
            ..Default::default()
        };
        let handler = CorsHandler::new(config);

        assert!(handler.validate_origin("https://app.example.com").is_ok());
        assert!(handler.validate_origin("https://api.example.com").is_ok());
        assert!(handler.validate_origin("https://example.com").is_err());
        assert!(handler.validate_origin("https://malicious.com").is_err());
    }

    #[test]
    fn test_cors_wildcard_with_credentials_error() {
        let config = CorsConfig {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allow_credentials: true,
            ..Default::default()
        };
        let handler = CorsHandler::new(config);

        assert!(handler.validate_origin("https://example.com").is_err());
    }

    #[test]
    fn test_cors_method_validation() {
        let config = CorsConfig {
            enabled: true,
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            ..Default::default()
        };
        let handler = CorsHandler::new(config);

        assert!(handler.is_method_allowed("GET"));
        assert!(handler.is_method_allowed("POST"));
        assert!(!handler.is_method_allowed("DELETE"));
    }

    #[test]
    fn test_cors_header_validation() {
        let config = CorsConfig {
            enabled: true,
            allowed_headers: vec!["Content-Type".to_string(), "Authorization".to_string()],
            ..Default::default()
        };
        let handler = CorsHandler::new(config);

        assert!(handler.is_header_allowed("Content-Type"));
        assert!(handler.is_header_allowed("Authorization"));
        assert!(!handler.is_header_allowed("X-Custom-Header"));
    }

    #[test]
    fn test_cors_headers_generation() {
        let config = CorsConfig {
            enabled: true,
            allowed_origins: vec!["https://example.com".to_string()],
            allowed_methods: vec!["GET".to_string(), "POST".to_string()],
            allowed_headers: vec!["Content-Type".to_string()],
            exposed_headers: vec!["X-Request-ID".to_string()],
            allow_credentials: true,
            max_age: 3600,
        };
        let handler = CorsHandler::new(config);

        let headers = handler.get_cors_headers("https://example.com");
        assert!(!headers.is_empty());

        // Check for specific headers
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Access-Control-Allow-Origin" && v == "https://example.com"));
        assert!(headers
            .iter()
            .any(|(k, _)| k == "Access-Control-Allow-Credentials"));
    }

    #[test]
    fn test_cors_disabled() {
        let config = CorsConfig {
            enabled: false,
            ..Default::default()
        };
        let handler = CorsHandler::new(config);

        assert!(handler.validate_origin("https://any-origin.com").is_ok());
        assert!(handler.is_method_allowed("ANY_METHOD"));
        assert!(handler.is_header_allowed("Any-Header"));
        assert!(handler.get_cors_headers("https://example.com").is_empty());
    }
}
