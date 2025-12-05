# MFA Usage Guide

Multi-Factor Authentication (MFA) implementation guide for mcp-rs.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [User Registration Flow](#user-registration-flow)
- [Authentication Flow](#authentication-flow)
- [Device Trust Management](#device-trust-management)
- [Session Integration](#session-integration)
- [Best Practices](#best-practices)
- [Examples](#examples)

## Overview

The MFA system in mcp-rs provides enterprise-grade multi-factor authentication with the following features:

- **TOTP (Time-based One-Time Passwords)**: RFC 6238 compliant with QR code generation
- **Backup Codes**: Secure recovery codes with Argon2id hashing
- **SMS Authentication**: Multi-provider support (Mock, Twilio, AWS SNS, Custom)
- **Device Trust**: Remember trusted devices to reduce MFA friction
- **Session Integration**: Session-level MFA verification with configurable validity

## Quick Start

### 1. Enable MFA Feature

Add to `Cargo.toml`:

```toml
[dependencies]
mcp-rs = { version = "0.15", features = ["mfa"] }
```

### 2. Basic TOTP Setup

```rust
use mcp_rs::security::mfa::{TotpConfig, TotpSecret, TotpVerifier};

// Generate TOTP secret for user
let config = TotpConfig::default();
let secret = TotpSecret::generate(&config)?;
let qr_code = secret.to_qr_code("MyApp", "user@example.com")?;

// Display QR code to user for scanning with authenticator app
// Store secret.encoded() in database

// Verify user's TOTP code
let verifier = TotpVerifier::new(config);
let is_valid = verifier.verify(&secret, &user_code)?;
```

### 3. Generate Backup Codes

```rust
use mcp_rs::security::mfa::{BackupCodeConfig, BackupCodeManager};

let config = BackupCodeConfig::default();
let manager = BackupCodeManager::new(config);
let (plaintext_codes, stored_codes) = manager.generate()?;

// Show plaintext_codes to user once
// Store stored_codes in database
```

### 4. Session Integration

```rust
use mcp_rs::security::mfa::{SessionMfaConfig, SessionMfaManager, MfaMethod};
use std::sync::Arc;

let session_config = SessionMfaConfig::default();
let session_manager = SessionMfaManager::new(session_config, None);

// Check if MFA required
let is_required = session_manager
    .is_mfa_required(user_id, session_id, device_fingerprint)
    .await;

if is_required {
    // Create challenge
    let challenge = session_manager
        .create_challenge(user_id, vec![MfaMethod::Totp, MfaMethod::BackupCode])
        .await?;
    
    // After successful MFA verification
    session_manager
        .verify_session_mfa(session_id, user_id, &challenge.challenge_id, MfaMethod::Totp, None)
        .await?;
}
```

## Configuration

### TOTP Configuration

```toml
[security.mfa.totp]
enabled = true
algorithm = "Sha256"  # Sha1, Sha256, Sha512
digits = 6            # 6 or 8
period = 30           # Time step in seconds
time_window = 1       # Allow ±1 time step
```

### Backup Codes Configuration

```toml
[security.mfa.backup_codes]
enabled = true
count = 10            # Number of codes to generate
length = 12           # Length of each code
use_separators = true # Format: XXXX-XXXX-XXXX
```

### Device Trust Configuration

```toml
[security.mfa.device_trust]
enabled = true
max_devices_per_user = 5          # Max trusted devices
token_validity_seconds = 2592000  # 30 days
require_mfa_on_new_device = true  # Require MFA for new devices
```

### Session MFA Configuration

```toml
[security.mfa.session]
enabled = true
require_for_all_sessions = false      # Force MFA even for trusted devices
challenge_expiration_seconds = 300    # Challenge validity (5 minutes)
max_attempts = 3                      # Max verification attempts
session_validity_seconds = 0          # MFA session validity (0 = entire session)
```

## User Registration Flow

### Step 1: Enable TOTP

```rust
use mcp_rs::security::mfa::{TotpConfig, TotpSecret};

async fn setup_totp_for_user(user_id: &str) -> Result<Vec<u8>, MfaError> {
    let config = TotpConfig::default();
    let secret = TotpSecret::generate(&config)?;
    
    // Generate QR code for user to scan
    let qr_code = secret.to_qr_code("MyApp", user_id)?;
    
    // Store secret in database
    database.store_totp_secret(user_id, secret.encoded()).await?;
    
    Ok(qr_code)
}
```

### Step 2: Verify TOTP Setup

```rust
async fn verify_totp_setup(user_id: &str, code: &str) -> Result<bool, MfaError> {
    let secret_str = database.get_totp_secret(user_id).await?;
    let config = TotpConfig::default();
    let secret = TotpSecret::from_encoded(secret_str, &config)?;
    let verifier = TotpVerifier::new(config);
    
    verifier.verify(&secret, code)
}
```

### Step 3: Generate Backup Codes

```rust
use mcp_rs::security::mfa::{BackupCodeConfig, BackupCodeManager};

async fn generate_backup_codes_for_user(user_id: &str) -> Result<Vec<String>, MfaError> {
    let config = BackupCodeConfig::default();
    let manager = BackupCodeManager::new(config);
    let (plaintext_codes, stored_codes) = manager.generate()?;
    
    // Store hashed codes in database
    database.store_backup_codes(user_id, &stored_codes).await?;
    
    // Return plaintext codes to show user once
    Ok(plaintext_codes)
}
```

## Authentication Flow

### Login with TOTP

```rust
async fn login_with_totp(user_id: &str, password: &str, totp_code: &str) -> Result<Session, Error> {
    // 1. Verify password
    if !verify_password(user_id, password).await? {
        return Err(Error::InvalidCredentials);
    }
    
    // 2. Check if MFA required
    let session_id = generate_session_id();
    let device_fp = generate_device_fingerprint(&request);
    
    if session_manager.is_mfa_required(user_id, &session_id, Some(&device_fp)).await {
        // 3. Verify TOTP
        let secret_str = database.get_totp_secret(user_id).await?;
        let config = TotpConfig::default();
        let secret = TotpSecret::from_encoded(secret_str, &config)?;
        let verifier = TotpVerifier::new(config);
        
        if !verifier.verify(&secret, totp_code)? {
            return Err(Error::InvalidMfaCode);
        }
        
        // 4. Create challenge and verify session
        let challenge = session_manager
            .create_challenge(user_id, vec![MfaMethod::Totp])
            .await?;
        
        session_manager
            .verify_session_mfa(&session_id, user_id, &challenge.challenge_id, MfaMethod::Totp, Some(&device_fp))
            .await?;
    }
    
    // 5. Create session
    Ok(create_session(user_id, session_id).await?)
}
```

### Fallback to Backup Code

```rust
async fn verify_backup_code(user_id: &str, code: &str) -> Result<bool, MfaError> {
    let mut stored_codes = database.get_backup_codes(user_id).await?;
    let config = BackupCodeConfig::default();
    let manager = BackupCodeManager::new(config);
    
    match manager.verify(code, &mut stored_codes) {
        Ok(_index) => {
            // Update stored codes in database
            database.update_backup_codes(user_id, &stored_codes).await?;
            
            // Check if regeneration needed
            if manager.should_regenerate(&stored_codes) {
                // Notify user to regenerate codes
            }
            
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}
```

## Device Trust Management

### Trust a Device

```rust
use mcp_rs::security::mfa::{DeviceTrustConfig, DeviceTrustManager};

async fn trust_current_device(user_id: &str, request: &HttpRequest) -> Result<(), MfaError> {
    let config = DeviceTrustConfig::default();
    let manager = DeviceTrustManager::new(config);
    
    let user_agent = request.headers().get("user-agent").unwrap();
    let ip_address = request.peer_addr().ip().to_string();
    let fingerprint = DeviceTrustManager::generate_fingerprint(user_agent, &ip_address, None);
    
    manager.trust_device(
        user_id,
        &fingerprint,
        user_agent,
        &ip_address,
        "My Device"
    ).await
}
```

### Check Device Trust

```rust
async fn is_device_trusted(user_id: &str, request: &HttpRequest) -> bool {
    let user_agent = request.headers().get("user-agent").unwrap();
    let ip_address = request.peer_addr().ip().to_string();
    let fingerprint = DeviceTrustManager::generate_fingerprint(user_agent, &ip_address, None);
    
    device_manager.is_device_trusted(user_id, &fingerprint).await
}
```

### List User Devices

```rust
async fn list_user_devices(user_id: &str) -> Vec<DeviceInfo> {
    device_manager.get_user_devices(user_id).await
}
```

### Revoke Device

```rust
async fn revoke_device(user_id: &str, device_id: &str) -> Result<(), MfaError> {
    device_manager.revoke_device(user_id, device_id).await
}
```

## Session Integration

### Create MFA Challenge

```rust
let challenge = session_manager
    .create_challenge(
        user_id,
        vec![MfaMethod::Totp, MfaMethod::Sms, MfaMethod::BackupCode]
    )
    .await?;

// Return challenge_id to client
```

### Verify MFA for Session

```rust
// After user provides MFA code
session_manager
    .verify_session_mfa(
        session_id,
        user_id,
        &challenge_id,
        MfaMethod::Totp,
        device_fingerprint
    )
    .await?;
```

### Mark Trusted Device Session

```rust
session_manager
    .mark_trusted_device(session_id, user_id, device_fingerprint)
    .await?;
```

### Clear Session on Logout

```rust
session_manager.clear_session(session_id).await;
```

## Best Practices

### Security

1. **Always use HTTPS**: MFA codes must be transmitted over secure connections
2. **Rate limiting**: Implement rate limiting on MFA endpoints to prevent brute force
3. **Secure storage**: Store TOTP secrets and backup code hashes securely
4. **Audit logging**: Log all MFA events (setup, verification, failures)
5. **Device fingerprinting**: Combine multiple factors for device identification

### User Experience

1. **Clear instructions**: Provide step-by-step setup guides with screenshots
2. **Backup codes**: Generate and show backup codes immediately after TOTP setup
3. **Device trust**: Allow users to trust devices for 30 days
4. **Regeneration warnings**: Notify users when backup codes are running low
5. **Alternative methods**: Always provide backup code fallback

### Performance

1. **Cache TOTP secrets**: Cache decoded secrets in memory
2. **Async operations**: Use async/await for all I/O operations
3. **Connection pooling**: Use connection pools for database access
4. **Cleanup tasks**: Run periodic cleanup for expired challenges and devices

### Testing

1. **Unit tests**: Test each MFA component independently
2. **Integration tests**: Test complete authentication flows
3. **Performance tests**: Verify latency targets are met
4. **Security tests**: Test for timing attacks and brute force resistance

## Examples

Complete examples are available in the `examples/` directory:

- `mfa_totp_demo.rs`: TOTP generation and verification
- `mfa_backup_codes_demo.rs`: Backup code lifecycle
- `mfa_sms_demo.rs`: SMS authentication (mock provider)
- `mfa_device_trust_demo.rs`: Device trust management
- `mfa_session_demo.rs`: Session-level MFA integration

Run an example:

```bash
cargo run --example mfa_totp_demo --features mfa
```

## Troubleshooting

### TOTP codes don't match

- Check system time synchronization
- Verify time_window setting (default ±1 step)
- Ensure secret encoding is correct (base32)

### Backup codes always fail

- Verify code normalization (remove separators)
- Check hash algorithm configuration
- Ensure codes aren't already used

### Device trust not working

- Verify device fingerprint generation is consistent
- Check token_validity_seconds hasn't expired
- Ensure max_devices_per_user limit isn't reached

### Session MFA keeps requiring verification

- Check session_validity_seconds setting
- Verify session state is persisted correctly
- Ensure challenge IDs match

## Performance Metrics

Measured latencies (typical):

- TOTP generation: < 10 μs
- TOTP verification: < 10 μs
- QR code generation: < 3 ms
- Backup code generation (10 codes): < 200 ms
- Backup code verification: < 20 ms
- Device fingerprint: < 100 μs

## Further Reading

- [RFC 6238 - TOTP](https://tools.ietf.org/html/rfc6238)
- [RFC 4226 - HOTP](https://tools.ietf.org/html/rfc4226)
- [OWASP MFA Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Multifactor_Authentication_Cheat_Sheet.html)
- [Argon2 Password Hashing](https://github.com/P-H-C/phc-winner-argon2)
