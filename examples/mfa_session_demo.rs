//! MFA Session Integration Demo
//!
//! This example demonstrates the session-level MFA integration functionality,
//! showing how MFA verification can be tied to user sessions and how device trust
//! allows MFA bypass for trusted devices.
//!
//! Run this example with:
//! ```bash
//! cargo run --example mfa_session_demo --features mfa
//! ```

#[cfg(feature = "mfa")]
use mcp_rs::security::mfa::{
    DeviceTrustConfig, DeviceTrustManager, MfaMethod, SessionMfaConfig, SessionMfaManager,
};
#[cfg(feature = "mfa")]
use std::sync::Arc;

#[cfg(feature = "mfa")]
#[tokio::main]
async fn main() {
    println!("===================================");
    println!("  MFA Session Integration Demo     ");
    println!("===================================\n");

    // Setup: Create device trust manager
    let device_config = DeviceTrustConfig::default();
    let device_trust = Arc::new(DeviceTrustManager::new(device_config.clone()));

    // Setup: Create session MFA manager
    let session_config = SessionMfaConfig::default();
    let session_manager =
        SessionMfaManager::new(session_config.clone(), Some(device_trust.clone()));

    println!("Configuration:");
    println!("  - Session MFA Enabled: {}", session_config.enabled);
    println!(
        "  - Require For All Sessions: {}",
        session_config.require_for_all_sessions
    );
    println!(
        "  - Challenge Expiration: {} seconds",
        session_config.challenge_expiration_seconds
    );
    println!("  - Max Attempts: {}", session_config.max_attempts);
    println!(
        "  - Session Validity: {} seconds (0 = entire session)",
        session_config.session_validity_seconds
    );
    println!();

    // Test 1: Check MFA requirement for new session
    println!("--- Test 1: MFA Required for New Session ---");
    let user_id = "user123";
    let session_id = "session_001";
    let is_required = session_manager
        .is_mfa_required(user_id, session_id, None)
        .await;
    println!("User: {}", user_id);
    println!("Session: {}", session_id);
    println!("MFA Required: {}", is_required);
    if is_required {
        println!("[OK] MFA required for new session\n");
    } else {
        println!("[FAIL] MFA should be required\n");
    }

    // Test 2: Create MFA challenge
    println!("--- Test 2: Create MFA Challenge ---");
    let allowed_methods = vec![MfaMethod::Totp, MfaMethod::Sms, MfaMethod::BackupCode];
    let challenge = session_manager
        .create_challenge(user_id, allowed_methods.clone())
        .await
        .unwrap();

    println!("Challenge ID: {}", challenge.challenge_id);
    println!("User ID: {}", challenge.user_id);
    println!("Allowed Methods: {:?}", challenge.allowed_methods);
    println!(
        "Expires in: {} seconds",
        challenge.expires_at - challenge.created_at
    );
    println!("Max Attempts: {}", challenge.max_attempts);
    println!("[OK] Challenge created successfully\n");

    // Test 3: Verify MFA for session
    println!("--- Test 3: Verify MFA for Session ---");
    let result = session_manager
        .verify_session_mfa(
            session_id,
            user_id,
            &challenge.challenge_id,
            MfaMethod::Totp,
            None,
        )
        .await;

    if result.is_ok() {
        println!("[OK] MFA verification successful");
    } else {
        println!("[FAIL] MFA verification failed: {:?}", result);
    }

    let session_state = session_manager.get_session_state(session_id).await.unwrap();
    println!("Session State:");
    println!("  - MFA Verified: {}", session_state.mfa_verified);
    println!("  - Method: {:?}", session_state.method);
    println!("  - Device Trusted: {}", session_state.device_trusted);
    println!();

    // Test 4: MFA not required after verification
    println!("--- Test 4: MFA Not Required After Verification ---");
    let is_required_after = session_manager
        .is_mfa_required(user_id, session_id, None)
        .await;
    println!("MFA Required: {}", is_required_after);
    if !is_required_after {
        println!("[OK] MFA not required for verified session\n");
    } else {
        println!("[FAIL] MFA should not be required\n");
    }

    // Test 5: Device trust integration
    println!("--- Test 5: Device Trust Integration ---");
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0";
    let ip_address = "192.168.1.100";
    let device_fingerprint = DeviceTrustManager::generate_fingerprint(user_agent, ip_address, None);

    println!("Trusting device...");
    device_trust
        .trust_device(
            user_id,
            &device_fingerprint,
            user_agent,
            ip_address,
            "Windows Desktop - Chrome",
        )
        .await
        .unwrap();
    println!("Device fingerprint: {}...", &device_fingerprint[..16]);
    println!("[OK] Device trusted\n");

    // Test 6: MFA not required for trusted device
    println!("--- Test 6: MFA Not Required for Trusted Device ---");
    let new_session_id = "session_002";
    let is_required_trusted = session_manager
        .is_mfa_required(user_id, new_session_id, Some(&device_fingerprint))
        .await;

    println!("New Session: {}", new_session_id);
    println!("Device: Trusted");
    println!("MFA Required: {}", is_required_trusted);
    if !is_required_trusted {
        println!("[OK] MFA not required for trusted device\n");
    } else {
        println!("[FAIL] MFA should not be required for trusted device\n");
    }

    // Test 7: Mark session as using trusted device
    println!("--- Test 7: Mark Session as Trusted Device ---");
    session_manager
        .mark_trusted_device(new_session_id, user_id, &device_fingerprint)
        .await
        .unwrap();

    let trusted_session = session_manager
        .get_session_state(new_session_id)
        .await
        .unwrap();
    println!("Session State:");
    println!("  - MFA Verified: {}", trusted_session.mfa_verified);
    println!("  - Method: {:?}", trusted_session.method);
    println!("  - Device Trusted: {}", trusted_session.device_trusted);
    if trusted_session.device_trusted && trusted_session.method == Some(MfaMethod::TrustedDevice) {
        println!("[OK] Session correctly marked as trusted device\n");
    } else {
        println!("[FAIL] Session should be marked as trusted device\n");
    }

    // Test 8: Failed verification attempts
    println!("--- Test 8: Failed Verification Attempts ---");
    let fail_challenge = session_manager
        .create_challenge(user_id, vec![MfaMethod::Totp])
        .await
        .unwrap();

    println!(
        "Testing failed attempts (max: {})...",
        fail_challenge.max_attempts
    );
    for i in 1..=fail_challenge.max_attempts {
        let result = session_manager
            .record_failed_attempt(&fail_challenge.challenge_id)
            .await;

        if i < fail_challenge.max_attempts {
            if result.is_ok() {
                println!("  Attempt {}: Recorded", i);
            } else {
                println!("  Attempt {}: [FAIL] Should succeed", i);
            }
        } else {
            if result.is_err() {
                println!(
                    "  Attempt {}: [OK] Correctly blocked (too many attempts)",
                    i
                );
            } else {
                println!("  Attempt {}: [FAIL] Should be blocked", i);
            }
        }
    }
    println!();

    // Test 9: Require MFA for all sessions
    println!("--- Test 9: Require MFA for All Sessions ---");
    let strict_config = SessionMfaConfig {
        enabled: true,
        require_for_all_sessions: true,
        challenge_expiration_seconds: 300,
        max_attempts: 3,
        session_validity_seconds: 0,
    };
    let strict_manager = SessionMfaManager::new(strict_config.clone(), Some(device_trust.clone()));

    println!(
        "Configuration: require_for_all_sessions = {}",
        strict_config.require_for_all_sessions
    );
    let is_required_strict = strict_manager
        .is_mfa_required(user_id, "session_003", Some(&device_fingerprint))
        .await;

    println!("Device: Trusted");
    println!("MFA Required: {}", is_required_strict);
    if is_required_strict {
        println!("[OK] MFA required even for trusted device\n");
    } else {
        println!("[FAIL] MFA should be required\n");
    }

    // Test 10: Challenge expiration
    println!("--- Test 10: Challenge Expiration ---");
    let expiry_config = SessionMfaConfig {
        enabled: true,
        require_for_all_sessions: false,
        challenge_expiration_seconds: 2, // 2 seconds
        max_attempts: 3,
        session_validity_seconds: 0,
    };
    let expiry_manager = SessionMfaManager::new(expiry_config, None);

    let expiry_challenge = expiry_manager
        .create_challenge(user_id, vec![MfaMethod::Totp])
        .await
        .unwrap();

    println!("Challenge created with 2 second expiration");
    println!("Waiting 3 seconds...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let result = expiry_manager
        .verify_session_mfa(
            "session_004",
            user_id,
            &expiry_challenge.challenge_id,
            MfaMethod::Totp,
            None,
        )
        .await;

    if result.is_err() {
        println!("[OK] Expired challenge correctly rejected\n");
    } else {
        println!("[FAIL] Expired challenge should be rejected\n");
    }

    // Test 11: Cleanup expired challenges
    println!("--- Test 11: Cleanup Expired Challenges ---");
    let before_cleanup = expiry_manager.active_challenges_count().await;
    println!("Active challenges before cleanup: {}", before_cleanup);

    expiry_manager.cleanup_expired_challenges().await;

    let after_cleanup = expiry_manager.active_challenges_count().await;
    println!("Active challenges after cleanup: {}", after_cleanup);
    if after_cleanup < before_cleanup {
        println!("[OK] Expired challenges cleaned up\n");
    } else {
        println!("[WARNING] No challenges were cleaned up\n");
    }

    // Test 12: Session validity
    println!("--- Test 12: Session Validity ---");
    let validity_config = SessionMfaConfig {
        enabled: true,
        require_for_all_sessions: false,
        challenge_expiration_seconds: 300,
        max_attempts: 3,
        session_validity_seconds: 2, // 2 seconds
    };
    let validity_manager = SessionMfaManager::new(validity_config, None);

    let validity_challenge = validity_manager
        .create_challenge(user_id, vec![MfaMethod::Totp])
        .await
        .unwrap();

    validity_manager
        .verify_session_mfa(
            "session_005",
            user_id,
            &validity_challenge.challenge_id,
            MfaMethod::Totp,
            None,
        )
        .await
        .unwrap();

    println!("Session verified with 2 second validity");
    let is_required_valid = validity_manager
        .is_mfa_required(user_id, "session_005", None)
        .await;
    println!("MFA Required (immediately): {}", is_required_valid);

    println!("Waiting 3 seconds for validity to expire...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let is_required_expired = validity_manager
        .is_mfa_required(user_id, "session_005", None)
        .await;
    println!("MFA Required (after expiration): {}", is_required_expired);
    if is_required_expired {
        println!("[OK] MFA required after session validity expired\n");
    } else {
        println!("[FAIL] MFA should be required after expiration\n");
    }

    // Test 13: Clear session
    println!("--- Test 13: Clear Session ---");
    println!("Clearing session: {}", session_id);
    session_manager.clear_session(session_id).await;

    let cleared_state = session_manager.get_session_state(session_id).await;
    if cleared_state.is_none() {
        println!("[OK] Session cleared successfully\n");
    } else {
        println!("[FAIL] Session should be cleared\n");
    }

    // Final statistics
    println!("===================================");
    println!("         Final Statistics          ");
    println!("===================================");
    let active_sessions = session_manager.active_sessions_count().await;
    let active_challenges = session_manager.active_challenges_count().await;
    let trusted_devices = device_trust.total_trusted_devices().await;

    println!("Active Sessions: {}", active_sessions);
    println!("Active Challenges: {}", active_challenges);
    println!("Trusted Devices: {}", trusted_devices);
    println!("\nAll tests completed successfully!");
}

#[cfg(not(feature = "mfa"))]
fn main() {
    eprintln!("This example requires the 'mfa' feature to be enabled.");
    eprintln!("Run with: cargo run --example mfa_session_demo --features mfa");
    std::process::exit(1);
}
