//! Anti-Replay Attack Prevention Integration Tests

use mcp_rs::security::anti_replay::{
    AntiReplayConfig, AntiReplayMiddleware, DeviceFingerprint, NonceManager,
};
use std::net::IpAddr;
use std::sync::Arc;

#[tokio::test]
async fn test_nonce_replay_attack_prevention() {
    let config = AntiReplayConfig::default();
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    let nonce = NonceManager::generate_nonce();

    // First use should succeed
    let result = middleware
        .nonce_manager()
        .validate_and_consume(&nonce, Some("user123".to_string()))
        .await;
    assert!(result.is_ok(), "First nonce use should succeed");

    // Second use should fail (replay attack)
    let result = middleware
        .nonce_manager()
        .validate_and_consume(&nonce, Some("user123".to_string()))
        .await;
    assert!(
        result.is_err(),
        "Second nonce use should fail (replay attack)"
    );
}

#[tokio::test]
async fn test_timestamp_validation() {
    let config = AntiReplayConfig {
        time_window_secs: 30,
        ..Default::default()
    };
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    // Current timestamp should be valid
    let current_timestamp = chrono::Utc::now().to_rfc3339();
    let result = middleware
        .timestamp_validator()
        .validate_timestamp(&current_timestamp);
    assert!(result.is_ok(), "Current timestamp should be valid");

    // Old timestamp should be rejected
    let old_timestamp = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
    let result = middleware
        .timestamp_validator()
        .validate_timestamp(&old_timestamp);
    assert!(result.is_err(), "Old timestamp should be rejected");

    // Future timestamp should be rejected
    let future_timestamp = (chrono::Utc::now() + chrono::Duration::seconds(60)).to_rfc3339();
    let result = middleware
        .timestamp_validator()
        .validate_timestamp(&future_timestamp);
    assert!(result.is_err(), "Future timestamp should be rejected");
}

#[tokio::test]
async fn test_device_fingerprint_registration_and_verification() {
    let config = AntiReplayConfig {
        enable_device_fingerprint: true,
        allow_new_devices: true,
        max_devices_per_user: 3,
        device_match_threshold: 0.8,
        ..Default::default()
    };
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    let user_id = "test-user-456";
    let ip: IpAddr = "192.168.1.100".parse().unwrap();

    // Create and register first device
    let device1 = DeviceFingerprint::new(Some("Mozilla/5.0 (Windows NT 10.0)"), ip, None);

    middleware
        .device_manager()
        .register_device(user_id, device1.clone())
        .await
        .unwrap();

    // Verify same device
    let verified = middleware
        .device_manager()
        .verify_device(user_id, &device1, 0.8)
        .await
        .unwrap();
    assert!(verified, "Same device should be verified");

    // Create different device
    let ip2: IpAddr = "10.0.0.1".parse().unwrap();
    let device2 = DeviceFingerprint::new(Some("Chrome/90.0"), ip2, None);

    // Different device should not be verified
    let verified = middleware
        .device_manager()
        .verify_device(user_id, &device2, 0.8)
        .await
        .unwrap();
    assert!(
        !verified,
        "Different device should not be verified initially"
    );

    // Register second device
    middleware
        .device_manager()
        .register_device(user_id, device2.clone())
        .await
        .unwrap();

    // Now it should be verified
    let verified = middleware
        .device_manager()
        .verify_device(user_id, &device2, 0.8)
        .await
        .unwrap();
    assert!(verified, "Registered device should be verified");

    // Check device count
    let devices = middleware.device_manager().get_user_devices(user_id).await;
    assert_eq!(devices.len(), 2, "User should have 2 registered devices");
}

#[tokio::test]
async fn test_max_devices_limit() {
    let config = AntiReplayConfig {
        enable_device_fingerprint: true,
        max_devices_per_user: 2,
        ..Default::default()
    };
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    let user_id = "limited-user";

    // Register 3 devices
    for i in 0..3 {
        let ip: IpAddr = format!("192.168.1.{}", 100 + i).parse().unwrap();
        let device = DeviceFingerprint::new(Some(&format!("Browser-{}", i)), ip, None);

        middleware
            .device_manager()
            .register_device(user_id, device)
            .await
            .unwrap();
    }

    // Should only keep 2 devices (oldest removed)
    let devices = middleware.device_manager().get_user_devices(user_id).await;
    assert_eq!(
        devices.len(),
        2,
        "Should only keep max_devices_per_user devices"
    );
}

#[tokio::test]
async fn test_nonce_expiration() {
    let config = AntiReplayConfig {
        nonce_ttl_secs: 1, // 1 second TTL
        ..Default::default()
    };
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    let nonce = NonceManager::generate_nonce();

    // Consume nonce
    middleware
        .nonce_manager()
        .validate_and_consume(&nonce, None)
        .await
        .unwrap();

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Should be able to use again after expiration
    let result = middleware
        .nonce_manager()
        .validate_and_consume(&nonce, None)
        .await;
    assert!(result.is_ok(), "Nonce should be usable after expiration");
}

#[tokio::test]
async fn test_nonce_cleanup() {
    let config = AntiReplayConfig {
        nonce_ttl_secs: 1,
        max_nonce_cache: 1000,
        ..Default::default()
    };
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    // Add multiple nonces
    for _ in 0..10 {
        let nonce = NonceManager::generate_nonce();
        middleware
            .nonce_manager()
            .validate_and_consume(&nonce, None)
            .await
            .unwrap();
    }

    assert_eq!(
        middleware.nonce_manager().nonce_count().await,
        10,
        "Should have 10 nonces"
    );

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Cleanup
    let removed = middleware.nonce_manager().cleanup_expired().await;
    assert_eq!(removed, 10, "Should have removed 10 expired nonces");
    assert_eq!(
        middleware.nonce_manager().nonce_count().await,
        0,
        "Should have 0 nonces after cleanup"
    );
}

#[tokio::test]
async fn test_device_removal() {
    let config = AntiReplayConfig {
        enable_device_fingerprint: true,
        ..Default::default()
    };
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    let user_id = "removal-test-user";
    let ip: IpAddr = "192.168.1.200".parse().unwrap();
    let device = DeviceFingerprint::new(Some("Mozilla/5.0"), ip, None);

    // Register device
    middleware
        .device_manager()
        .register_device(user_id, device.clone())
        .await
        .unwrap();

    assert_eq!(
        middleware
            .device_manager()
            .get_user_devices(user_id)
            .await
            .len(),
        1
    );

    // Remove device
    middleware
        .device_manager()
        .remove_device(user_id, &device.device_id)
        .await
        .unwrap();

    assert_eq!(
        middleware
            .device_manager()
            .get_user_devices(user_id)
            .await
            .len(),
        0
    );
}

#[test]
fn test_nonce_generation_uniqueness() {
    let mut nonces = std::collections::HashSet::new();

    // Generate 1000 nonces
    for _ in 0..1000 {
        let nonce = NonceManager::generate_nonce();
        assert!(
            nonces.insert(nonce),
            "All generated nonces should be unique"
        );
    }

    assert_eq!(
        nonces.len(),
        1000,
        "Should have generated 1000 unique nonces"
    );
}

#[test]
fn test_device_fingerprint_similarity() {
    let ip1: IpAddr = "192.168.1.1".parse().unwrap();
    let ip2: IpAddr = "192.168.1.1".parse().unwrap();
    let ip3: IpAddr = "10.0.0.1".parse().unwrap();

    let fp1 = DeviceFingerprint::new(Some("Mozilla/5.0"), ip1, None);
    let fp2 = DeviceFingerprint::new(Some("Mozilla/5.0"), ip2, None);
    let fp3 = DeviceFingerprint::new(Some("Chrome/90.0"), ip3, None);

    // Same UA and IP should be 100% similar
    assert_eq!(
        fp1.similarity(&fp2),
        1.0,
        "Identical fingerprints should have 1.0 similarity"
    );

    // Different UA and IP should have low similarity
    let similarity = fp1.similarity(&fp3);
    assert!(
        similarity < 0.5,
        "Different fingerprints should have <0.5 similarity, got {}",
        similarity
    );
}

#[tokio::test]
async fn test_concurrent_nonce_validation() {
    let config = AntiReplayConfig::default();
    let middleware = Arc::new(AntiReplayMiddleware::new(config));

    let nonce = NonceManager::generate_nonce();

    // Try to use the same nonce concurrently
    let middleware1 = middleware.clone();
    let middleware2 = middleware.clone();
    let nonce1 = nonce.clone();
    let nonce2 = nonce.clone();

    let handle1 = tokio::spawn(async move {
        middleware1
            .nonce_manager()
            .validate_and_consume(&nonce1, None)
            .await
    });

    let handle2 = tokio::spawn(async move {
        middleware2
            .nonce_manager()
            .validate_and_consume(&nonce2, None)
            .await
    });

    let result1 = handle1.await.unwrap();
    let result2 = handle2.await.unwrap();

    // One should succeed, one should fail
    assert!(
        result1.is_ok() != result2.is_ok(),
        "Only one concurrent nonce use should succeed"
    );
}
