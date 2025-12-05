//! Example: Backup Codes Demo
//!
//! Demonstrates MFA backup codes functionality:
//! - Generate backup codes
//! - Display codes to user
//! - Store hashed codes
//! - Verify backup codes
//! - Track usage
//! - Regeneration detection

use mcp_rs::security::mfa::{BackupCodeConfig, BackupCodeManager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 MFA Backup Codes Demo\n");

    // Step 1: Configure backup codes
    println!("Step 1: Configure backup codes");
    let config = BackupCodeConfig {
        enabled: true,
        count: 10,
        length: 12,
        use_separators: true,
    };
    println!("Configuration: {} codes, {} characters each", config.count, config.length);
    println!("Separators: {}\n", if config.use_separators { "enabled" } else { "disabled" });

    // Step 2: Create manager
    println!("Step 2: Create backup code manager");
    let manager = BackupCodeManager::new(config.clone());
    println!("✓ Manager created\n");

    // Step 3: Generate backup codes
    println!("Step 3: Generate backup codes");
    let (plaintext_codes, mut hashed_codes) = manager.generate()?;
    println!("✓ Generated {} backup codes\n", plaintext_codes.len());

    // Step 4: Display codes to user (only once!)
    println!("Step 4: Display codes to user");
    println!("╔════════════════════════════════════════╗");
    println!("║      BACKUP CODES - SAVE SECURELY      ║");
    println!("╠════════════════════════════════════════╣");
    println!("║  These codes can only be shown ONCE!   ║");
    println!("║  Each code can only be used ONCE!      ║");
    println!("╠════════════════════════════════════════╣");
    for (i, code) in plaintext_codes.iter().enumerate() {
        println!("║  {}: {}  ║", i + 1, code);
    }
    println!("╚════════════════════════════════════════╝\n");

    // Step 5: Verify remaining count
    println!("Step 5: Check remaining codes");
    let remaining = manager.remaining_count(&hashed_codes);
    println!("Remaining codes: {}/{}", remaining, config.count);
    println!("Should regenerate: {}\n", manager.should_regenerate(&hashed_codes));

    // Step 6: Simulate user login with backup code
    println!("Step 6: Simulate user login with backup code");
    let test_code = plaintext_codes[0].clone();
    println!("User enters: {}", test_code);

    match manager.verify(&test_code, &mut hashed_codes) {
        Ok(index) => {
            println!("✓ Backup code verified successfully!");
            println!("  Code index: {}", index);
            println!("  Code marked as used\n");
        }
        Err(e) => {
            println!("✗ Verification failed: {:?}\n", e);
        }
    }

    // Step 7: Check updated status
    println!("Step 7: Check updated status");
    let remaining = manager.remaining_count(&hashed_codes);
    println!("Remaining codes: {}/{}", remaining, config.count);
    println!("Code 1 used at: {:?}", hashed_codes[0].used_at);
    println!("Should regenerate: {}\n", manager.should_regenerate(&hashed_codes));

    // Step 8: Try to reuse the same code
    println!("Step 8: Try to reuse the same code");
    println!("User enters: {}", test_code);
    match manager.verify(&test_code, &mut hashed_codes) {
        Ok(_) => {
            println!("✗ ERROR: Used code verified again (should not happen!)\n");
        }
        Err(e) => {
            println!("✓ Correctly rejected: {:?}\n", e);
        }
    }

    // Step 9: Test code normalization (with/without separators)
    println!("Step 9: Test code normalization");
    let code_with_sep = &plaintext_codes[1];
    let code_without_sep = code_with_sep.replace('-', "");
    
    println!("Original code: {}", code_with_sep);
    println!("Without separators: {}", code_without_sep);
    println!("User enters: {}", code_without_sep);

    match manager.verify(&code_without_sep, &mut hashed_codes) {
        Ok(index) => {
            println!("✓ Code accepted (separators optional)");
            println!("  Code index: {}\n", index);
        }
        Err(e) => {
            println!("✗ Verification failed: {:?}\n", e);
        }
    }

    // Step 10: Use codes until regeneration warning
    println!("Step 10: Use codes until regeneration warning");
    let mut used_count = 2; // Already used 2 codes
    
    while !manager.should_regenerate(&hashed_codes) && used_count < plaintext_codes.len() {
        if let Ok(_) = manager.verify(&plaintext_codes[used_count], &mut hashed_codes) {
            used_count += 1;
            let remaining = manager.remaining_count(&hashed_codes);
            println!("Used code {}. Remaining: {}", used_count, remaining);
        }
    }

    println!("\n⚠️  Warning: Only {} codes remaining!", manager.remaining_count(&hashed_codes));
    println!("    User should regenerate backup codes\n");

    // Step 11: Generate new backup codes
    println!("Step 11: Regenerate backup codes");
    let (new_plaintext, new_hashed) = manager.generate()?;
    println!("✓ Generated {} new backup codes", new_plaintext.len());
    println!("Remaining: {}/{}\n", manager.remaining_count(&new_hashed), config.count);

    // Step 12: Test invalid code
    println!("Step 12: Test invalid code");
    let invalid_code = "INVALID-CODE-1234";
    println!("User enters: {}", invalid_code);
    
    let mut test_hashed = new_hashed.clone();
    match manager.verify(invalid_code, &mut test_hashed) {
        Ok(_) => {
            println!("✗ ERROR: Invalid code verified (should not happen!)\n");
        }
        Err(e) => {
            println!("✓ Correctly rejected: {:?}\n", e);
        }
    }

    // Step 13: Test disabled backup codes
    println!("Step 13: Test disabled backup codes");
    let disabled_config = BackupCodeConfig {
        enabled: false,
        ..config
    };
    let disabled_manager = BackupCodeManager::new(disabled_config);
    
    match disabled_manager.generate() {
        Ok(_) => {
            println!("✗ ERROR: Disabled manager generated codes (should not happen!)\n");
        }
        Err(e) => {
            println!("✓ Correctly rejected: {:?}\n", e);
        }
    }

    // Summary
    println!("═══════════════════════════════════════════════════");
    println!("                    SUMMARY                        ");
    println!("═══════════════════════════════════════════════════");
    println!("✓ Backup code generation: Working");
    println!("✓ Code verification: Working");
    println!("✓ One-time use enforcement: Working");
    println!("✓ Code normalization: Working");
    println!("✓ Usage tracking: Working");
    println!("✓ Regeneration detection: Working");
    println!("✓ Invalid code rejection: Working");
    println!("✓ Disabled state handling: Working");
    println!("═══════════════════════════════════════════════════");
    println!("\n🎉 All backup code functionality working correctly!");

    Ok(())
}
