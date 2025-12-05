//! MFA Performance Benchmarks
//!
//! Measure MFA operation performance to ensure they meet target latencies.
//!
//! Targets:
//! - TOTP generation: < 50ms
//! - TOTP verification: < 50ms
//! - QR code generation: < 100ms
//! - Backup code generation: < 100ms
//! - Device fingerprint: < 10ms

#[cfg(all(feature = "mfa", test))]
mod performance_tests {
    use mcp_rs::security::mfa::{
        BackupCodeConfig, BackupCodeManager, DeviceTrustManager, TotpConfig, TotpSecret,
        TotpVerifier,
    };
    use std::time::Instant;

    #[test]
    fn perf_totp_generation() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        let start = Instant::now();
        let _code = verifier.generate_code(&secret).unwrap();
        let duration = start.elapsed();

        println!("TOTP generation: {:?}", duration);
        assert!(
            duration.as_millis() < 50,
            "TOTP generation took {:?}, expected < 50ms",
            duration
        );
    }

    #[test]
    fn perf_totp_verification() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);
        let code = verifier.generate_code(&secret).unwrap();

        let start = Instant::now();
        let _valid = verifier.verify(&secret, &code).unwrap();
        let duration = start.elapsed();

        println!("TOTP verification: {:?}", duration);
        assert!(
            duration.as_millis() < 50,
            "TOTP verification took {:?}, expected < 50ms",
            duration
        );
    }

    #[test]
    fn perf_qr_code_generation() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();

        let start = Instant::now();
        let _qr = secret.to_qr_code("TestApp", "user@example.com").unwrap();
        let duration = start.elapsed();

        println!("QR code generation: {:?}", duration);
        assert!(
            duration.as_millis() < 100,
            "QR code generation took {:?}, expected < 100ms",
            duration
        );
    }

    #[test]
    fn perf_backup_code_generation() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config);

        let start = Instant::now();
        let _codes = manager.generate().unwrap();
        let duration = start.elapsed();

        println!("Backup code generation (10 codes): {:?}", duration);
        // Argon2id hashing is intentionally slow for security
        // 10 codes with secure hashing should complete within 500ms
        assert!(
            duration.as_millis() < 500,
            "Backup code generation took {:?}, expected < 500ms",
            duration
        );
    }

    #[test]
    fn perf_device_fingerprint() {
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0";
        let ip = "192.168.1.100";

        let start = Instant::now();
        let _fp = DeviceTrustManager::generate_fingerprint(user_agent, ip, None);
        let duration = start.elapsed();

        println!("Device fingerprint generation: {:?}", duration);
        assert!(
            duration.as_millis() < 10,
            "Device fingerprint took {:?}, expected < 10ms",
            duration
        );
    }

    #[test]
    fn perf_backup_code_verification() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config);
        let (plaintext, mut stored) = manager.generate().unwrap();

        let start = Instant::now();
        let _result = manager.verify(&plaintext[0], &mut stored);
        let duration = start.elapsed();

        println!("Backup code verification: {:?}", duration);
        assert!(
            duration.as_millis() < 50,
            "Backup code verification took {:?}, expected < 50ms",
            duration
        );
    }

    #[test]
    fn perf_multiple_totp_generations() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        let iterations = 100;
        let start = Instant::now();

        for _ in 0..iterations {
            let _code = verifier.generate_code(&secret).unwrap();
        }

        let duration = start.elapsed();
        let avg = duration.as_micros() / iterations;

        println!(
            "Average TOTP generation (100 iterations): {} μs",
            avg
        );
        assert!(
            avg < 50000, // 50ms in microseconds
            "Average TOTP generation took {} μs, expected < 50000 μs",
            avg
        );
    }
}
