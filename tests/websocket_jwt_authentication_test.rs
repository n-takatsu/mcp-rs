//! WebSocket JWT authentication tests
//!
//! This test module verifies JWT authentication functionality
//! for WebSocket connections.

use mcp_rs::transport::websocket::{JwtAlgorithm, JwtConfig, WebSocketConfig};

#[test]
fn test_jwt_algorithm_enum() {
    // Test all JWT algorithm variants
    let algorithms = [
        JwtAlgorithm::HS256,
        JwtAlgorithm::HS384,
        JwtAlgorithm::HS512,
        JwtAlgorithm::RS256,
        JwtAlgorithm::RS384,
        JwtAlgorithm::RS512,
        JwtAlgorithm::ES256,
        JwtAlgorithm::ES384,
    ];

    assert_eq!(algorithms.len(), 8);
    assert_eq!(JwtAlgorithm::default(), JwtAlgorithm::HS256);
}

#[test]
fn test_jwt_config_default() {
    let config = JwtConfig::default();

    assert!(config.secret.is_empty());
    assert_eq!(config.algorithm, JwtAlgorithm::HS256);
    assert_eq!(config.required_claims, vec!["sub"]);
    assert!(config.allowed_roles.is_empty());
    assert!(config.validate_exp);
    assert!(config.validate_nbf);
    assert!(!config.validate_iat);
    assert_eq!(config.leeway_seconds, 60);
}

#[test]
fn test_jwt_config_custom() {
    let config = JwtConfig {
        secret: "my-secret-key".to_string(),
        algorithm: JwtAlgorithm::HS512,
        required_claims: vec!["sub".to_string(), "email".to_string()],
        allowed_roles: vec!["admin".to_string(), "user".to_string()],
        validate_exp: true,
        validate_nbf: true,
        validate_iat: true,
        leeway_seconds: 30,
    };

    assert_eq!(config.secret, "my-secret-key");
    assert_eq!(config.algorithm, JwtAlgorithm::HS512);
    assert_eq!(config.required_claims.len(), 2);
    assert_eq!(config.allowed_roles.len(), 2);
    assert!(config.validate_iat);
    assert_eq!(config.leeway_seconds, 30);
}

#[test]
fn test_websocket_config_with_jwt_disabled() {
    let config = WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        use_tls: false,
        tls_config: None,
        origin_validation: mcp_rs::transport::websocket::OriginValidationPolicy::default(),
        require_origin_header: false,
        jwt_config: None,
        require_authentication: false,
        auth_timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 5,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        ..Default::default()
    };

    assert!(config.jwt_config.is_none());
    assert!(!config.require_authentication);
    assert_eq!(config.auth_timeout_seconds, Some(30));
}

#[test]
fn test_websocket_config_with_jwt_enabled() {
    let jwt_config = JwtConfig {
        secret: "test-secret".to_string(),
        algorithm: JwtAlgorithm::HS256,
        required_claims: vec!["sub".to_string()],
        allowed_roles: vec![],
        validate_exp: true,
        validate_nbf: true,
        validate_iat: false,
        leeway_seconds: 60,
    };

    let config = WebSocketConfig {
        url: "ws://localhost:8080".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        use_tls: false,
        tls_config: None,
        origin_validation: mcp_rs::transport::websocket::OriginValidationPolicy::default(),
        require_origin_header: false,
        jwt_config: Some(jwt_config),
        require_authentication: true,
        auth_timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 5,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        ..Default::default()
    };

    assert!(config.jwt_config.is_some());
    assert!(config.require_authentication);

    let jwt = config.jwt_config.unwrap();
    assert_eq!(jwt.secret, "test-secret");
    assert_eq!(jwt.algorithm, JwtAlgorithm::HS256);
}

#[test]
fn test_jwt_config_with_roles() {
    let config = JwtConfig {
        secret: "secret".to_string(),
        algorithm: JwtAlgorithm::HS256,
        required_claims: vec!["sub".to_string(), "roles".to_string()],
        allowed_roles: vec!["admin".to_string(), "moderator".to_string()],
        validate_exp: true,
        validate_nbf: true,
        validate_iat: false,
        leeway_seconds: 60,
    };

    assert_eq!(config.allowed_roles.len(), 2);
    assert!(config.allowed_roles.contains(&"admin".to_string()));
    assert!(config.allowed_roles.contains(&"moderator".to_string()));
    assert!(!config.allowed_roles.contains(&"user".to_string()));
}

#[test]
fn test_jwt_config_security_best_practices() {
    // Production configuration example
    let production_config = JwtConfig {
        secret: "very-long-and-secure-secret-key-min-32-chars".to_string(),
        algorithm: JwtAlgorithm::HS256,
        required_claims: vec!["sub".to_string(), "exp".to_string(), "iat".to_string()],
        allowed_roles: vec!["user".to_string()],
        validate_exp: true,
        validate_nbf: true,
        validate_iat: true,
        leeway_seconds: 30, // Strict leeway
    };

    assert!(production_config.secret.len() >= 32);
    assert!(production_config.validate_exp);
    assert!(production_config.validate_nbf);
    assert!(production_config.validate_iat);
    assert!(production_config.leeway_seconds <= 60);
}

#[test]
fn test_websocket_config_authentication_modes() {
    // Mode 1: No authentication
    let no_auth = WebSocketConfig {
        jwt_config: None,
        require_authentication: false,
        ..Default::default()
    };
    assert!(no_auth.jwt_config.is_none());
    assert!(!no_auth.require_authentication);

    // Mode 2: Optional authentication
    let optional_auth = WebSocketConfig {
        jwt_config: Some(JwtConfig::default()),
        require_authentication: false,
        ..Default::default()
    };
    assert!(optional_auth.jwt_config.is_some());
    assert!(!optional_auth.require_authentication);

    // Mode 3: Required authentication
    let required_auth = WebSocketConfig {
        jwt_config: Some(JwtConfig::default()),
        require_authentication: true,
        ..Default::default()
    };
    assert!(required_auth.jwt_config.is_some());
    assert!(required_auth.require_authentication);
}

#[test]
fn test_jwt_config_claim_validation() {
    let config = JwtConfig {
        secret: "secret".to_string(),
        algorithm: JwtAlgorithm::HS256,
        required_claims: vec!["sub".to_string(), "email".to_string(), "exp".to_string()],
        allowed_roles: vec![],
        validate_exp: true,
        validate_nbf: true,
        validate_iat: false,
        leeway_seconds: 60,
    };

    // Verify required claims
    assert!(config.required_claims.contains(&"sub".to_string()));
    assert!(config.required_claims.contains(&"email".to_string()));
    assert!(config.required_claims.contains(&"exp".to_string()));
    assert!(!config.required_claims.contains(&"iat".to_string()));
}

#[test]
fn test_jwt_algorithms_security_levels() {
    // HMAC algorithms (symmetric)
    let hmac_algorithms = [
        JwtAlgorithm::HS256,
        JwtAlgorithm::HS384,
        JwtAlgorithm::HS512,
    ];
    assert_eq!(hmac_algorithms.len(), 3);

    // RSA algorithms (asymmetric)
    let rsa_algorithms = [
        JwtAlgorithm::RS256,
        JwtAlgorithm::RS384,
        JwtAlgorithm::RS512,
    ];
    assert_eq!(rsa_algorithms.len(), 3);

    // ECDSA algorithms (asymmetric)
    let ecdsa_algorithms = [JwtAlgorithm::ES256, JwtAlgorithm::ES384];
    assert_eq!(ecdsa_algorithms.len(), 2);
}

#[test]
fn test_auth_timeout_configuration() {
    // Test various timeout configurations
    let configs = vec![
        (Some(10), "Short timeout"),
        (Some(30), "Standard timeout"),
        (Some(60), "Long timeout"),
        (None, "No timeout"),
    ];

    for (timeout, description) in configs {
        let config = WebSocketConfig {
            auth_timeout_seconds: timeout,
            ..Default::default()
        };
        assert_eq!(config.auth_timeout_seconds, timeout, "{}", description);
    }
}

#[test]
fn test_jwt_config_serialization() {
    let config = JwtConfig {
        secret: "test-secret".to_string(),
        algorithm: JwtAlgorithm::HS256,
        required_claims: vec!["sub".to_string()],
        allowed_roles: vec!["admin".to_string()],
        validate_exp: true,
        validate_nbf: true,
        validate_iat: false,
        leeway_seconds: 60,
    };

    // Test that config can be serialized (implicit test via serde)
    let serialized = serde_json::to_string(&config).expect("Failed to serialize");
    assert!(!serialized.is_empty());
    assert!(serialized.contains("test-secret"));
}

#[test]
fn test_combined_security_features() {
    // Test WebSocket config with both Origin validation and JWT authentication
    let secure_config = WebSocketConfig {
        url: "wss://api.example.com".to_string(),
        server_mode: true,
        timeout_seconds: Some(30),
        use_tls: true,
        tls_config: None,
        origin_validation: mcp_rs::transport::websocket::OriginValidationPolicy::AllowList(vec![
            "https://app.example.com".to_string(),
        ]),
        require_origin_header: true,
        jwt_config: Some(JwtConfig {
            secret: "secure-secret".to_string(),
            algorithm: JwtAlgorithm::HS256,
            required_claims: vec!["sub".to_string(), "exp".to_string()],
            allowed_roles: vec!["user".to_string()],
            validate_exp: true,
            validate_nbf: true,
            validate_iat: true,
            leeway_seconds: 30,
        }),
        require_authentication: true,
        auth_timeout_seconds: Some(30),
        heartbeat_interval: 30,
        max_reconnect_attempts: 5,
        reconnect_delay: 5,
        max_message_size: 16 * 1024 * 1024,
        max_connections: 100,
        ..Default::default()
    };

    // Verify all security features are enabled
    assert!(secure_config.use_tls);
    assert!(secure_config.require_origin_header);
    assert!(secure_config.jwt_config.is_some());
    assert!(secure_config.require_authentication);
}
