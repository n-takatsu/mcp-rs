# WAF (Web Application Firewall) Implementation Design

**Issue**: #76
**Branch**: feature/waf-implementation
**Priority**: High
**Estimated Duration**: 12 days (2.5 weeks)

## 📋 Overview

Comprehensive WAF implementation to protect against application-layer attacks. Currently, only XSS/SQL injection detection exists; full WAF capabilities are missing.

## 🎯 Implementation Scope

### Phase 1: CORS Implementation (2 days)
- Origin validation
- Preflight request handling
- Credential-enabled CORS
- Configuration management

### Phase 2: CSP Implementation (2 days)
- CSP header generation
- Directive management
- Nonce generation for inline scripts
- Report URI configuration

### Phase 3: Request Validation (3 days)
- Request body size limits
- HTTP method restrictions
- Content-Type validation
- File upload validation
  - MIME type checking
  - File size limits
  - Malware scanning integration hooks

### Phase 4: Security Headers (1 day)
- X-Content-Type-Options
- X-Frame-Options
- X-XSS-Protection
- Strict-Transport-Security (HSTS)
- Referrer-Policy

### Phase 5: Enhanced Rate Limiting (2 days)
- Per-endpoint rate limiting
- IP-based rate limiting (extension)
- User-based rate limiting
- Dynamic rate adjustment

### Phase 6: Integration & Testing (2 days)
- Middleware integration
- Comprehensive test suite
- Performance validation
- Documentation

## 🏗️ Architecture

```
src/security/
├── waf/
│   ├── mod.rs              # WAF main module
│   ├── cors.rs             # CORS functionality
│   ├── csp.rs              # Content Security Policy
│   ├── request_validator.rs # Request validation
│   ├── security_headers.rs  # Security header management
│   └── rate_limiter.rs     # Enhanced rate limiting
├── mod.rs                  # Re-exports
└── [existing modules]

src/server/
└── middleware/
    ├── mod.rs
    └── waf_middleware.rs   # WAF middleware integration
```

## 📊 Data Structures

### WAF Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WafConfig {
    pub enabled: bool,
    pub cors: CorsConfig,
    pub csp: CspConfig,
    pub request_limits: RequestLimitsConfig,
    pub security_headers: SecurityHeadersConfig,
    pub rate_limiting: RateLimitingConfig,
    pub audit_logging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    pub enabled: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub exposed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CspConfig {
    pub enabled: bool,
    pub default_src: Vec<String>,
    pub script_src: Vec<String>,
    pub style_src: Vec<String>,
    pub img_src: Vec<String>,
    pub connect_src: Vec<String>,
    pub font_src: Vec<String>,
    pub object_src: Vec<String>,
    pub media_src: Vec<String>,
    pub frame_src: Vec<String>,
    pub report_uri: Option<String>,
    pub use_nonce: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLimitsConfig {
    pub max_body_size: usize,          // bytes
    pub allowed_methods: Vec<String>,
    pub allowed_content_types: Vec<String>,
    pub file_upload: FileUploadConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadConfig {
    pub enabled: bool,
    pub max_file_size: usize,          // bytes
    pub allowed_mime_types: Vec<String>,
    pub scan_for_malware: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityHeadersConfig {
    pub x_content_type_options: bool,
    pub x_frame_options: String,       // DENY, SAMEORIGIN, ALLOW-FROM
    pub x_xss_protection: String,      // 0, 1, 1; mode=block
    pub strict_transport_security: Option<HstsConfig>,
    pub referrer_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HstsConfig {
    pub max_age: u32,
    pub include_subdomains: bool,
    pub preload: bool,
}
```

## 🔒 Security Considerations

### CORS
- Strict origin validation
- Wildcard restrictions
- Credential handling
- Cache control

### CSP
- Nonce rotation per request
- Strict default policies
- Report-only mode for testing
- Violation reporting

### Request Validation
- Early rejection of oversized requests
- Stream processing for large bodies
- Async validation
- DoS protection

### Rate Limiting
- Distributed rate limiting support
- Sliding window algorithm
- Burst handling
- IP whitelist/blacklist

## 🎨 Implementation Details

### CORS Handler

```rust
pub struct CorsHandler {
    config: CorsConfig,
}

impl CorsHandler {
    pub fn new(config: CorsConfig) -> Self {
        Self { config }
    }

    pub fn validate_origin(&self, origin: &str) -> Result<bool, WafError> {
        // Validate origin against allowed list
    }

    pub fn handle_preflight(&self, request: &Request) -> Response {
        // Handle OPTIONS preflight request
    }

    pub fn add_cors_headers(&self, response: &mut Response, origin: &str) {
        // Add appropriate CORS headers
    }
}
```

### CSP Generator

```rust
pub struct CspGenerator {
    config: CspConfig,
}

impl CspGenerator {
    pub fn generate_nonce(&self) -> String {
        // Generate cryptographically secure nonce
    }

    pub fn build_header(&self, nonce: Option<&str>) -> String {
        // Build CSP header string
    }

    pub fn parse_violation_report(&self, report: &str) -> CspViolation {
        // Parse CSP violation reports
    }
}
```

### Request Validator

```rust
pub struct RequestValidator {
    config: RequestLimitsConfig,
}

impl RequestValidator {
    pub async fn validate_request(&self, request: &Request) -> Result<(), WafError> {
        self.validate_method(request)?;
        self.validate_content_type(request)?;
        self.validate_body_size(request).await?;
        Ok(())
    }

    pub async fn validate_file_upload(&self, file: &UploadedFile) -> Result<(), WafError> {
        self.validate_file_size(file)?;
        self.validate_mime_type(file)?;
        if self.config.file_upload.scan_for_malware {
            self.scan_file(file).await?;
        }
        Ok(())
    }
}
```

## 📈 Performance Targets

- **CORS validation**: < 0.1ms per request
- **CSP generation**: < 0.5ms per request
- **Request validation**: < 1ms per request
- **Overall WAF overhead**: < 5ms per request
- **Memory overhead**: < 10MB per instance

## ✅ Testing Strategy

### Unit Tests
- [ ] CORS origin validation
- [ ] CSP header generation
- [ ] Nonce generation uniqueness
- [ ] Request size validation
- [ ] File upload validation
- [ ] Security header generation

### Integration Tests
- [ ] Full request/response cycle
- [ ] Preflight handling
- [ ] Multi-origin scenarios
- [ ] Large file uploads
- [ ] Rate limiting integration

### Security Tests
- [ ] CORS bypass attempts
- [ ] CSP policy violations
- [ ] Oversized request handling
- [ ] Malicious file uploads
- [ ] Header injection attempts

### Performance Tests
- [ ] Benchmark CORS validation
- [ ] Benchmark CSP generation
- [ ] Benchmark request validation
- [ ] Load testing with WAF enabled

## 📝 Configuration Example

```toml
[waf]
enabled = true
audit_logging = true

[waf.cors]
enabled = true
allowed_origins = ["https://example.com", "https://app.example.com"]
allowed_methods = ["GET", "POST", "PUT", "DELETE"]
allowed_headers = ["Content-Type", "Authorization"]
exposed_headers = ["X-Request-ID"]
allow_credentials = true
max_age = 86400

[waf.csp]
enabled = true
default_src = ["'self'"]
script_src = ["'self'", "'nonce-{NONCE}'"]
style_src = ["'self'", "'unsafe-inline'"]
img_src = ["'self'", "data:", "https:"]
connect_src = ["'self'"]
report_uri = "/csp-violation-report"
use_nonce = true

[waf.request_limits]
max_body_size = 10485760  # 10MB
allowed_methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]
allowed_content_types = ["application/json", "application/x-www-form-urlencoded", "multipart/form-data"]

[waf.request_limits.file_upload]
enabled = true
max_file_size = 5242880  # 5MB
allowed_mime_types = ["image/jpeg", "image/png", "image/gif", "application/pdf"]
scan_for_malware = false  # Requires external integration

[waf.security_headers]
x_content_type_options = true
x_frame_options = "SAMEORIGIN"
x_xss_protection = "1; mode=block"
referrer_policy = "strict-origin-when-cross-origin"

[waf.security_headers.strict_transport_security]
max_age = 31536000
include_subdomains = true
preload = false
```

## 🚀 Deployment Checklist

- [ ] Configuration validated
- [ ] CORS origins configured for production
- [ ] CSP policies tested in report-only mode
- [ ] Rate limits tuned for expected traffic
- [ ] Monitoring alerts configured
- [ ] Documentation updated
- [ ] Security team review completed

## 📚 References

- [OWASP WAF Best Practices](https://owasp.org/www-community/Web_Application_Firewall)
- [MDN CORS Documentation](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS)
- [CSP Level 3 Specification](https://www.w3.org/TR/CSP3/)
- [OWASP Secure Headers Project](https://owasp.org/www-project-secure-headers/)
