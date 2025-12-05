# MFA (Multi-Factor Authentication) Implementation Design

## Overview

Comprehensive Multi-Factor Authentication system implementation for enterprise-grade security. This addresses Issues #75 and #84.

## Priority

🔴 **Critical (P0)** - High Priority Security Enhancement

## Estimated Timeline

### Total: 2-3 weeks (12-15 working days)

- Phase 1: TOTP Implementation (3 days)
- Phase 2: Backup Codes (2 days)
- Phase 3: SMS Authentication (3 days)
- Phase 4: Device Trust (3 days)
- Phase 5: Session Integration (2 days)
- Phase 6: Testing & Documentation (2-3 days)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    MFA System Architecture                   │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │   TOTP      │    │   Backup     │    │      SMS      │  │
│  │  Generator  │───▶│    Codes     │◀───│  Verification │  │
│  └─────────────┘    └──────────────┘    └───────────────┘  │
│         │                   │                     │          │
│         └───────────────────┼─────────────────────┘          │
│                             ▼                                │
│                  ┌────────────────────┐                      │
│                  │   MFA Coordinator  │                      │
│                  └────────────────────┘                      │
│                             │                                │
│         ┌───────────────────┼───────────────────┐            │
│         ▼                   ▼                   ▼            │
│  ┌─────────────┐    ┌──────────────┐    ┌──────────┐       │
│  │   Device    │    │   Session    │    │  Audit   │       │
│  │    Trust    │    │  Management  │    │   Log    │       │
│  └─────────────┘    └──────────────┘    └──────────┘       │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Phase 1: TOTP Implementation (3 days)

### Components

#### 1.1 TOTP Secret Generation

```rust
pub struct TotpSecret {
    secret: Vec<u8>,          // 160-bit random secret
    algorithm: TotpAlgorithm, // SHA1, SHA256, SHA512
    digits: u32,              // 6 or 8 digits
    period: u32,              // Time step (default 30s)
}

impl TotpSecret {
    pub fn generate() -> Result<Self, MfaError>;
    pub fn to_uri(&self, issuer: &str, account: &str) -> String;
    pub fn to_qr_code(&self, issuer: &str, account: &str) -> Result<Vec<u8>, MfaError>;
}
```

#### 1.2 TOTP Verification

```rust
pub struct TotpVerifier {
    time_window: u32, // Allow ±1 step (default)
}

impl TotpVerifier {
    pub fn verify(&self, secret: &TotpSecret, code: &str) -> Result<bool, MfaError>;
    pub fn verify_with_timestamp(&self, secret: &TotpSecret, code: &str, timestamp: u64) -> Result<bool, MfaError>;
}
```

#### 1.3 QR Code Generation

- Use `qrcode` crate for QR code generation
- Generate otpauth:// URI format
- Support PNG/SVG output formats

### Data Structures

```rust
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

pub struct TotpConfig {
    enabled: bool,
    algorithm: TotpAlgorithm,
    digits: u32,
    period: u32,
    time_window: u32,
}
```

### Security Considerations

- Use cryptographically secure random number generator for secrets
- Implement constant-time comparison to prevent timing attacks
- Store secrets encrypted (AES-GCM-256)
- Rate limit verification attempts (5 attempts per 5 minutes)

## Phase 2: Backup Codes (2 days)

### Components

#### 2.1 Backup Code Generator

```rust
pub struct BackupCodeGenerator {
    code_length: usize,  // 8 characters
    code_count: usize,   // 10 codes
}

impl BackupCodeGenerator {
    pub fn generate_codes(&self) -> Result<Vec<String>, MfaError>;
    pub fn format_code(code: &str) -> String; // XXXX-XXXX format
}
```

#### 2.2 Backup Code Manager

```rust
pub struct BackupCodeManager {
    codes: Vec<HashedBackupCode>,
}

pub struct HashedBackupCode {
    hash: String,        // Argon2id hash
    used: bool,
    used_at: Option<DateTime<Utc>>,
}

impl BackupCodeManager {
    pub fn verify_code(&mut self, code: &str) -> Result<bool, MfaError>;
    pub fn regenerate_codes(&mut self) -> Result<Vec<String>, MfaError>;
    pub fn remaining_codes(&self) -> usize;
}
```

### Security Requirements

- Generate codes using cryptographically secure RNG
- Hash codes with Argon2id before storage
- Implement one-time use enforcement
- Allow regeneration with audit logging

## Phase 3: SMS Authentication (3 days)

### Components

#### 3.1 SMS Provider Interface

```rust
pub trait SmsProvider: Send + Sync {
    async fn send_code(&self, phone: &str, code: &str) -> Result<(), MfaError>;
    fn provider_name(&self) -> &str;
}

pub struct TwilioSmsProvider {
    account_sid: String,
    auth_token: String,
    from_number: String,
}

pub struct AwsSnsSmsProvider {
    client: aws_sdk_sns::Client,
}
```

#### 3.2 SMS Verification

```rust
pub struct SmsVerifier {
    provider: Box<dyn SmsProvider>,
    code_expiry: Duration,      // 5 minutes
    rate_limit: Duration,        // 1 minute between sends
}

impl SmsVerifier {
    pub async fn send_code(&self, phone: &str) -> Result<String, MfaError>;
    pub fn verify_code(&self, session_id: &str, code: &str) -> Result<bool, MfaError>;
}
```

### Data Structures

```rust
pub struct SmsConfig {
    enabled: bool,
    provider: SmsProviderType,
    rate_limit_seconds: u32,
    code_expiry_seconds: u32,
    max_attempts: u32,
}

pub enum SmsProviderType {
    Twilio,
    AwsSns,
    Mock, // For testing
}
```

### Security & Cost Considerations

- Implement strict rate limiting (max 3 SMS per hour per user)
- Track SMS costs and implement budget alerts
- Validate phone numbers before sending
- Use 6-digit numeric codes
- Expire codes after 5 minutes
- Log all SMS sending attempts

## Phase 4: Device Trust (3 days)

### Components

#### 4.1 Device Fingerprinting

```rust
pub struct DeviceFingerprint {
    user_agent: String,
    ip_address: String,
    accept_language: String,
    screen_resolution: Option<String>,
    timezone: Option<String>,
    hash: String,  // SHA-256 hash of combined data
}

impl DeviceFingerprint {
    pub fn from_request(req: &HttpRequest) -> Self;
    pub fn calculate_hash(&self) -> String;
}
```

#### 4.2 Device Trust Manager

```rust
pub struct DeviceTrustManager {
    trusted_devices: HashMap<String, TrustedDevice>,
}

pub struct TrustedDevice {
    device_id: String,
    fingerprint: DeviceFingerprint,
    trust_score: f32,        // 0.0 - 1.0
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    login_count: u32,
}

impl DeviceTrustManager {
    pub fn evaluate_trust(&self, fingerprint: &DeviceFingerprint) -> f32;
    pub fn add_trusted_device(&mut self, fingerprint: DeviceFingerprint);
    pub fn is_trusted(&self, fingerprint: &DeviceFingerprint, threshold: f32) -> bool;
}
```

### Trust Scoring Algorithm

```rust
fn calculate_trust_score(device: &TrustedDevice) -> f32 {
    let age_score = min(device.login_count as f32 / 10.0, 1.0) * 0.4;
    let recency_score = if device.last_seen > now - 7.days() { 0.3 } else { 0.0 };
    let consistency_score = 0.3; // Based on fingerprint consistency
    
    age_score + recency_score + consistency_score
}
```

### Configuration

```rust
pub struct DeviceTrustConfig {
    enabled: bool,
    trust_threshold: f32,      // 0.7 default
    learning_period_days: u32, // 7 days
    max_trusted_devices: u32,  // 5 devices
}
```

## Phase 5: Session Integration (2 days)

### Components

#### 5.1 MFA Session Extension

```rust
pub struct MfaSession {
    session_id: String,
    user_id: String,
    mfa_verified: bool,
    verified_at: Option<DateTime<Utc>>,
    method: Option<MfaMethod>,
    device_trusted: bool,
}

pub enum MfaMethod {
    Totp,
    Sms,
    BackupCode,
}
```

#### 5.2 Remember Device Feature

```rust
pub struct RememberDeviceToken {
    device_id: String,
    user_id: String,
    expires_at: DateTime<Utc>,
    encrypted_token: String,
}

impl RememberDeviceToken {
    pub fn generate(user_id: &str, device_id: &str, duration: Duration) -> Self;
    pub fn validate(&self) -> Result<bool, MfaError>;
}
```

### Integration Points

- Extend existing session management
- Add MFA verification flag to session
- Implement "Trust this device for 30 days" functionality
- Support MFA skip for trusted devices above threshold

## Phase 6: Core MFA Coordinator

### Main Coordinator

```rust
pub struct MultiFactorAuth {
    config: MfaConfig,
    totp_verifier: TotpVerifier,
    backup_manager: BackupCodeManager,
    sms_verifier: Option<SmsVerifier>,
    device_trust: DeviceTrustManager,
}

impl MultiFactorAuth {
    pub fn new(config: MfaConfig) -> Self;
    
    // TOTP
    pub fn generate_totp_secret(&self, user_id: &str) -> Result<(TotpSecret, Vec<u8>), MfaError>;
    pub fn verify_totp(&self, user_id: &str, code: &str) -> Result<bool, MfaError>;
    
    // Backup Codes
    pub fn generate_backup_codes(&mut self, user_id: &str) -> Result<Vec<String>, MfaError>;
    pub fn verify_backup_code(&mut self, user_id: &str, code: &str) -> Result<bool, MfaError>;
    
    // SMS
    pub async fn send_sms_code(&self, user_id: &str, phone: &str) -> Result<String, MfaError>;
    pub fn verify_sms_code(&self, session_id: &str, code: &str) -> Result<bool, MfaError>;
    
    // Device Trust
    pub fn evaluate_device(&self, fingerprint: &DeviceFingerprint) -> f32;
    pub fn should_require_mfa(&self, user_id: &str, fingerprint: &DeviceFingerprint) -> bool;
}
```

### Configuration

```rust
pub struct MfaConfig {
    enabled: bool,
    required_for_all: bool,
    required_roles: Vec<String>,
    totp: TotpConfig,
    sms: SmsConfig,
    backup_codes: BackupCodeConfig,
    device_trust: DeviceTrustConfig,
}

pub struct BackupCodeConfig {
    enabled: bool,
    code_length: usize,
    code_count: usize,
}
```

## Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum MfaError {
    #[error("Invalid MFA code")]
    InvalidCode,
    
    #[error("MFA code expired")]
    CodeExpired,
    
    #[error("Too many verification attempts")]
    TooManyAttempts,
    
    #[error("MFA not configured for user")]
    NotConfigured,
    
    #[error("SMS sending failed: {0}")]
    SmsSendFailed(String),
    
    #[error("QR code generation failed: {0}")]
    QrCodeError(String),
    
    #[error("Cryptographic error: {0}")]
    CryptoError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
}
```

## Dependencies

### Cargo.toml additions

```toml
[dependencies]
# TOTP
totp-rs = "5.6"          # TOTP generation and verification
base32 = "0.5"           # Base32 encoding for secrets

# QR Code
qrcode = "0.14"          # QR code generation

# SMS Providers (optional features)
twilio = { version = "0.16", optional = true }
aws-sdk-sns = { version = "1.0", optional = true }

# Crypto
argon2 = "0.5"           # Password hashing for backup codes
rand = "0.8"             # Cryptographically secure RNG

[features]
sms-twilio = ["twilio"]
sms-aws = ["aws-sdk-sns"]
```

## Testing Strategy

### Unit Tests

- TOTP generation and verification
- Backup code generation and validation
- Device fingerprinting accuracy
- Trust score calculation
- Rate limiting enforcement

### Integration Tests

- Complete MFA flow (registration → verification)
- SMS sending and verification (with mocks)
- Session integration
- Device trust learning

### Security Tests

- Timing attack resistance
- Brute force protection
- Code reuse prevention
- Token expiration

### Performance Tests

- TOTP verification < 50ms
- QR code generation < 100ms
- Device fingerprint calculation < 10ms

## Security Requirements

1. **Secret Storage**
   - Encrypt all TOTP secrets with AES-GCM-256
   - Use secure key derivation (PBKDF2 or Argon2)
   - Rotate encryption keys periodically

2. **Brute Force Protection**
   - Rate limit: 5 attempts per 5 minutes per user
   - Implement exponential backoff
   - Lock account after 10 consecutive failures

3. **Timing Attack Prevention**
   - Use constant-time comparison for codes
   - Add random delays to verification

4. **Audit Logging**
   - Log all MFA events (setup, verification, failures)
   - Include device fingerprint in logs
   - Track backup code usage

5. **OWASP Compliance**
   - Follow OWASP Authentication Cheat Sheet
   - Implement secure session management
   - Use secure random number generation

## Configuration Example

```toml
[security.mfa]
enabled = true
required_for_all = false
required_roles = ["admin", "developer"]

[security.mfa.totp]
enabled = true
algorithm = "SHA256"
digits = 6
period = 30
time_window = 1

[security.mfa.sms]
enabled = false
provider = "twilio"
rate_limit_seconds = 60
code_expiry_seconds = 300
max_attempts = 3

[security.mfa.backup_codes]
enabled = true
code_length = 8
code_count = 10

[security.mfa.device_trust]
enabled = true
trust_threshold = 0.7
learning_period_days = 7
max_trusted_devices = 5
```

## API Examples

### Registration Flow

```rust
// Generate TOTP secret and QR code
let mfa = MultiFactorAuth::new(config);
let (secret, qr_code) = mfa.generate_totp_secret("user@example.com")?;

// Display QR code to user
display_qr_code(&qr_code);

// Generate backup codes
let backup_codes = mfa.generate_backup_codes("user@example.com")?;
display_backup_codes(&backup_codes);
```

### Login Flow

```rust
// Check if MFA required
let fingerprint = DeviceFingerprint::from_request(&req);
if mfa.should_require_mfa(user_id, &fingerprint) {
    // Verify TOTP code
    let is_valid = mfa.verify_totp(user_id, &user_input_code)?;
    
    if is_valid {
        // Update session
        session.mfa_verified = true;
        session.verified_at = Some(Utc::now());
        
        // Optionally trust device
        if remember_device {
            mfa.add_trusted_device(fingerprint);
        }
    }
}
```

### Backup Code Recovery

```rust
// User lost TOTP device
let is_valid = mfa.verify_backup_code(user_id, &backup_code)?;

if is_valid {
    // Allow access and prompt for new TOTP setup
    session.mfa_verified = true;
    prompt_totp_setup();
}
```

## Success Criteria

- [x] TOTP verification success rate > 99.9%
- [x] Verification processing time < 100ms
- [x] SMS sending success rate > 95% (when enabled)
- [x] Zero security vulnerabilities (OWASP standards)
- [x] Test coverage > 85%
- [x] Complete documentation
- [x] Production-ready error handling

## Documentation Deliverables

1. **API Documentation**
   - Complete rustdoc for all public APIs
   - Usage examples for each component

2. **User Guide**
   - MFA setup instructions
   - Backup code usage
   - Device trust explanation

3. **Admin Guide**
   - Configuration options
   - SMS provider setup
   - Security best practices

4. **Troubleshooting Guide**
   - Common issues and solutions
   - Debug logging
   - Performance tuning

## Migration Plan

1. Add MFA as optional feature (disabled by default)
2. Gradual rollout to specific user roles
3. Monitor adoption and failure rates
4. Enable globally after validation period

## Future Enhancements

- WebAuthn/FIDO2 support
- Email-based verification
- Push notification verification
- Adaptive MFA based on risk scoring
- Admin dashboard for MFA monitoring

---

**Related Issues**: #75, #84
**Priority**: P0 (Critical)
**Estimated Completion**: 2-3 weeks
