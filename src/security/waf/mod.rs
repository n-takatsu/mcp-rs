//! Web Application Firewall (WAF) Module
//!
//! Provides comprehensive application-layer security including:
//! - CORS (Cross-Origin Resource Sharing)
//! - CSP (Content Security Policy)
//! - Request validation and limits
//! - Security headers
//! - Enhanced rate limiting

pub mod cors;
pub mod csp;
pub mod request_validator;
pub mod security_headers;

use serde::{Deserialize, Serialize};
use std::fmt;

// Re-export types from submodules
pub use cors::{CorsConfig, CorsHandler};
pub use csp::{CspConfig, CspGenerator, CspViolation};
pub use request_validator::{FileUploadConfig, RequestLimitsConfig, RequestValidator};
pub use security_headers::{HstsConfig, SecurityHeaderManager, SecurityHeadersConfig};

/// WAF configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafConfig {
    /// Enable/disable WAF
    pub enabled: bool,
    /// CORS configuration
    pub cors: cors::CorsConfig,
    /// CSP configuration
    pub csp: csp::CspConfig,
    /// Request limits configuration
    pub request_limits: request_validator::RequestLimitsConfig,
    /// File upload configuration
    pub file_upload: request_validator::FileUploadConfig,
    /// Security headers configuration
    pub security_headers: security_headers::SecurityHeadersConfig,
    /// Enable audit logging
    pub audit_logging: bool,
}

impl Default for WafConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cors: cors::CorsConfig::default(),
            csp: csp::CspConfig::default(),
            request_limits: request_validator::RequestLimitsConfig::default(),
            file_upload: request_validator::FileUploadConfig::default(),
            security_headers: security_headers::SecurityHeadersConfig::default(),
            audit_logging: true,
        }
    }
}

/// WAF errors
#[derive(Debug, Clone)]
pub enum WafError {
    /// CORS validation failed
    CorsViolation(String),
    /// CSP validation failed
    CspViolation(String),
    /// Request validation failed
    RequestValidationFailed(String),
    /// File upload validation failed
    FileUploadRejected(String),
    /// Configuration error
    ConfigurationError(String),
}

impl fmt::Display for WafError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WafError::CorsViolation(msg) => write!(f, "CORS violation: {}", msg),
            WafError::CspViolation(msg) => write!(f, "CSP violation: {}", msg),
            WafError::RequestValidationFailed(msg) => {
                write!(f, "Request validation failed: {}", msg)
            }
            WafError::FileUploadRejected(msg) => write!(f, "File upload rejected: {}", msg),
            WafError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for WafError {}

/// Web Application Firewall
pub struct WebApplicationFirewall {
    config: WafConfig,
    cors_handler: cors::CorsHandler,
    csp_generator: csp::CspGenerator,
    request_validator: request_validator::RequestValidator,
    header_manager: security_headers::SecurityHeaderManager,
}

impl WebApplicationFirewall {
    /// Create a new WAF instance
    pub fn new(config: WafConfig) -> Self {
        Self {
            cors_handler: cors::CorsHandler::new(config.cors.clone()),
            csp_generator: csp::CspGenerator::new(config.csp.clone()),
            request_validator: request_validator::RequestValidator::new(
                config.request_limits.clone(),
                config.file_upload.clone(),
            ),
            header_manager: security_headers::SecurityHeaderManager::new(
                config.security_headers.clone(),
            ),
            config,
        }
    }

    /// Check if WAF is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get CORS handler
    pub fn cors_handler(&self) -> &cors::CorsHandler {
        &self.cors_handler
    }

    /// Get CSP generator
    pub fn csp_generator(&self) -> &csp::CspGenerator {
        &self.csp_generator
    }

    /// Get request validator
    pub fn request_validator(&self) -> &request_validator::RequestValidator {
        &self.request_validator
    }

    /// Get security header manager
    pub fn header_manager(&self) -> &security_headers::SecurityHeaderManager {
        &self.header_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waf_creation() {
        let config = WafConfig::default();
        let waf = WebApplicationFirewall::new(config);
        assert!(waf.is_enabled());
    }

    #[test]
    fn test_waf_disabled() {
        let mut config = WafConfig::default();
        config.enabled = false;
        let waf = WebApplicationFirewall::new(config);
        assert!(!waf.is_enabled());
    }
}
