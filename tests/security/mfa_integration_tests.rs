//! MFA Integration Tests
//!
//! Comprehensive integration tests for the entire MFA system,
//! testing the interaction between all phases.

#[cfg(feature = "mfa")]
use mcp_rs::security::mfa::{
    BackupCodeConfig, BackupCodeManager, DeviceTrustConfig, DeviceTrustManager, MfaMethod,
    SessionMfaConfig, SessionMfaManager, TotpConfig, TotpSecret, TotpVerifier,
};
#[cfg(feature = "mfa")]
use std::sync::Arc;

#[cfg(feature = "mfa")]
#[tokio::test]
async fn test_complete_mfa_flow() {
    // User registration: Generate TOTP secret and backup codes
    let totp_config = TotpConfig::default();
    let totp_secret = TotpSecret::generate(&totp_config).unwrap();
    let totp_verifier = TotpVerifier::new(totp_config.clone());

    let backup_config = BackupCodeConfig::default();
    let backup_manager = BackupCodeManager::new(backup_config);
    let (plaintext_codes, mut stored_codes) = backup_manager.generate().unwrap();

    // Verify initial state
    assert_eq!(plaintext_codes.len(), 10);
    assert_eq!(stored_codes.len(), 10);

    // First login: Verify TOTP
    let code = totp_verifier.generate_code(&totp_secret).unwrap();
    let totp_valid = totp_verifier.verify(&totp_secret, &code).unwrap();
    assert!(totp_valid);

    // Second login: Use backup code
    let backup_code = plaintext_codes[0].clone();
    let result = backup_manager.verify(&backup_code, &mut stored_codes);
    assert!(result.is_ok());

    // Backup code should be consumed
    let result_reuse = backup_manager.verify(&backup_code, &mut stored_codes);
    assert!(result_reuse.is_err());
}

#[tokio::test]
async fn test_device_trust_and_session_integration() {
    // Setup device trust
    let device_config = DeviceTrustConfig::default();
    let device_trust = Arc::new(DeviceTrustManager::new(device_config));

    // Setup session MFA
    let session_config = SessionMfaConfig::default();
    let session_manager = SessionMfaManager::new(session_config, Some(device_trust.clone()));

    let user_id = "user123";
    let session_id = "session_001";

    // First login: MFA required
    let is_required = session_manager
        .is_mfa_required(user_id, session_id, None)
        .await;
    assert!(is_required);

    // Create and verify MFA challenge
    let challenge = session_manager
        .create_challenge(user_id, vec![MfaMethod::Totp])
        .await
        .unwrap();

    session_manager
        .verify_session_mfa(
            session_id,
            user_id,
            &challenge.challenge_id,
            MfaMethod::Totp,
            None,
        )
        .await
        .unwrap();

    // Trust the device
    let fingerprint = DeviceTrustManager::generate_fingerprint("ua", "ip", None);
    device_trust
        .trust_device(user_id, &fingerprint, "ua", "ip", "Test Device")
        .await
        .unwrap();

    // Second login from trusted device: MFA not required
    let session_id2 = "session_002";
    let is_required2 = session_manager
        .is_mfa_required(user_id, session_id2, Some(&fingerprint))
        .await;
    assert!(!is_required2);
}

#[tokio::test]
async fn test_session_mfa_with_multiple_devices() {
    // Setup device trust and session manager
    let device_config = DeviceTrustConfig::default();
    let device_trust = Arc::new(DeviceTrustManager::new(device_config));

    let session_config = SessionMfaConfig::default();
    let session_manager = SessionMfaManager::new(session_config, Some(device_trust.clone()));

    let user_id = "user123";

    // Trust multiple devices
    let devices = vec![
        ("ua1", "ip1", "Device 1"),
        ("ua2", "ip2", "Device 2"),
        ("ua3", "ip3", "Device 3"),
    ];

    for (ua, ip, name) in &devices {
        let fp = DeviceTrustManager::generate_fingerprint(ua, ip, None);
        device_trust
            .trust_device(user_id, &fp, ua, ip, name)
            .await
            .unwrap();
    }

    // Each device should bypass MFA
    for (idx, (ua, ip, _)) in devices.iter().enumerate() {
        let fp = DeviceTrustManager::generate_fingerprint(ua, ip, None);
        let session_id = format!("session_{:03}", idx);
        let is_required = session_manager
            .is_mfa_required(user_id, &session_id, Some(&fp))
            .await;
        assert!(!is_required);
    }

    // Unknown device should require MFA
    let unknown_fp = DeviceTrustManager::generate_fingerprint("unknown_ua", "unknown_ip", None);
    let is_required_unknown = session_manager
        .is_mfa_required(user_id, "session_unknown", Some(&unknown_fp))
        .await;
    assert!(is_required_unknown);
}

#[tokio::test]
async fn test_totp_algorithm_variations() {
    // Test all TOTP algorithms
    use mcp_rs::security::mfa::TotpAlgorithm;

    let algorithms = vec![
        TotpAlgorithm::Sha1,
        TotpAlgorithm::Sha256,
        TotpAlgorithm::Sha512,
    ];

    for algorithm in algorithms {
        let config = TotpConfig {
            enabled: true,
            algorithm,
            digits: 6,
            period: 30,
            time_window: 1,
        };

        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        let code = verifier.generate_code(&secret).unwrap();
        assert_eq!(code.len(), 6);
        assert!(verifier.verify(&secret, &code).unwrap());
    }
}

#[tokio::test]
async fn test_backup_code_lifecycle() {
    let config = BackupCodeConfig::default();
    let manager = BackupCodeManager::new(config);

    // Generate initial codes
    let (plaintext1, mut stored1) = manager.generate().unwrap();
    assert_eq!(plaintext1.len(), 10);

    // Use 8 codes
    for code in plaintext1.iter().take(8) {
        manager.verify(code, &mut stored1).unwrap();
    }

    // Should suggest regeneration
    assert!(manager.should_regenerate(&stored1));
    assert_eq!(manager.remaining_count(&stored1), 2);

    // Regenerate
    let (plaintext2, mut stored2) = manager.generate().unwrap();
    assert_eq!(plaintext2.len(), 10);

    // New codes should work
    manager.verify(&plaintext2[0], &mut stored2).unwrap();
    assert_eq!(manager.remaining_count(&stored2), 9);
}

#[tokio::test]
async fn test_device_max_limit_enforcement() {
    let config = DeviceTrustConfig {
        enabled: true,
        max_devices_per_user: 3,
        token_validity_seconds: 2592000,
        require_mfa_on_new_device: true,
    };
    let manager = DeviceTrustManager::new(config);
    let user_id = "user123";

    // Add 3 devices (at limit)
    for i in 0..3 {
        let fp = DeviceTrustManager::generate_fingerprint(
            &format!("ua{}", i),
            &format!("ip{}", i),
            None,
        );
        manager
            .trust_device(
                user_id,
                &fp,
                &format!("ua{}", i),
                &format!("ip{}", i),
                &format!("Device {}", i),
            )
            .await
            .unwrap();
    }

    let devices = manager.get_user_devices(user_id).await;
    assert_eq!(devices.len(), 3);

    // Adding 4th device should fail (max limit reached)
    let fp4 = DeviceTrustManager::generate_fingerprint("ua4", "ip4", None);
    let result = manager
        .trust_device(user_id, &fp4, "ua4", "ip4", "Device 4")
        .await;

    // Should return an error due to max limit
    assert!(result.is_err());

    // Device count should remain at 3
    let devices_after = manager.get_user_devices(user_id).await;
    assert_eq!(devices_after.len(), 3);
}

#[tokio::test]
async fn test_concurrent_session_management() {
    let session_config = SessionMfaConfig::default();
    let session_manager = Arc::new(SessionMfaManager::new(session_config, None));

    let user_id = "user123";
    let mut handles = vec![];

    // Create 10 concurrent sessions
    for i in 0..10 {
        let manager = session_manager.clone();
        let session_id = format!("session_{:03}", i);
        let uid = user_id.to_string();

        let handle = tokio::spawn(async move {
            let challenge = manager
                .create_challenge(&uid, vec![MfaMethod::Totp])
                .await
                .unwrap();

            manager
                .verify_session_mfa(
                    &session_id,
                    &uid,
                    &challenge.challenge_id,
                    MfaMethod::Totp,
                    None,
                )
                .await
                .unwrap();

            manager.get_session_state(&session_id).await.unwrap()
        });

        handles.push(handle);
    }

    // Wait for all to complete
    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed
    for result in results {
        let state = result.unwrap();
        assert!(state.mfa_verified);
        assert_eq!(state.method, Some(MfaMethod::Totp));
    }

    // Should have 10 active sessions
    assert_eq!(session_manager.active_sessions_count().await, 10);
}

#[tokio::test]
async fn test_complete_user_journey() {
    // Simulate complete user journey from registration to trusted device

    // 1. Registration: Setup TOTP
    let totp_config = TotpConfig::default();
    let totp_secret = TotpSecret::generate(&totp_config).unwrap();
    let totp_verifier = TotpVerifier::new(totp_config);

    // Generate QR code for user
    let qr_code = totp_secret
        .to_qr_code("TestApp", "user@example.com")
        .unwrap();
    assert!(!qr_code.is_empty());

    // 2. Registration: Generate backup codes
    let backup_config = BackupCodeConfig::default();
    let backup_manager = BackupCodeManager::new(backup_config);
    let (plaintext_codes, _stored_codes) = backup_manager.generate().unwrap();
    assert_eq!(plaintext_codes.len(), 10);

    // 3. First login: Verify TOTP, create session
    let device_config = DeviceTrustConfig::default();
    let device_trust = Arc::new(DeviceTrustManager::new(device_config));

    let session_config = SessionMfaConfig::default();
    let session_manager = SessionMfaManager::new(session_config, Some(device_trust.clone()));

    let totp_code = totp_verifier.generate_code(&totp_secret).unwrap();
    assert!(totp_verifier.verify(&totp_secret, &totp_code).unwrap());

    let challenge = session_manager
        .create_challenge("user123", vec![MfaMethod::Totp])
        .await
        .unwrap();

    session_manager
        .verify_session_mfa(
            "session_001",
            "user123",
            &challenge.challenge_id,
            MfaMethod::Totp,
            None,
        )
        .await
        .unwrap();

    // 4. Trust this device
    let fingerprint = DeviceTrustManager::generate_fingerprint("user_agent", "192.168.1.1", None);
    device_trust
        .trust_device(
            "user123",
            &fingerprint,
            "user_agent",
            "192.168.1.1",
            "My Laptop",
        )
        .await
        .unwrap();

    // 5. Second login from same device: No MFA required
    let is_required = session_manager
        .is_mfa_required("user123", "session_002", Some(&fingerprint))
        .await;
    assert!(!is_required);

    // Mark as trusted device session
    session_manager
        .mark_trusted_device("session_002", "user123", &fingerprint)
        .await
        .unwrap();

    let session_state = session_manager
        .get_session_state("session_002")
        .await
        .unwrap();
    assert!(session_state.device_trusted);
    assert_eq!(session_state.method, Some(MfaMethod::TrustedDevice));

    // 6. Login from new device: MFA required
    let new_fingerprint = DeviceTrustManager::generate_fingerprint("new_ua", "10.0.0.1", None);
    let is_required_new = session_manager
        .is_mfa_required("user123", "session_003", Some(&new_fingerprint))
        .await;
    assert!(is_required_new);
}
