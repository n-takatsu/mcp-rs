//! WebSocket Origin Validation Demo
//!
//! This example demonstrates different Origin validation policies for WebSocket connections.
//! Origin validation is crucial for preventing Cross-Site Request Forgery (CSRF) attacks.

use mcp_rs::transport::websocket::{OriginValidationPolicy, WebSocketConfig};

fn main() {
    println!("=== WebSocket Origin Validation Demo ===\n");

    demo_allow_any_policy();
    demo_reject_all_policy();
    demo_allow_list_policy();
    demo_allow_pattern_policy();
    demo_production_configuration();
    demo_development_configuration();
}

/// Demo: AllowAny policy (development only)
fn demo_allow_any_policy() {
    println!("1. AllowAny Policy (Development Only)");
    println!("   ⚠️  WARNING: Do not use in production!");
    println!();

    let _config = WebSocketConfig {
        url: "127.0.0.1:8080".to_string(),
        server_mode: true,
        origin_validation: OriginValidationPolicy::AllowAny,
        require_origin_header: false,
        ..Default::default()
    };

    println!("   Configuration:");
    println!("   - Policy: AllowAny");
    println!("   - Require Origin Header: false");
    println!("   - Use Case: Local development, testing");
    println!();
    println!("   Behavior:");
    println!("   ✅ Accepts connections from any origin");
    println!("   ✅ Accepts connections without Origin header");
    println!();
}

/// Demo: RejectAll policy (most secure)
fn demo_reject_all_policy() {
    println!("2. RejectAll Policy (Default - Most Secure)");
    println!();

    let _config = WebSocketConfig {
        url: "0.0.0.0:8443".to_string(),
        server_mode: true,
        use_tls: true,
        origin_validation: OriginValidationPolicy::RejectAll,
        require_origin_header: true,
        ..Default::default()
    };

    println!("   Configuration:");
    println!("   - Policy: RejectAll (default)");
    println!("   - Require Origin Header: true");
    println!("   - Use Case: Initial deployment, security testing");
    println!();
    println!("   Behavior:");
    println!("   ❌ Rejects all connections");
    println!("   ❌ Logs rejected attempts as security attacks");
    println!();
}

/// Demo: AllowList policy (production recommended)
fn demo_allow_list_policy() {
    println!("3. AllowList Policy (Production Recommended)");
    println!();

    let allowed_origins = vec![
        "https://app.example.com".to_string(),
        "https://admin.example.com".to_string(),
        "https://api.example.com".to_string(),
    ];

    let _config = WebSocketConfig {
        url: "0.0.0.0:8443".to_string(),
        server_mode: true,
        use_tls: true,
        origin_validation: OriginValidationPolicy::AllowList(allowed_origins.clone()),
        require_origin_header: true,
        ..Default::default()
    };

    println!("   Configuration:");
    println!("   - Policy: AllowList");
    println!("   - Allowed Origins:");
    for origin in &allowed_origins {
        println!("     ✅ {}", origin);
    }
    println!("   - Require Origin Header: true");
    println!();
    println!("   Behavior:");
    println!("   ✅ https://app.example.com → Accepted");
    println!("   ✅ https://admin.example.com → Accepted");
    println!("   ❌ https://malicious.com → Rejected (not in allowlist)");
    println!("   ❌ http://app.example.com → Rejected (HTTP not allowed)");
    println!();
}

/// Demo: AllowPattern policy (flexible)
fn demo_allow_pattern_policy() {
    println!("4. AllowPattern Policy (Flexible with Regex)");
    println!();

    let patterns = vec![
        r"^https://.*\.example\.com$".to_string(), // All example.com subdomains
        r"^https://example\.com$".to_string(),     // example.com itself
    ];

    let _config = WebSocketConfig {
        url: "0.0.0.0:8443".to_string(),
        server_mode: true,
        use_tls: true,
        origin_validation: OriginValidationPolicy::AllowPattern(patterns.clone()),
        require_origin_header: true,
        ..Default::default()
    };

    println!("   Configuration:");
    println!("   - Policy: AllowPattern");
    println!("   - Patterns:");
    for pattern in &patterns {
        println!("     - {}", pattern);
    }
    println!("   - Require Origin Header: true");
    println!();

    println!("   Pattern Matching Examples:");
    use regex::Regex;

    let test_cases = vec![
        ("https://app.example.com", true),
        ("https://api.example.com", true),
        ("https://admin.example.com", true),
        ("https://example.com", true),
        ("http://app.example.com", false), // Wrong protocol
        ("https://example.com.malicious.com", false), // Wrong domain
        ("https://malicious.com", false),  // Different domain
    ];

    for (origin, should_match) in test_cases {
        let matches = patterns.iter().any(|pattern| {
            Regex::new(pattern)
                .map(|re| re.is_match(origin))
                .unwrap_or(false)
        });

        let symbol = if matches == should_match {
            if matches {
                "✅"
            } else {
                "❌"
            }
        } else {
            "❗"
        };

        println!(
            "   {} {} → {}",
            symbol,
            origin,
            if matches { "Accepted" } else { "Rejected" }
        );
    }
    println!();
}

/// Demo: Production configuration
fn demo_production_configuration() {
    println!("5. Production Configuration Example");
    println!();

    let _config = WebSocketConfig {
        url: "0.0.0.0:8443".to_string(),
        server_mode: true,
        use_tls: true,
        tls_config: Some(mcp_rs::transport::websocket::TlsConfig {
            cert_path: Some(std::path::PathBuf::from("/etc/ssl/certs/server.crt")),
            key_path: Some(std::path::PathBuf::from("/etc/ssl/private/server.key")),
            ca_cert_path: Some(std::path::PathBuf::from("/etc/ssl/certs/ca-bundle.crt")),
            verify_server: true,
            accept_invalid_certs: false,
        }),
        origin_validation: OriginValidationPolicy::AllowList(vec![
            "https://app.example.com".to_string(),
            "https://admin.example.com".to_string(),
        ]),
        require_origin_header: true,
        max_connections: 1000,
        heartbeat_interval: 30,
        timeout_seconds: Some(60),
        ..Default::default()
    };

    println!("   Security Features:");
    println!("   ✅ TLS/WSS encryption enabled");
    println!("   ✅ Origin validation (AllowList)");
    println!("   ✅ Require Origin header");
    println!("   ✅ Certificate verification");
    println!("   ✅ Max 1000 concurrent connections");
    println!("   ✅ 30s heartbeat interval");
    println!();
    println!("   This configuration provides:");
    println!("   - Protection against CSRF attacks");
    println!("   - Encrypted communication (TLS)");
    println!("   - Audit logging of security events");
    println!("   - Production-ready scalability");
    println!();
}

/// Demo: Development configuration
fn demo_development_configuration() {
    println!("6. Development Configuration Example");
    println!();

    // Development configuration (localhost only)
    #[cfg(debug_assertions)]
    let origin_policy = OriginValidationPolicy::AllowPattern(vec![
        r"^https?://localhost:\d+$".to_string(),
        r"^https?://127\.0\.0\.1:\d+$".to_string(),
    ]);

    #[cfg(not(debug_assertions))]
    let origin_policy =
        OriginValidationPolicy::AllowList(vec!["https://app.example.com".to_string()]);

    let _config = WebSocketConfig {
        url: "127.0.0.1:8080".to_string(),
        server_mode: true,
        use_tls: false, // Development only
        origin_validation: origin_policy,
        require_origin_header: false, // Relaxed for development
        ..Default::default()
    };

    println!("   Development Features:");
    println!("   ✅ Localhost-only binding (127.0.0.1)");
    println!("   ✅ No TLS (development only)");
    println!("   ✅ Accepts localhost origins (HTTP/HTTPS)");
    println!("   ⚠️  Optional Origin header");
    println!();
    println!("   Accepted Origins:");
    println!("   ✅ http://localhost:3000");
    println!("   ✅ https://localhost:3000");
    println!("   ✅ http://127.0.0.1:5173");
    println!();
    println!("   ⚠️  WARNING: This configuration is for development only!");
    println!("   ⚠️  Do not use in production environments.");
    println!();
}
