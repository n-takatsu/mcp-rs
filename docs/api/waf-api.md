# WAF (Web Application Firewall) API Documentation

## Overview

The WAF module provides comprehensive application-layer security features including CORS, CSP, request validation, and security headers.

## Core Components

### WebApplicationFirewall

Main coordinator for all WAF functionality.

```rust
use mcp_rs::security::waf::{WafConfig, WebApplicationFirewall};

let config = WafConfig::default();
let waf = WebApplicationFirewall::new(config);
```

### WafConfig

Central configuration structure for all WAF features.

```rust
pub struct WafConfig {
    pub enabled: bool,
    pub cors: CorsConfig,
    pub csp: CspConfig,
    pub request_limits: RequestLimitsConfig,
    pub file_upload: FileUploadConfig,
    pub security_headers: SecurityHeadersConfig,
    pub audit_logging: bool,
}
```

## CORS (Cross-Origin Resource Sharing)

### CorsConfig

```rust
pub struct CorsConfig {
    pub enabled: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub exposed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: u32,
}
```

### CorsHandler

```rust
use mcp_rs::security::waf::CorsHandler;

let handler = CorsHandler::new(cors_config);

// Validate origin
match handler.validate_origin("https://example.com") {
    Ok(_) => println!("Origin allowed"),
    Err(e) => eprintln!("Origin rejected: {}", e),
}

// Check method
if handler.is_method_allowed("POST") {
    // Process request
}

// Get CORS headers
let headers = handler.get_cors_headers("https://example.com");
```

### Features

- **Wildcard origins**: `"*"` allows all origins (not with credentials)
- **Specific origins**: `["https://example.com"]`
- **Subdomain wildcards**: `["*.example.com"]` matches `app.example.com` but not `example.com`
- **Method validation**: Configurable allowed HTTP methods
- **Header validation**: Control allowed request/response headers
- **Credentials support**: Enable/disable credential sharing

## CSP (Content Security Policy)

### CspConfig

```rust
pub struct CspConfig {
    pub enabled: bool,
    pub directives: HashMap<String, Vec<String>>,
    pub report_only: bool,
    pub report_uri: Option<String>,
    pub use_nonces: bool,
}
```

### CspGenerator

```rust
use mcp_rs::security::waf::CspGenerator;

let generator = CspGenerator::new(csp_config);

// Generate nonce for inline scripts
let nonce = generator.generate_nonce();

// Build CSP header
let csp_header = generator.build_header(Some(&nonce));
let header_name = generator.header_name(); // "Content-Security-Policy" or "-Report-Only"

// Add to response
response.add_header(header_name, csp_header);
```

### Features

- **Directive-based configuration**: Configure `default-src`, `script-src`, etc.
- **Nonce generation**: Cryptographically secure nonces for inline scripts/styles
- **Report-Only mode**: Test CSP without blocking
- **Violation reporting**: Parse and handle CSP violation reports

## Request Validation

### RequestLimitsConfig

```rust
pub struct RequestLimitsConfig {
    pub max_body_size: usize,
    pub max_url_length: usize,
    pub max_headers: usize,
    pub max_header_length: usize,
    pub allowed_methods: Vec<String>,
    pub allowed_content_types: Vec<String>,
}
```

### FileUploadConfig

```rust
pub struct FileUploadConfig {
    pub enabled: bool,
    pub max_file_size: usize,
    pub allowed_mime_types: Vec<String>,
    pub blocked_extensions: Vec<String>,
    pub enable_virus_scan: bool,
}
```

### RequestValidator

```rust
use mcp_rs::security::waf::RequestValidator;

let validator = RequestValidator::new(limits_config, upload_config);

// Validate individual aspects
validator.validate_method("POST")?;
validator.validate_body_size(1024)?;
validator.validate_url_length(url)?;
validator.validate_content_type("application/json")?;

// Validate file upload
validator.validate_file_upload("document.pdf", "application/pdf", 1024)?;

// Validate complete request
validator.validate_request(
    "POST",
    "https://api.example.com/users",
    &headers,
    body_size,
    Some("application/json")
).await?;
```

### Features

- **Body size limits**: Prevent memory exhaustion
- **URL length limits**: Prevent buffer overflow attacks
- **Header limits**: Control header count and size
- **Method whitelisting**: Only allow specific HTTP methods
- **Content-Type validation**: Restrict accepted content types
- **File upload validation**: Size, extension, and MIME type checks

## Security Headers

### SecurityHeadersConfig

```rust
pub struct SecurityHeadersConfig {
    pub hsts: HstsConfig,
    pub x_content_type_options: bool,
    pub x_frame_options: Option<String>,
    pub x_xss_protection: bool,
    pub referrer_policy: Option<String>,
    pub permissions_policy: Option<String>,
}

pub struct HstsConfig {
    pub enabled: bool,
    pub max_age: u32,
    pub include_subdomains: bool,
    pub preload: bool,
}
```

### SecurityHeaderManager

```rust
use mcp_rs::security::waf::SecurityHeaderManager;

let manager = SecurityHeaderManager::new(headers_config);

// Generate all headers at once
let headers = manager.generate_headers();
for (name, value) in headers {
    response.add_header(name, value);
}

// Or get individual headers
if let Some(hsts) = manager.get_hsts_header() {
    response.add_header("Strict-Transport-Security", hsts);
}
```

### Supported Headers

- **HSTS**: HTTP Strict Transport Security
- **X-Content-Type-Options**: Prevent MIME sniffing
- **X-Frame-Options**: Clickjacking protection
- **X-XSS-Protection**: Legacy XSS filter
- **Referrer-Policy**: Control referrer information
- **Permissions-Policy**: Feature policy (geolocation, camera, etc.)

## Error Handling

```rust
pub enum WafError {
    CorsViolation(String),
    CspViolation(String),
    RequestValidationFailed(String),
    FileUploadRejected(String),
    ConfigurationError(String),
}
```

All WAF errors implement `std::error::Error` and can be handled uniformly:

```rust
match waf_operation() {
    Ok(result) => process(result),
    Err(WafError::CorsViolation(msg)) => {
        // Return 403 Forbidden
    }
    Err(WafError::RequestValidationFailed(msg)) => {
        // Return 400 Bad Request
    }
    Err(e) => {
        // Log and return 500
    }
}
```

## Configuration Examples

### TOML Configuration

```toml
[waf]
enabled = true
audit_logging = true

[waf.cors]
enabled = true
allowed_origins = ["https://example.com", "*.example.com"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
allow_credentials = true
max_age = 86400

[waf.csp]
enabled = true
use_nonces = true
report_uri = "/csp-violation-report"

[waf.csp.directives]
default-src = ["'self'"]
script-src = ["'self'", "https://cdn.example.com"]
style-src = ["'self'", "https://cdn.example.com"]

[waf.request_limits]
max_body_size = 10485760  # 10 MB
max_url_length = 2048

[waf.file_upload]
enabled = true
max_file_size = 5242880  # 5 MB
allowed_mime_types = ["image/jpeg", "image/png", "application/pdf"]
blocked_extensions = ["exe", "bat", "sh"]

[waf.security_headers.hsts]
enabled = true
max_age = 31536000
include_subdomains = true
```

### Programmatic Configuration

```rust
use mcp_rs::security::waf::*;
use std::collections::HashMap;

let mut csp_directives = HashMap::new();
csp_directives.insert(
    "default-src".to_string(),
    vec!["'self'".to_string()]
);
csp_directives.insert(
    "script-src".to_string(),
    vec!["'self'".to_string(), "https://cdn.example.com".to_string()]
);

let config = WafConfig {
    enabled: true,
    cors: CorsConfig {
        enabled: true,
        allowed_origins: vec!["https://example.com".to_string()],
        allowed_methods: vec!["GET".to_string(), "POST".to_string()],
        allowed_headers: vec!["Content-Type".to_string()],
        exposed_headers: vec![],
        allow_credentials: true,
        max_age: 86400,
    },
    csp: CspConfig {
        enabled: true,
        directives: csp_directives,
        report_only: false,
        report_uri: Some("/csp-report".to_string()),
        use_nonces: true,
    },
    request_limits: RequestLimitsConfig::default(),
    file_upload: FileUploadConfig::default(),
    security_headers: SecurityHeadersConfig::default(),
    audit_logging: true,
};

let waf = WebApplicationFirewall::new(config);
```

## Performance Considerations

- **Target overhead**: < 5ms per request
- **Nonce generation**: Uses cryptographically secure random number generation
- **Header generation**: Cached configuration, minimal overhead
- **Validation**: Early rejection for invalid requests

## Best Practices

1. **Enable audit logging** in production to track security events
2. **Use CSP Report-Only mode** initially to test policies without breaking functionality
3. **Start with restrictive policies** and relax as needed
4. **Use nonces for CSP** instead of `'unsafe-inline'` when possible
5. **Keep allowed origins specific** - avoid wildcards in production
6. **Set appropriate file size limits** based on your use case
7. **Enable HSTS with preload** for production domains
8. **Monitor CSP violation reports** to detect attacks and misconfigurations

## Integration Example

```rust
use mcp_rs::security::waf::{WafConfig, WebApplicationFirewall};

async fn handle_request(
    waf: &WebApplicationFirewall,
    request: Request,
) -> Result<Response, Box<dyn std::error::Error>> {
    // CORS validation
    if let Some(origin) = request.header("Origin") {
        waf.cors_handler().validate_origin(origin)?;
    }
    
    // Request validation
    let headers: Vec<_> = request.headers().collect();
    waf.request_validator().validate_request(
        request.method(),
        request.uri(),
        &headers,
        request.body_size(),
        request.header("Content-Type"),
    ).await?;
    
    // Process request
    let mut response = process_request(request).await?;
    
    // Add security headers
    for (name, value) in waf.header_manager().generate_headers() {
        response.add_header(name, value);
    }
    
    // Add CSP with nonce
    if waf.csp_generator().config().use_nonces {
        let nonce = waf.csp_generator().generate_nonce();
        let csp = waf.csp_generator().build_header(Some(&nonce));
        response.add_header(waf.csp_generator().header_name(), csp);
    }
    
    // Add CORS headers
    if let Some(origin) = request.header("Origin") {
        for (name, value) in waf.cors_handler().get_cors_headers(origin) {
            response.add_header(name, value);
        }
    }
    
    Ok(response)
}
```

## Testing

Run WAF tests:

```bash
cargo test --lib security::waf
```

Run demo:

```bash
cargo run --example waf_demo
```

## See Also

- [WAF Implementation Design](../design/waf-implementation.md)
- [Security Configuration Guide](../../examples/security_configuration_guide.rs)
- [OWASP Application Security](https://owasp.org/)
