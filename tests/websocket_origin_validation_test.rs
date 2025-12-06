use mcp_rs::transport::websocket::{WebSocketConfig, OriginValidationPolicy};

#[test]
fn test_origin_validation_policies() {
    // Test AllowAny policy
    let allow_any = OriginValidationPolicy::AllowAny;
    assert!(matches!(allow_any, OriginValidationPolicy::AllowAny));

    // Test RejectAll policy (default)
    let reject_all = OriginValidationPolicy::default();
    assert!(matches!(reject_all, OriginValidationPolicy::RejectAll));

    // Test AllowList policy
    let allow_list = OriginValidationPolicy::AllowList(vec![
        "https://example.com".to_string(),
        "https://app.example.com".to_string(),
    ]);
    if let OriginValidationPolicy::AllowList(ref origins) = allow_list {
        assert_eq!(origins.len(), 2);
        assert!(origins.contains(&"https://example.com".to_string()));
        assert!(origins.contains(&"https://app.example.com".to_string()));
    } else {
        panic!("Expected AllowList policy");
    }

    // Test AllowPattern policy
    let allow_pattern = OriginValidationPolicy::AllowPattern(vec![
        r"^https://.*\.example\.com$".to_string(),
    ]);
    if let OriginValidationPolicy::AllowPattern(ref patterns) = allow_pattern {
        assert_eq!(patterns.len(), 1);
        
        // Test pattern matching
        use regex::Regex;
        let re = Regex::new(&patterns[0]).unwrap();
        assert!(re.is_match("https://app.example.com"));
        assert!(re.is_match("https://api.example.com"));
        assert!(!re.is_match("https://example.com")); // Missing subdomain
        assert!(!re.is_match("http://app.example.com")); // Wrong protocol
    } else {
        panic!("Expected AllowPattern policy");
    }
}

#[test]
fn test_websocket_config_origin_validation() {
    // Test default configuration (RejectAll)
    let default_config = WebSocketConfig::default();
    assert!(matches!(
        default_config.origin_validation,
        OriginValidationPolicy::RejectAll
    ));
    assert!(!default_config.require_origin_header);

    // Test AllowAny configuration
    let allow_any_config = WebSocketConfig {
        origin_validation: OriginValidationPolicy::AllowAny,
        require_origin_header: false,
        ..Default::default()
    };
    assert!(matches!(
        allow_any_config.origin_validation,
        OriginValidationPolicy::AllowAny
    ));

    // Test AllowList configuration
    let allow_list_config = WebSocketConfig {
        origin_validation: OriginValidationPolicy::AllowList(vec![
            "https://example.com".to_string(),
        ]),
        require_origin_header: true,
        ..Default::default()
    };
    assert!(allow_list_config.require_origin_header);

    // Test AllowPattern configuration
    let allow_pattern_config = WebSocketConfig {
        origin_validation: OriginValidationPolicy::AllowPattern(vec![
            r"^https://.*\.example\.com$".to_string(),
        ]),
        require_origin_header: true,
        ..Default::default()
    };
    assert!(allow_pattern_config.require_origin_header);
}

#[test]
fn test_origin_pattern_matching() {
    // Test common patterns
    let patterns = vec![
        // Subdomain pattern
        (r"^https://.*\.example\.com$", vec![
            ("https://app.example.com", true),
            ("https://api.example.com", true),
            ("https://example.com", false),
            ("http://app.example.com", false),
        ]),
        // Exact match
        (r"^https://example\.com$", vec![
            ("https://example.com", true),
            ("https://app.example.com", false),
            ("http://example.com", false),
        ]),
        // Localhost pattern
        (r"^https?://localhost:\d+$", vec![
            ("http://localhost:3000", true),
            ("https://localhost:8080", true),
            ("http://localhost", false),
            ("https://example.com", false),
        ]),
    ];

    for (pattern, cases) in patterns {
        use regex::Regex;
        let re = Regex::new(pattern).unwrap();
        
        for (origin, expected) in cases {
            assert_eq!(
                re.is_match(origin),
                expected,
                "Pattern '{}' should {} match '{}'",
                pattern,
                if expected { "" } else { "not" },
                origin
            );
        }
    }
}

#[test]
fn test_origin_validation_security() {
    // Security-focused test cases
    
    // 1. RejectAll policy (most secure default)
    let reject_all = OriginValidationPolicy::RejectAll;
    assert!(matches!(reject_all, OriginValidationPolicy::RejectAll));
    
    // 2. AllowList policy (production recommended)
    let production_origins = vec![
        "https://app.example.com".to_string(),
        "https://admin.example.com".to_string(),
    ];
    let allow_list = OriginValidationPolicy::AllowList(production_origins);
    if let OriginValidationPolicy::AllowList(ref origins) = allow_list {
        // Verify no wildcards or insecure origins
        for origin in origins {
            assert!(origin.starts_with("https://"));
            assert!(!origin.contains('*'));
        }
    }
    
    // 3. AllowPattern policy (flexible but requires careful regex)
    let safe_patterns = vec![
        r"^https://.*\.example\.com$".to_string(), // Only HTTPS, only example.com subdomains
    ];
    let allow_pattern = OriginValidationPolicy::AllowPattern(safe_patterns);
    if let OriginValidationPolicy::AllowPattern(ref patterns) = allow_pattern {
        use regex::Regex;
        for pattern in patterns {
            let re = Regex::new(pattern).unwrap();
            // Verify pattern enforces HTTPS
            assert!(!re.is_match("http://app.example.com"));
            // Verify pattern doesn't match arbitrary domains
            assert!(!re.is_match("https://malicious.com"));
        }
    }
}

#[test]
fn test_require_origin_header_flag() {
    // Test require_origin_header flag behavior
    
    // Secure configuration: require Origin header
    let secure_config = WebSocketConfig {
        origin_validation: OriginValidationPolicy::AllowList(vec![
            "https://app.example.com".to_string(),
        ]),
        require_origin_header: true,
        ..Default::default()
    };
    assert!(secure_config.require_origin_header);
    
    // Development configuration: optional Origin header
    let dev_config = WebSocketConfig {
        origin_validation: OriginValidationPolicy::AllowAny,
        require_origin_header: false,
        ..Default::default()
    };
    assert!(!dev_config.require_origin_header);
}
