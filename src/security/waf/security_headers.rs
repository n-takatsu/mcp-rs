//! Security Headers Implementation

use serde::{Deserialize, Serialize};

/// HSTS (HTTP Strict Transport Security) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HstsConfig {
    /// Enable HSTS
    pub enabled: bool,
    /// Max age in seconds
    pub max_age: u32,
    /// Include subdomains
    pub include_subdomains: bool,
    /// Preload directive
    pub preload: bool,
}

impl Default for HstsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age: 31536000, // 1 year
            include_subdomains: true,
            preload: false,
        }
    }
}

/// Security headers configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityHeadersConfig {
    /// HSTS configuration
    pub hsts: HstsConfig,
    /// X-Content-Type-Options
    pub x_content_type_options: bool,
    /// X-Frame-Options (DENY, SAMEORIGIN, or custom)
    pub x_frame_options: Option<String>,
    /// X-XSS-Protection
    pub x_xss_protection: bool,
    /// Referrer-Policy
    pub referrer_policy: Option<String>,
    /// Permissions-Policy (formerly Feature-Policy)
    pub permissions_policy: Option<String>,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            hsts: HstsConfig::default(),
            x_content_type_options: true,
            x_frame_options: Some("SAMEORIGIN".to_string()),
            x_xss_protection: true,
            referrer_policy: Some("strict-origin-when-cross-origin".to_string()),
            permissions_policy: Some("geolocation=(), microphone=(), camera=()".to_string()),
        }
    }
}

/// Security header manager
pub struct SecurityHeaderManager {
    config: SecurityHeadersConfig,
}

impl SecurityHeaderManager {
    /// Create a new security header manager
    pub fn new(config: SecurityHeadersConfig) -> Self {
        Self { config }
    }

    /// Generate all security headers
    pub fn generate_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![];

        // HSTS
        if self.config.hsts.enabled {
            let mut hsts_value = format!("max-age={}", self.config.hsts.max_age);
            if self.config.hsts.include_subdomains {
                hsts_value.push_str("; includeSubDomains");
            }
            if self.config.hsts.preload {
                hsts_value.push_str("; preload");
            }
            headers.push(("Strict-Transport-Security".to_string(), hsts_value));
        }

        // X-Content-Type-Options
        if self.config.x_content_type_options {
            headers.push(("X-Content-Type-Options".to_string(), "nosniff".to_string()));
        }

        // X-Frame-Options
        if let Some(ref value) = self.config.x_frame_options {
            headers.push(("X-Frame-Options".to_string(), value.clone()));
        }

        // X-XSS-Protection
        if self.config.x_xss_protection {
            headers.push(("X-XSS-Protection".to_string(), "1; mode=block".to_string()));
        }

        // Referrer-Policy
        if let Some(ref value) = self.config.referrer_policy {
            headers.push(("Referrer-Policy".to_string(), value.clone()));
        }

        // Permissions-Policy
        if let Some(ref value) = self.config.permissions_policy {
            headers.push(("Permissions-Policy".to_string(), value.clone()));
        }

        headers
    }

    /// Get HSTS header value
    pub fn get_hsts_header(&self) -> Option<String> {
        if !self.config.hsts.enabled {
            return None;
        }

        let mut value = format!("max-age={}", self.config.hsts.max_age);
        if self.config.hsts.include_subdomains {
            value.push_str("; includeSubDomains");
        }
        if self.config.hsts.preload {
            value.push_str("; preload");
        }
        Some(value)
    }

    /// Get X-Content-Type-Options header value
    pub fn get_x_content_type_options_header(&self) -> Option<String> {
        if self.config.x_content_type_options {
            Some("nosniff".to_string())
        } else {
            None
        }
    }

    /// Get X-Frame-Options header value
    pub fn get_x_frame_options_header(&self) -> Option<String> {
        self.config.x_frame_options.clone()
    }

    /// Get X-XSS-Protection header value
    pub fn get_x_xss_protection_header(&self) -> Option<String> {
        if self.config.x_xss_protection {
            Some("1; mode=block".to_string())
        } else {
            None
        }
    }

    /// Get Referrer-Policy header value
    pub fn get_referrer_policy_header(&self) -> Option<String> {
        self.config.referrer_policy.clone()
    }

    /// Get Permissions-Policy header value
    pub fn get_permissions_policy_header(&self) -> Option<String> {
        self.config.permissions_policy.clone()
    }

    /// Get configuration
    pub fn config(&self) -> &SecurityHeadersConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_security_headers() {
        let config = SecurityHeadersConfig::default();
        let manager = SecurityHeaderManager::new(config);

        let headers = manager.generate_headers();
        assert!(!headers.is_empty());

        // Check for essential security headers
        assert!(headers
            .iter()
            .any(|(k, _)| k == "Strict-Transport-Security"));
        assert!(headers.iter().any(|(k, _)| k == "X-Content-Type-Options"));
        assert!(headers.iter().any(|(k, _)| k == "X-Frame-Options"));
    }

    #[test]
    fn test_hsts_header_basic() {
        let config = SecurityHeadersConfig {
            hsts: HstsConfig {
                enabled: true,
                max_age: 31536000,
                include_subdomains: false,
                preload: false,
            },
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let hsts = manager.get_hsts_header();
        assert_eq!(hsts, Some("max-age=31536000".to_string()));
    }

    #[test]
    fn test_hsts_header_with_subdomains() {
        let config = SecurityHeadersConfig {
            hsts: HstsConfig {
                enabled: true,
                max_age: 31536000,
                include_subdomains: true,
                preload: false,
            },
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let hsts = manager.get_hsts_header();
        assert_eq!(
            hsts,
            Some("max-age=31536000; includeSubDomains".to_string())
        );
    }

    #[test]
    fn test_hsts_header_with_preload() {
        let config = SecurityHeadersConfig {
            hsts: HstsConfig {
                enabled: true,
                max_age: 31536000,
                include_subdomains: true,
                preload: true,
            },
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let hsts = manager.get_hsts_header();
        assert_eq!(
            hsts,
            Some("max-age=31536000; includeSubDomains; preload".to_string())
        );
    }

    #[test]
    fn test_hsts_disabled() {
        let config = SecurityHeadersConfig {
            hsts: HstsConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let hsts = manager.get_hsts_header();
        assert_eq!(hsts, None);
    }

    #[test]
    fn test_x_content_type_options() {
        let config = SecurityHeadersConfig::default();
        let manager = SecurityHeaderManager::new(config);

        let header = manager.get_x_content_type_options_header();
        assert_eq!(header, Some("nosniff".to_string()));
    }

    #[test]
    fn test_x_frame_options_sameorigin() {
        let config = SecurityHeadersConfig {
            x_frame_options: Some("SAMEORIGIN".to_string()),
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let header = manager.get_x_frame_options_header();
        assert_eq!(header, Some("SAMEORIGIN".to_string()));
    }

    #[test]
    fn test_x_frame_options_deny() {
        let config = SecurityHeadersConfig {
            x_frame_options: Some("DENY".to_string()),
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let header = manager.get_x_frame_options_header();
        assert_eq!(header, Some("DENY".to_string()));
    }

    #[test]
    fn test_x_xss_protection() {
        let config = SecurityHeadersConfig::default();
        let manager = SecurityHeaderManager::new(config);

        let header = manager.get_x_xss_protection_header();
        assert_eq!(header, Some("1; mode=block".to_string()));
    }

    #[test]
    fn test_referrer_policy() {
        let config = SecurityHeadersConfig {
            referrer_policy: Some("strict-origin-when-cross-origin".to_string()),
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let header = manager.get_referrer_policy_header();
        assert_eq!(header, Some("strict-origin-when-cross-origin".to_string()));
    }

    #[test]
    fn test_permissions_policy() {
        let config = SecurityHeadersConfig {
            permissions_policy: Some("geolocation=(), microphone=(), camera=()".to_string()),
            ..Default::default()
        };
        let manager = SecurityHeaderManager::new(config);

        let header = manager.get_permissions_policy_header();
        assert_eq!(
            header,
            Some("geolocation=(), microphone=(), camera=()".to_string())
        );
    }

    #[test]
    fn test_generate_all_headers() {
        let config = SecurityHeadersConfig::default();
        let manager = SecurityHeaderManager::new(config);

        let headers = manager.generate_headers();

        // Should have 6 headers by default
        assert_eq!(headers.len(), 6);

        let header_names: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(header_names.contains(&"Strict-Transport-Security"));
        assert!(header_names.contains(&"X-Content-Type-Options"));
        assert!(header_names.contains(&"X-Frame-Options"));
        assert!(header_names.contains(&"X-XSS-Protection"));
        assert!(header_names.contains(&"Referrer-Policy"));
        assert!(header_names.contains(&"Permissions-Policy"));
    }

    #[test]
    fn test_minimal_configuration() {
        let config = SecurityHeadersConfig {
            hsts: HstsConfig {
                enabled: false,
                ..Default::default()
            },
            x_content_type_options: false,
            x_frame_options: None,
            x_xss_protection: false,
            referrer_policy: None,
            permissions_policy: None,
        };
        let manager = SecurityHeaderManager::new(config);

        let headers = manager.generate_headers();
        assert!(headers.is_empty());
    }
}
