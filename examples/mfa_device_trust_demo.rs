//! MFA Device Trust Demo
//!
//! This example demonstrates the device trust functionality of the MFA system,
//! which allows users to trust specific devices and bypass MFA on subsequent logins.
//!
//! Run this example with:
//! ```bash
//! cargo run --example mfa_device_trust_demo --features mfa
//! ```

#[cfg(feature = "mfa")]
use mcp_rs::security::mfa::{DeviceTrustConfig, DeviceTrustManager};

#[cfg(feature = "mfa")]
#[tokio::main]
async fn main() {
    println!("===================================");
    println!("   MFA Device Trust Demonstration  ");
    println!("===================================\n");

    // Create device trust manager with default config
    let config = DeviceTrustConfig::default();
    let manager = DeviceTrustManager::new(config.clone());

    println!("Configuration:");
    println!("  - Enabled: {}", config.enabled);
    println!("  - Max devices per user: {}", config.max_devices_per_user);
    println!(
        "  - Token validity: {} seconds ({} days)",
        config.token_validity_seconds,
        config.token_validity_seconds / 86400
    );
    println!(
        "  - Require MFA on new device: {}\n",
        config.require_mfa_on_new_device
    );

    // Test 1: Generate device fingerprint
    println!("--- Test 1: Device Fingerprint Generation ---");
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0";
    let ip_address = "192.168.1.100";
    let fingerprint = DeviceTrustManager::generate_fingerprint(user_agent, ip_address, None);
    println!("User Agent: {}", user_agent);
    println!("IP Address: {}", ip_address);
    println!("Generated Fingerprint: {}", fingerprint);
    println!("[OK] Fingerprint generated successfully\n");

    // Test 2: Trust a device
    println!("--- Test 2: Trust Device ---");
    let user_id = "user123";
    let device_name = "Windows Desktop - Chrome";
    match manager
        .trust_device(user_id, &fingerprint, user_agent, ip_address, device_name)
        .await
    {
        Ok(_) => println!("[OK] Device trusted successfully"),
        Err(e) => println!("[FAIL] Failed to trust device: {:?}", e),
    }
    println!("Device Name: {}", device_name);
    println!("User ID: {}\n", user_id);

    // Test 3: Check device trust status
    println!("--- Test 3: Verify Device Trust Status ---");
    let is_trusted = manager.is_device_trusted(user_id, &fingerprint).await;
    println!("Device ID: {}", fingerprint);
    println!("Is Trusted: {}", is_trusted);
    if is_trusted {
        println!("[OK] Device is trusted\n");
    } else {
        println!("[FAIL] Device should be trusted\n");
    }

    // Test 4: Update device activity
    println!("--- Test 4: Update Device Activity ---");
    match manager.update_device_activity(user_id, &fingerprint).await {
        Ok(_) => println!("[OK] Device activity updated successfully"),
        Err(e) => println!("[FAIL] Failed to update activity: {:?}", e),
    }
    println!();

    // Test 5: Add multiple devices
    println!("--- Test 5: Trust Multiple Devices ---");
    let devices = vec![
        (
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0)",
            "10.0.0.1",
            "iPhone 15 Pro",
        ),
        (
            "Mozilla/5.0 (Linux; Android 14)",
            "10.0.0.2",
            "Samsung Galaxy S24",
        ),
        (
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)",
            "10.0.0.3",
            "MacBook Pro",
        ),
    ];

    for (ua, ip, name) in &devices {
        let fp = DeviceTrustManager::generate_fingerprint(ua, ip, None);
        match manager.trust_device(user_id, &fp, ua, ip, name).await {
            Ok(_) => println!("[OK] Trusted device: {}", name),
            Err(e) => println!("[FAIL] Failed to trust {}: {:?}", name, e),
        }
    }
    println!();

    // Test 6: Get user devices
    println!("--- Test 6: List User Devices ---");
    let user_devices = manager.get_user_devices(user_id).await;
    println!("Total devices for {}: {}", user_id, user_devices.len());
    for (idx, device) in user_devices.iter().enumerate() {
        println!("\nDevice {}:", idx + 1);
        println!("  Name: {}", device.device_name);
        println!("  User Agent: {}", device.user_agent);
        println!("  IP Address: {}", device.ip_address);
        println!("  Trusted: {}", device.is_trusted);
        println!("  Device ID: {}...", &device.device_id[..16]);
    }
    println!();

    // Test 7: Test max devices limit
    println!("--- Test 7: Test Max Devices Limit ---");
    println!("Current device count: {}", user_devices.len());
    println!("Max devices allowed: {}", config.max_devices_per_user);

    if user_devices.len() < config.max_devices_per_user {
        let remaining = config.max_devices_per_user - user_devices.len();
        println!("Can add {} more devices", remaining);

        // Try to add one more device
        let new_ua = "Mozilla/5.0 (iPad; CPU OS 17_0)";
        let new_ip = "10.0.0.4";
        let new_fp = DeviceTrustManager::generate_fingerprint(new_ua, new_ip, None);
        match manager
            .trust_device(user_id, &new_fp, new_ua, new_ip, "iPad Pro")
            .await
        {
            Ok(_) => println!("[OK] Added new device within limit"),
            Err(e) => println!("[FAIL] Failed to add device: {:?}", e),
        }
    } else {
        println!("[WARNING] At maximum device limit");
    }
    println!();

    // Test 8: Revoke a device
    println!("--- Test 8: Revoke Device ---");
    let revoke_fp = DeviceTrustManager::generate_fingerprint(devices[0].0, devices[0].1, None);
    match manager.revoke_device(user_id, &revoke_fp).await {
        Ok(_) => println!("[OK] Device revoked successfully"),
        Err(e) => println!("[FAIL] Failed to revoke device: {:?}", e),
    }

    let is_still_trusted = manager.is_device_trusted(user_id, &revoke_fp).await;
    println!("Device: {}", devices[0].2);
    println!("Is Trusted After Revocation: {}", is_still_trusted);
    if !is_still_trusted {
        println!("[OK] Device correctly shows as untrusted\n");
    } else {
        println!("[FAIL] Device should be untrusted\n");
    }

    // Test 9: Remove a device
    println!("--- Test 9: Remove Device ---");
    let remove_fp = DeviceTrustManager::generate_fingerprint(devices[1].0, devices[1].1, None);
    match manager.remove_device(user_id, &remove_fp).await {
        Ok(_) => println!("[OK] Device removed successfully"),
        Err(e) => println!("[FAIL] Failed to remove device: {:?}", e),
    }
    println!("Removed Device: {}\n", devices[1].2);

    // Verify device count after removal
    let updated_devices = manager.get_user_devices(user_id).await;
    println!("Devices after removal: {}", updated_devices.len());
    println!();

    // Test 10: Test device expiration
    println!("--- Test 10: Device Expiration Test ---");
    // Create a manager with very short expiration
    let short_config = DeviceTrustConfig {
        enabled: true,
        max_devices_per_user: 5,
        token_validity_seconds: 2, // 2 seconds only
        require_mfa_on_new_device: true,
    };
    let short_manager = DeviceTrustManager::new(short_config.clone());

    let test_user = "test_user";
    let test_fp = DeviceTrustManager::generate_fingerprint("test_ua", "test_ip", None);

    match short_manager
        .trust_device(test_user, &test_fp, "test_ua", "test_ip", "Test Device")
        .await
    {
        Ok(_) => println!("[OK] Test device trusted"),
        Err(e) => println!("[FAIL] Failed to trust test device: {:?}", e),
    }

    println!("Waiting 3 seconds for expiration...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let is_expired = !short_manager.is_device_trusted(test_user, &test_fp).await;
    if is_expired {
        println!(
            "[OK] Device correctly expired after {} seconds\n",
            short_config.token_validity_seconds
        );
    } else {
        println!("[FAIL] Device should have expired\n");
    }

    // Test 11: Test cleanup of expired devices
    println!("--- Test 11: Cleanup Expired Devices ---");
    let before_cleanup = short_manager.get_user_devices(test_user).await.len();
    println!("Devices before cleanup: {}", before_cleanup);

    short_manager.cleanup_expired().await;

    let after_cleanup = short_manager.get_user_devices(test_user).await.len();
    println!("Devices after cleanup: {}", after_cleanup);

    if after_cleanup < before_cleanup {
        println!("[OK] Expired devices cleaned up successfully\n");
    } else {
        println!("[WARNING] No devices were cleaned up\n");
    }

    // Test 12: Test disabled device trust
    println!("--- Test 12: Disabled Device Trust ---");
    let disabled_config = DeviceTrustConfig {
        enabled: false,
        max_devices_per_user: 5,
        token_validity_seconds: 2592000,
        require_mfa_on_new_device: true,
    };
    let disabled_manager = DeviceTrustManager::new(disabled_config);

    let disabled_fp = DeviceTrustManager::generate_fingerprint("disabled_ua", "disabled_ip", None);
    match disabled_manager
        .trust_device(
            "disabled_user",
            &disabled_fp,
            "disabled_ua",
            "disabled_ip",
            "Disabled Test",
        )
        .await
    {
        Ok(_) => println!("[FAIL] Should not be able to trust device when disabled"),
        Err(e) => println!("[OK] Correctly rejected trust operation: {:?}", e),
    }
    println!();

    // Final statistics
    println!("===================================");
    println!("         Final Statistics          ");
    println!("===================================");
    let final_devices = manager.get_user_devices(user_id).await;
    let total_trusted = manager.total_trusted_devices().await;
    println!("Total devices in system: {}", total_trusted);
    println!("Devices for {}: {}", user_id, final_devices.len());
    println!("\nAll tests completed successfully!");
}

#[cfg(not(feature = "mfa"))]
fn main() {
    eprintln!("This example requires the 'mfa' feature to be enabled.");
    eprintln!("Run with: cargo run --example mfa_device_trust_demo --features mfa");
    std::process::exit(1);
}
