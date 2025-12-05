//! WAF (Web Application Firewall) Demo
//!
//! This example demonstrates the WAF functionality including:
//! - CORS validation
//! - CSP header generation
//! - Request validation
//! - Security headers

use mcp_rs::security::waf::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== WAF Demo ===\n");

    // Create WAF with default configuration
    let config = WafConfig::default();
    let waf = WebApplicationFirewall::new(config);

    println!("✓ WAF initialized with default configuration");
    println!("  - WAF enabled: {}", waf.is_enabled());
    println!();

    // CORS Demo
    demo_cors(waf.cors_handler()).await?;
    println!();

    // CSP Demo
    demo_csp(waf.csp_generator()).await?;
    println!();

    // Request Validation Demo
    demo_request_validation(waf.request_validator()).await?;
    println!();

    // Security Headers Demo
    demo_security_headers(waf.header_manager()).await?;
    println!();

    println!("=== Demo Complete ===");

    Ok(())
}

async fn demo_cors(cors_handler: &CorsHandler) -> Result<(), Box<dyn std::error::Error>> {
    println!("--- CORS Demo ---");

    // Test valid origin
    let origin = "https://example.com";
    match cors_handler.validate_origin(origin) {
        Ok(_) => println!("✓ Origin '{}' is allowed", origin),
        Err(e) => println!("✗ Origin '{}' rejected: {}", origin, e),
    }

    // Test invalid origin (with custom config)
    let custom_config = CorsConfig {
        allowed_origins: vec!["https://trusted.com".to_string()],
        ..Default::default()
    };
    let custom_cors = CorsHandler::new(custom_config);

    let untrusted_origin = "https://malicious.com";
    match custom_cors.validate_origin(untrusted_origin) {
        Ok(_) => println!("✓ Origin '{}' is allowed", untrusted_origin),
        Err(e) => println!("✓ Origin '{}' correctly rejected: {}", untrusted_origin, e),
    }

    // Test method validation
    println!("\nMethod validation:");
    for method in &["GET", "POST", "DELETE", "TRACE"] {
        if cors_handler.is_method_allowed(method) {
            println!("  ✓ {} allowed", method);
        } else {
            println!("  ✗ {} not allowed", method);
        }
    }

    // Generate CORS headers
    println!("\nCORS headers for valid origin:");
    let headers = cors_handler.get_cors_headers(origin);
    for (name, value) in headers {
        println!("  {}: {}", name, value);
    }

    Ok(())
}

async fn demo_csp(csp_generator: &CspGenerator) -> Result<(), Box<dyn std::error::Error>> {
    println!("--- CSP Demo ---");

    // Generate CSP header without nonce
    let csp_header = csp_generator.build_header(None);
    println!("CSP Header (without nonce):");
    println!("  {}: {}", csp_generator.header_name(), csp_header);

    // Generate nonce and CSP header with nonce
    let config = CspConfig {
        use_nonces: true,
        ..Default::default()
    };
    let csp_with_nonce = CspGenerator::new(config);

    let nonce = csp_with_nonce.generate_nonce();
    let csp_header_with_nonce = csp_with_nonce.build_header(Some(&nonce));
    println!("\nCSP Header (with nonce):");
    println!("  Nonce: {}", nonce);
    println!("  {}: {}", csp_with_nonce.header_name(), csp_header_with_nonce);

    Ok(())
}

async fn demo_request_validation(
    validator: &RequestValidator,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Request Validation Demo ---");

    // Test method validation
    println!("Method validation:");
    for method in &["GET", "POST", "TRACE"] {
        match validator.validate_method(method) {
            Ok(_) => println!("  ✓ {} allowed", method),
            Err(e) => println!("  ✗ {} rejected: {}", method, e),
        }
    }

    // Test body size validation
    println!("\nBody size validation:");
    let test_sizes = vec![1024, 1024 * 1024, 10 * 1024 * 1024, 11 * 1024 * 1024];
    for size in test_sizes {
        match validator.validate_body_size(size) {
            Ok(_) => println!("  ✓ {} bytes allowed", size),
            Err(e) => println!("  ✗ {} bytes rejected: {}", size, e),
        }
    }

    // Test URL length validation
    println!("\nURL length validation:");
    let short_url = "https://example.com/api/v1/users";
    let long_url = format!("https://example.com/{}", "x".repeat(3000));
    
    match validator.validate_url_length(short_url) {
        Ok(_) => println!("  ✓ Short URL allowed"),
        Err(e) => println!("  ✗ Short URL rejected: {}", e),
    }
    
    match validator.validate_url_length(&long_url) {
        Ok(_) => println!("  ✓ Long URL allowed"),
        Err(e) => println!("  ✗ Long URL correctly rejected: {}", e),
    }

    // Test file upload validation
    println!("\nFile upload validation:");
    let test_files = vec![
        ("document.pdf", "application/pdf", 1024 * 100),
        ("image.jpg", "image/jpeg", 1024 * 500),
        ("large.jpg", "image/jpeg", 10 * 1024 * 1024),
        ("malware.exe", "application/octet-stream", 1024),
    ];

    for (filename, mime_type, size) in test_files {
        match validator.validate_file_upload(filename, mime_type, size) {
            Ok(_) => println!("  ✓ File '{}' allowed", filename),
            Err(e) => println!("  ✗ File '{}' rejected: {}", filename, e),
        }
    }

    // Test complete request validation
    println!("\nComplete request validation:");
    let headers = vec![
        ("Content-Type".to_string(), "application/json".to_string()),
        ("Authorization".to_string(), "Bearer token123".to_string()),
    ];

    match validator
        .validate_request(
            "POST",
            "https://example.com/api/users",
            &headers,
            1024,
            Some("application/json"),
        )
        .await
    {
        Ok(_) => println!("  ✓ Valid request accepted"),
        Err(e) => println!("  ✗ Request rejected: {}", e),
    }

    Ok(())
}

async fn demo_security_headers(
    header_manager: &SecurityHeaderManager,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Security Headers Demo ---");

    let headers = header_manager.generate_headers();
    println!("Generated {} security headers:", headers.len());
    for (name, value) in headers {
        println!("  {}: {}", name, value);
    }

    println!("\nIndividual header checks:");
    
    if let Some(hsts) = header_manager.get_hsts_header() {
        println!("  HSTS: {}", hsts);
    }
    
    if let Some(cto) = header_manager.get_x_content_type_options_header() {
        println!("  X-Content-Type-Options: {}", cto);
    }
    
    if let Some(xfo) = header_manager.get_x_frame_options_header() {
        println!("  X-Frame-Options: {}", xfo);
    }
    
    if let Some(xxp) = header_manager.get_x_xss_protection_header() {
        println!("  X-XSS-Protection: {}", xxp);
    }
    
    if let Some(rp) = header_manager.get_referrer_policy_header() {
        println!("  Referrer-Policy: {}", rp);
    }
    
    if let Some(pp) = header_manager.get_permissions_policy_header() {
        println!("  Permissions-Policy: {}", pp);
    }

    Ok(())
}
