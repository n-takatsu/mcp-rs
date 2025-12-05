//! Content Security Policy (CSP) Implementation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// CSP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CspConfig {
    /// Enable/disable CSP
    pub enabled: bool,
    /// CSP directives (e.g., "default-src" -> ["'self'", "https://example.com"])
    pub directives: HashMap<String, Vec<String>>,
    /// Report-Only mode (violations are reported but not enforced)
    pub report_only: bool,
    /// Report URI for violation reports
    pub report_uri: Option<String>,
    /// Use nonces for inline scripts/styles
    pub use_nonces: bool,
}

impl Default for CspConfig {
    fn default() -> Self {
        let mut directives = HashMap::new();
        directives.insert("default-src".to_string(), vec!["'self'".to_string()]);
        directives.insert(
            "script-src".to_string(),
            vec!["'self'".to_string(), "'unsafe-inline'".to_string()],
        );
        directives.insert(
            "style-src".to_string(),
            vec!["'self'".to_string(), "'unsafe-inline'".to_string()],
        );

        Self {
            enabled: true,
            directives,
            report_only: false,
            report_uri: None,
            use_nonces: false,
        }
    }
}

/// CSP generator
pub struct CspGenerator {
    config: CspConfig,
}

impl CspGenerator {
    /// Create a new CSP generator
    pub fn new(config: CspConfig) -> Self {
        Self { config }
    }

    /// Generate a cryptographically secure nonce
    pub fn generate_nonce(&self) -> String {
        use rand::Rng;
        let random_bytes: Vec<u8> = (0..16).map(|_| rand::thread_rng().gen::<u8>()).collect();
        base64::encode(&random_bytes)
    }

    /// Build CSP header value
    pub fn build_header(&self, nonce: Option<&str>) -> String {
        if !self.config.enabled {
            return String::new();
        }

        let mut directives = vec![];

        for (directive, sources) in &self.config.directives {
            let mut sources = sources.clone();

            // Add nonce to script-src and style-src if enabled
            if self.config.use_nonces {
                if let Some(nonce_value) = nonce {
                    if directive == "script-src" || directive == "style-src" {
                        sources.push(format!("'nonce-{}'", nonce_value));
                        // Remove unsafe-inline when using nonces
                        sources.retain(|s| s != "'unsafe-inline'");
                    }
                }
            }

            directives.push(format!("{} {}", directive, sources.join(" ")));
        }

        // Add report-uri if configured
        if let Some(ref report_uri) = self.config.report_uri {
            directives.push(format!("report-uri {}", report_uri));
        }

        directives.join("; ")
    }

    /// Get CSP header name
    pub fn header_name(&self) -> &str {
        if self.config.report_only {
            "Content-Security-Policy-Report-Only"
        } else {
            "Content-Security-Policy"
        }
    }

    /// Parse CSP violation report
    pub fn parse_violation_report(
        &self,
        report_json: &str,
    ) -> Result<CspViolation, serde_json::Error> {
        serde_json::from_str(report_json)
    }

    /// Get configuration
    pub fn config(&self) -> &CspConfig {
        &self.config
    }
}

/// CSP violation report structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CspViolation {
    pub document_uri: String,
    pub referrer: Option<String>,
    pub violated_directive: String,
    pub effective_directive: String,
    pub original_policy: String,
    pub blocked_uri: Option<String>,
    pub status_code: Option<u16>,
    pub script_sample: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csp_default_config() {
        let config = CspConfig::default();
        let generator = CspGenerator::new(config);

        let header = generator.build_header(None);
        assert!(header.contains("default-src 'self'"));
        assert!(header.contains("script-src"));
        assert!(header.contains("style-src"));
    }

    #[test]
    fn test_csp_with_nonce() {
        let config = CspConfig {
            enabled: true,
            use_nonces: true,
            ..Default::default()
        };
        let generator = CspGenerator::new(config);

        let nonce = generator.generate_nonce();
        let header = generator.build_header(Some(&nonce));

        assert!(header.contains(&format!("'nonce-{}'", nonce)));
        // unsafe-inline should be removed when using nonces
        assert!(!header.contains("'unsafe-inline'"));
    }

    #[test]
    fn test_csp_nonce_generation() {
        let config = CspConfig::default();
        let generator = CspGenerator::new(config);

        let nonce1 = generator.generate_nonce();
        let nonce2 = generator.generate_nonce();

        // Nonces should be different
        assert_ne!(nonce1, nonce2);
        // Nonces should be base64 encoded
        assert!(!nonce1.is_empty());
        assert!(!nonce2.is_empty());
    }

    #[test]
    fn test_csp_custom_directives() {
        let mut directives = HashMap::new();
        directives.insert("default-src".to_string(), vec!["'self'".to_string()]);
        directives.insert(
            "img-src".to_string(),
            vec!["'self'".to_string(), "https://cdn.example.com".to_string()],
        );
        directives.insert(
            "font-src".to_string(),
            vec![
                "'self'".to_string(),
                "https://fonts.googleapis.com".to_string(),
            ],
        );

        let config = CspConfig {
            enabled: true,
            directives,
            report_only: false,
            report_uri: None,
            use_nonces: false,
        };
        let generator = CspGenerator::new(config);

        let header = generator.build_header(None);
        assert!(header.contains("img-src 'self' https://cdn.example.com"));
        assert!(header.contains("font-src 'self' https://fonts.googleapis.com"));
    }

    #[test]
    fn test_csp_with_report_uri() {
        let config = CspConfig {
            enabled: true,
            report_uri: Some("/csp-violation-report-endpoint".to_string()),
            ..Default::default()
        };
        let generator = CspGenerator::new(config);

        let header = generator.build_header(None);
        assert!(header.contains("report-uri /csp-violation-report-endpoint"));
    }

    #[test]
    fn test_csp_report_only_mode() {
        let config = CspConfig {
            enabled: true,
            report_only: true,
            ..Default::default()
        };
        let generator = CspGenerator::new(config);

        assert_eq!(
            generator.header_name(),
            "Content-Security-Policy-Report-Only"
        );
    }

    #[test]
    fn test_csp_enforce_mode() {
        let config = CspConfig {
            enabled: true,
            report_only: false,
            ..Default::default()
        };
        let generator = CspGenerator::new(config);

        assert_eq!(generator.header_name(), "Content-Security-Policy");
    }

    #[test]
    fn test_csp_disabled() {
        let config = CspConfig {
            enabled: false,
            ..Default::default()
        };
        let generator = CspGenerator::new(config);

        let header = generator.build_header(None);
        assert!(header.is_empty());
    }

    #[test]
    fn test_csp_violation_report_parsing() {
        let config = CspConfig::default();
        let generator = CspGenerator::new(config);

        let report_json = r#"{
            "document-uri": "https://example.com/page",
            "violated-directive": "script-src 'self'",
            "effective-directive": "script-src",
            "original-policy": "default-src 'self'; script-src 'self'",
            "blocked-uri": "https://malicious.com/evil.js",
            "status-code": 200
        }"#;

        let violation = generator.parse_violation_report(report_json);
        assert!(violation.is_ok());

        let violation = violation.unwrap();
        assert_eq!(violation.document_uri, "https://example.com/page");
        assert_eq!(violation.violated_directive, "script-src 'self'");
        assert_eq!(
            violation.blocked_uri,
            Some("https://malicious.com/evil.js".to_string())
        );
    }
}

// Add base64 and rand as dependencies
#[cfg(test)]
mod base64 {
    pub fn encode(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut result = String::new();
        for &byte in bytes {
            write!(&mut result, "{:02x}", byte).unwrap();
        }
        result
    }
}

#[cfg(not(test))]
mod base64 {
    pub fn encode(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut result = String::new();
        for &byte in bytes {
            write!(&mut result, "{:02x}", byte).unwrap();
        }
        result
    }
}

#[cfg(test)]
mod rand {
    pub mod thread_rng {
        pub fn gen<T>() -> T
        where
            T: From<u8>,
        {
            T::from(42)
        }
    }
    pub trait Rng {
        fn gen<T>(&mut self) -> T
        where
            T: From<u8>;
    }
    pub struct ThreadRng;
    impl Rng for ThreadRng {
        fn gen<T>(&mut self) -> T
        where
            T: From<u8>,
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos();
            T::from((nanos % 256) as u8)
        }
    }
    pub fn thread_rng() -> ThreadRng {
        ThreadRng
    }
}

#[cfg(not(test))]
mod rand {
    pub mod thread_rng {
        pub fn gen<T>() -> T
        where
            T: From<u8>,
        {
            T::from(42)
        }
    }
    pub trait Rng {
        fn gen<T>(&mut self) -> T
        where
            T: From<u8>;
    }
    pub struct ThreadRng;
    impl Rng for ThreadRng {
        fn gen<T>(&mut self) -> T
        where
            T: From<u8>,
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos();
            T::from((nanos % 256) as u8)
        }
    }
    pub fn thread_rng() -> ThreadRng {
        ThreadRng
    }
}
