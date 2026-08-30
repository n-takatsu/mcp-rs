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
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ip_whitelist: vec!["192.168.1.100".to_string()],
        ..NetworkPolicy::default()
    };

    let allowed: SocketAddr = "192.168.1.100:8080".parse().unwrap();
    assert!(policy.validate_connection(&allowed).is_ok());
}

#[test]
fn test_whitelist_rejects_non_whitelisted_ip() {
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ip_whitelist: vec!["192.168.1.100".to_string()],
        ..NetworkPolicy::default()
    };

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
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ip_whitelist: vec![],
        ..NetworkPolicy::default()
    };

    // Any IP should be allowed when whitelist is empty and external connections are allowed
    let any_ip: SocketAddr = "8.8.8.8:53".parse().unwrap();
    assert!(policy.validate_connection(&any_ip).is_ok());
}

#[test]
fn test_localhost_always_whitelisted() {
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ip_whitelist: vec!["192.168.1.100".to_string()],
        ..NetworkPolicy::default()
    };

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

    // Loopback binds are always allowed regardless of policy.
    assert!(policy.validate_bind_address(&localhost).is_ok());
}

#[test]
fn test_bind_address_validation_external() {
    let policy = NetworkPolicy::default();
    let external: SocketAddr = "0.0.0.0:8080".parse().unwrap();

    // Under the default (secure) policy, binding to a non-loopback address
    // must be rejected outright, not just logged as a warning.
    let result = policy.validate_bind_address(&external);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        NetworkPolicyError::ExternalAccessDenied(_)
    ));
}

#[test]
fn test_bind_address_rejects_unspecified_v6() {
    let policy = NetworkPolicy::default();
    let external: SocketAddr = "[::]:8080".parse().unwrap();

    assert!(policy.validate_bind_address(&external).is_err());
}

#[test]
fn test_bind_address_rejects_concrete_external_ip() {
    let policy = NetworkPolicy::default();
    let external: SocketAddr = "192.168.1.50:8080".parse().unwrap();

    assert!(policy.validate_bind_address(&external).is_err());
}

#[test]
fn test_bind_address_allows_when_reject_external_disabled() {
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ..NetworkPolicy::default()
    };
    let external: SocketAddr = "0.0.0.0:8080".parse().unwrap();

    assert!(policy.validate_bind_address(&external).is_ok());
}

#[test]
fn test_bind_address_allows_loopback_v6_regardless_of_flags() {
    let policy = NetworkPolicy {
        reject_external_connections: true,
        warn_on_external_bind: false,
        ..NetworkPolicy::default()
    };
    let localhost: SocketAddr = "[::1]:8080".parse().unwrap();

    assert!(policy.validate_bind_address(&localhost).is_ok());
}

#[test]
fn test_bind_address_warn_flag_does_not_gate_enforcement() {
    // Regression test for the original bug: enforcement must be driven by
    // reject_external_connections, not warn_on_external_bind. Disabling the
    // warning must not disable the actual rejection.
    let policy = NetworkPolicy {
        reject_external_connections: true,
        warn_on_external_bind: false,
        ..NetworkPolicy::default()
    };
    let external: SocketAddr = "0.0.0.0:8080".parse().unwrap();

    assert!(policy.validate_bind_address(&external).is_err());
}

#[test]
fn test_whitelist_cidr_range_matches() {
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ip_whitelist: vec!["192.168.1.0/24".to_string()],
        ..NetworkPolicy::default()
    };

    let inside: SocketAddr = "192.168.1.42:8080".parse().unwrap();
    assert!(policy.validate_connection(&inside).is_ok());
}

#[test]
fn test_whitelist_cidr_range_excludes_outside_range() {
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ip_whitelist: vec!["192.168.1.0/24".to_string()],
        ..NetworkPolicy::default()
    };

    let outside: SocketAddr = "192.168.2.42:8080".parse().unwrap();
    let result = policy.validate_connection(&outside);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        NetworkPolicyError::IpNotWhitelisted(_)
    ));
}

#[test]
fn test_multiple_whitelisted_ips() {
    let policy = NetworkPolicy {
        reject_external_connections: false,
        ip_whitelist: vec![
            "192.168.1.100".to_string(),
            "192.168.1.101".to_string(),
            "10.0.0.50".to_string(),
        ],
        ..NetworkPolicy::default()
    };

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
