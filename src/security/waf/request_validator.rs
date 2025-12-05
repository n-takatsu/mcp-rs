//! Request Validation Implementation

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Request limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLimitsConfig {
    /// Maximum request body size (bytes)
    pub max_body_size: usize,
    /// Maximum URL length
    pub max_url_length: usize,
    /// Maximum number of headers
    pub max_headers: usize,
    /// Maximum header value length
    pub max_header_length: usize,
    /// Allowed HTTP methods
    pub allowed_methods: Vec<String>,
    /// Allowed content types
    pub allowed_content_types: Vec<String>,
}

impl Default for RequestLimitsConfig {
    fn default() -> Self {
        Self {
            max_body_size: 10 * 1024 * 1024, // 10 MB
            max_url_length: 2048,
            max_headers: 100,
            max_header_length: 8192,
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "PATCH".to_string(),
                "OPTIONS".to_string(),
                "HEAD".to_string(),
            ],
            allowed_content_types: vec![
                "application/json".to_string(),
                "application/x-www-form-urlencoded".to_string(),
                "multipart/form-data".to_string(),
                "text/plain".to_string(),
            ],
        }
    }
}

/// File upload configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadConfig {
    /// Enable file upload validation
    pub enabled: bool,
    /// Maximum file size (bytes)
    pub max_file_size: usize,
    /// Allowed MIME types
    pub allowed_mime_types: Vec<String>,
    /// Disallowed file extensions
    pub blocked_extensions: Vec<String>,
    /// Enable virus scanning integration
    pub enable_virus_scan: bool,
}

impl Default for FileUploadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_file_size: 5 * 1024 * 1024, // 5 MB
            allowed_mime_types: vec![
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/gif".to_string(),
                "application/pdf".to_string(),
            ],
            blocked_extensions: vec![
                "exe".to_string(),
                "bat".to_string(),
                "cmd".to_string(),
                "sh".to_string(),
                "dll".to_string(),
                "so".to_string(),
            ],
            enable_virus_scan: false,
        }
    }
}

/// Request validator
pub struct RequestValidator {
    limits_config: RequestLimitsConfig,
    upload_config: FileUploadConfig,
    allowed_methods_set: HashSet<String>,
    allowed_content_types_set: HashSet<String>,
    blocked_extensions_set: HashSet<String>,
}

impl RequestValidator {
    /// Create a new request validator
    pub fn new(limits_config: RequestLimitsConfig, upload_config: FileUploadConfig) -> Self {
        let allowed_methods_set = limits_config
            .allowed_methods
            .iter()
            .map(|m| m.to_uppercase())
            .collect();

        let allowed_content_types_set = limits_config
            .allowed_content_types
            .iter()
            .cloned()
            .collect();

        let blocked_extensions_set = upload_config
            .blocked_extensions
            .iter()
            .map(|e| e.to_lowercase())
            .collect();

        Self {
            limits_config,
            upload_config,
            allowed_methods_set,
            allowed_content_types_set,
            blocked_extensions_set,
        }
    }

    /// Validate request method
    pub fn validate_method(&self, method: &str) -> Result<(), super::WafError> {
        let method_upper = method.to_uppercase();
        if !self.allowed_methods_set.contains(&method_upper) {
            return Err(super::WafError::RequestValidationFailed(format!(
                "HTTP method '{}' not allowed",
                method
            )));
        }
        Ok(())
    }

    /// Validate request body size
    pub fn validate_body_size(&self, size: usize) -> Result<(), super::WafError> {
        if size > self.limits_config.max_body_size {
            return Err(super::WafError::RequestValidationFailed(format!(
                "Request body size {} exceeds maximum of {} bytes",
                size, self.limits_config.max_body_size
            )));
        }
        Ok(())
    }

    /// Validate URL length
    pub fn validate_url_length(&self, url: &str) -> Result<(), super::WafError> {
        if url.len() > self.limits_config.max_url_length {
            return Err(super::WafError::RequestValidationFailed(format!(
                "URL length {} exceeds maximum of {} characters",
                url.len(),
                self.limits_config.max_url_length
            )));
        }
        Ok(())
    }

    /// Validate number of headers
    pub fn validate_header_count(&self, count: usize) -> Result<(), super::WafError> {
        if count > self.limits_config.max_headers {
            return Err(super::WafError::RequestValidationFailed(format!(
                "Header count {} exceeds maximum of {}",
                count, self.limits_config.max_headers
            )));
        }
        Ok(())
    }

    /// Validate header value length
    pub fn validate_header_length(&self, value: &str) -> Result<(), super::WafError> {
        if value.len() > self.limits_config.max_header_length {
            return Err(super::WafError::RequestValidationFailed(format!(
                "Header value length {} exceeds maximum of {} characters",
                value.len(),
                self.limits_config.max_header_length
            )));
        }
        Ok(())
    }

    /// Validate content type
    pub fn validate_content_type(&self, content_type: &str) -> Result<(), super::WafError> {
        // Extract base content type (before semicolon)
        let base_type = content_type
            .split(';')
            .next()
            .unwrap_or(content_type)
            .trim();

        if !self.allowed_content_types_set.contains(base_type) {
            return Err(super::WafError::RequestValidationFailed(format!(
                "Content-Type '{}' not allowed",
                content_type
            )));
        }
        Ok(())
    }

    /// Validate file upload
    pub fn validate_file_upload(
        &self,
        filename: &str,
        mime_type: &str,
        size: usize,
    ) -> Result<(), super::WafError> {
        if !self.upload_config.enabled {
            return Ok(());
        }

        // Check file size
        if size > self.upload_config.max_file_size {
            return Err(super::WafError::FileUploadRejected(format!(
                "File size {} exceeds maximum of {} bytes",
                size, self.upload_config.max_file_size
            )));
        }

        // Check file extension
        if let Some(extension) = filename.rsplit('.').next() {
            if self
                .blocked_extensions_set
                .contains(&extension.to_lowercase())
            {
                return Err(super::WafError::FileUploadRejected(format!(
                    "File extension '.{}' is not allowed",
                    extension
                )));
            }
        }

        // Check MIME type
        if !self.upload_config.allowed_mime_types.is_empty()
            && !self
                .upload_config
                .allowed_mime_types
                .contains(&mime_type.to_string())
        {
            return Err(super::WafError::FileUploadRejected(format!(
                "MIME type '{}' not allowed",
                mime_type
            )));
        }

        Ok(())
    }

    /// Validate complete request
    pub async fn validate_request(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body_size: usize,
        content_type: Option<&str>,
    ) -> Result<(), super::WafError> {
        // Validate method
        self.validate_method(method)?;

        // Validate URL length
        self.validate_url_length(url)?;

        // Validate header count
        self.validate_header_count(headers.len())?;

        // Validate each header length
        for (_, value) in headers {
            self.validate_header_length(value)?;
        }

        // Validate body size
        self.validate_body_size(body_size)?;

        // Validate content type if present
        if let Some(ct) = content_type {
            self.validate_content_type(ct)?;
        }

        Ok(())
    }

    /// Get limits configuration
    pub fn limits_config(&self) -> &RequestLimitsConfig {
        &self.limits_config
    }

    /// Get upload configuration
    pub fn upload_config(&self) -> &FileUploadConfig {
        &self.upload_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_validator() -> RequestValidator {
        RequestValidator::new(RequestLimitsConfig::default(), FileUploadConfig::default())
    }

    #[test]
    fn test_validate_method_allowed() {
        let validator = create_validator();
        assert!(validator.validate_method("GET").is_ok());
        assert!(validator.validate_method("POST").is_ok());
        assert!(validator.validate_method("put").is_ok()); // Case insensitive
    }

    #[test]
    fn test_validate_method_not_allowed() {
        let validator = create_validator();
        assert!(validator.validate_method("TRACE").is_err());
        assert!(validator.validate_method("CONNECT").is_err());
    }

    #[test]
    fn test_validate_body_size() {
        let validator = create_validator();
        assert!(validator.validate_body_size(1024).is_ok());
        assert!(validator.validate_body_size(10 * 1024 * 1024).is_ok()); // Exactly max
        assert!(validator.validate_body_size(10 * 1024 * 1024 + 1).is_err()); // Over max
    }

    #[test]
    fn test_validate_url_length() {
        let validator = create_validator();
        let short_url = "https://example.com/api";
        let long_url = "x".repeat(3000);

        assert!(validator.validate_url_length(short_url).is_ok());
        assert!(validator.validate_url_length(&long_url).is_err());
    }

    #[test]
    fn test_validate_header_count() {
        let validator = create_validator();
        assert!(validator.validate_header_count(50).is_ok());
        assert!(validator.validate_header_count(100).is_ok()); // Exactly max
        assert!(validator.validate_header_count(101).is_err()); // Over max
    }

    #[test]
    fn test_validate_header_length() {
        let validator = create_validator();
        let short_value = "short value";
        let long_value = "x".repeat(10000);

        assert!(validator.validate_header_length(short_value).is_ok());
        assert!(validator.validate_header_length(&long_value).is_err());
    }

    #[test]
    fn test_validate_content_type() {
        let validator = create_validator();
        assert!(validator.validate_content_type("application/json").is_ok());
        assert!(validator
            .validate_content_type("application/json; charset=utf-8")
            .is_ok());
        assert!(validator.validate_content_type("application/xml").is_err());
    }

    #[test]
    fn test_validate_file_upload_size() {
        let validator = create_validator();
        assert!(validator
            .validate_file_upload("test.jpg", "image/jpeg", 1024)
            .is_ok());
        assert!(validator
            .validate_file_upload("large.jpg", "image/jpeg", 10 * 1024 * 1024)
            .is_err());
    }

    #[test]
    fn test_validate_file_upload_extension() {
        let validator = create_validator();
        assert!(validator
            .validate_file_upload("test.jpg", "image/jpeg", 1024)
            .is_ok());
        assert!(validator
            .validate_file_upload("malware.exe", "application/octet-stream", 1024)
            .is_err());
        assert!(validator
            .validate_file_upload("script.sh", "text/x-shellscript", 1024)
            .is_err());
    }

    #[test]
    fn test_validate_file_upload_mime_type() {
        let validator = create_validator();
        assert!(validator
            .validate_file_upload("test.jpg", "image/jpeg", 1024)
            .is_ok());
        assert!(validator
            .validate_file_upload("test.png", "image/png", 1024)
            .is_ok());
        assert!(validator
            .validate_file_upload("test.txt", "text/plain", 1024)
            .is_err());
    }

    #[tokio::test]
    async fn test_validate_complete_request() {
        let validator = create_validator();
        let headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Authorization".to_string(), "Bearer token123".to_string()),
        ];

        let result = validator
            .validate_request(
                "POST",
                "https://example.com/api",
                &headers,
                1024,
                Some("application/json"),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_complete_request_failure() {
        let validator = create_validator();
        let headers = vec![("Content-Type".to_string(), "x".repeat(10000))];

        let result = validator
            .validate_request("POST", "https://example.com/api", &headers, 1024, None)
            .await;

        assert!(result.is_err());
    }
}
