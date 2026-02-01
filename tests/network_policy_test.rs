//! Network Policy Integration Tests

use mcp_rs::security::{NetworkPolicy, NetworkPolicyError};
use std::net::SocketAddr;

#[test]
fn test_default_policy_allows_localhost_v4() {
    let policy = NetworkPolicy::default();
    let localhost: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    assert!(policy.validate_connection(&localhost).is_ok());
}

#[test]
fn test_default_policy_allows_localhost_v6() {
    let policy = NetworkPolicy::default();
    let localhost: SocketAddr = "[::1]:8080".parse().unwrap();

    assert!(policy.validate_connection(&localhost).is_ok());
}

#[test]
fn test_default_policy_rejects_external_ip() {
    let policy = NetworkPolicy::default();
    let external: SocketAddr = "192.168.1.100:8080".parse().unwrap();

    let result = policy.validate_connection(&external);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        NetworkPolicyError::ExternalAccessDenied(_)
    ));
}

#[test]
fn test_default_policy_rejects_internet_ip() {
    let policy = NetworkPolicy::default();
    let internet: SocketAddr = "8.8.8.8:53".parse().unwrap();

    let result = policy.validate_connection(&internet);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        NetworkPolicyError::ExternalAccessDenied(_)
    ));
}

#[test]
fn test_whitelist_allows_specific_ip() {
    let mut policy = NetworkPolicy::default();
    policy.reject_external_connections = false;
    policy.ip_whitelist = vec!["192.168.1.100".to_string()];

    let allowed: SocketAddr = "192.168.1.100:8080".parse().unwrap();
    assert!(policy.validate_connection(&allowed).is_ok());
}

#[test]
fn test_whitelist_rejects_non_whitelisted_ip() {
    let mut policy = NetworkPolicy::default();
    policy.reject_external_connections = false;
    policy.ip_whitelist = vec!["192.168.1.100".to_string()];

    let denied: SocketAddr = "192.168.1.101:8080".parse().unwrap();

    let result = policy.validate_connection(&denied);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        NetworkPolicyError::IpNotWhitelisted(_)
    ));
}

#[test]
fn test_empty_whitelist_with_external_allowed() {
    let mut policy = NetworkPolicy::default();
    policy.reject_external_connections = false;
    policy.ip_whitelist = vec![];

    // Any IP should be allowed when whitelist is empty and external connections are allowed
    let any_ip: SocketAddr = "8.8.8.8:53".parse().unwrap();
    assert!(policy.validate_connection(&any_ip).is_ok());
}

#[test]
fn test_localhost_always_whitelisted() {
    let mut policy = NetworkPolicy::default();
    policy.reject_external_connections = false;
    policy.ip_whitelist = vec!["192.168.1.100".to_string()];

    // Localhost should always be allowed, even if not explicitly in whitelist
    let localhost_v4: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let localhost_v6: SocketAddr = "[::1]:8080".parse().unwrap();

    assert!(policy.validate_connection(&localhost_v4).is_ok());
    assert!(policy.validate_connection(&localhost_v6).is_ok());
}

#[test]
fn test_bind_address_validation_localhost() {
    let policy = NetworkPolicy::default();
    let localhost: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    // Should not error, just potentially warn
    assert!(policy.validate_bind_address(&localhost).is_ok());
}

#[test]
fn test_bind_address_validation_external() {
    let policy = NetworkPolicy::default();
    let external: SocketAddr = "0.0.0.0:8080".parse().unwrap();

    // Should not error, just warn
    assert!(policy.validate_bind_address(&external).is_ok());
}

#[test]
fn test_multiple_whitelisted_ips() {
    let mut policy = NetworkPolicy::default();
    policy.reject_external_connections = false;
    policy.ip_whitelist = vec![
        "192.168.1.100".to_string(),
        "192.168.1.101".to_string(),
        "10.0.0.50".to_string(),
    ];

    let ip1: SocketAddr = "192.168.1.100:8080".parse().unwrap();
    let ip2: SocketAddr = "192.168.1.101:8080".parse().unwrap();
    let ip3: SocketAddr = "10.0.0.50:8080".parse().unwrap();
    let ip4: SocketAddr = "192.168.1.102:8080".parse().unwrap();

    assert!(policy.validate_connection(&ip1).is_ok());
    assert!(policy.validate_connection(&ip2).is_ok());
    assert!(policy.validate_connection(&ip3).is_ok());
    assert!(policy.validate_connection(&ip4).is_err());
}

#[test]
fn test_serialization() {
    let policy = NetworkPolicy {
        reject_external_connections: true,
        warn_on_external_bind: true,
        ip_whitelist: vec!["192.168.1.100".to_string()],
    };

    let serialized = serde_json::to_string(&policy).unwrap();
    let deserialized: NetworkPolicy = serde_json::from_str(&serialized).unwrap();

    assert_eq!(
        policy.reject_external_connections,
        deserialized.reject_external_connections
    );
    assert_eq!(
        policy.warn_on_external_bind,
        deserialized.warn_on_external_bind
    );
    assert_eq!(policy.ip_whitelist, deserialized.ip_whitelist);
}
