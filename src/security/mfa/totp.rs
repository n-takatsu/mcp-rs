//! TOTP (Time-based One-Time Password) Implementation
//!
//! Provides secure TOTP generation and verification for multi-factor authentication.

#[cfg(feature = "mfa")]
use qrcode::QrCode;
#[cfg(feature = "mfa")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "mfa")]
use totp_rs::{Algorithm, Secret, TOTP};

#[cfg(feature = "mfa")]
use super::MfaError;

/// TOTP algorithm selection
#[cfg(feature = "mfa")]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TotpAlgorithm {
    /// SHA-1 (legacy, less secure but widely supported)
    Sha1,
    /// SHA-256 (recommended)
    #[default]
    Sha256,
    /// SHA-512 (most secure)
    Sha512,
}

#[cfg(feature = "mfa")]
impl From<TotpAlgorithm> for Algorithm {
    fn from(algo: TotpAlgorithm) -> Self {
        match algo {
            TotpAlgorithm::Sha1 => Algorithm::SHA1,
            TotpAlgorithm::Sha256 => Algorithm::SHA256,
            TotpAlgorithm::Sha512 => Algorithm::SHA512,
        }
    }
}

/// TOTP configuration
#[cfg(feature = "mfa")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpConfig {
    /// Enable/disable TOTP
    pub enabled: bool,
    /// Hash algorithm
    pub algorithm: TotpAlgorithm,
    /// Number of digits (6 or 8)
    pub digits: usize,
    /// Time step in seconds (typically 30)
    pub period: u64,
    /// Time window for validation (±N steps)
    pub time_window: u8,
}

#[cfg(feature = "mfa")]
impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            algorithm: TotpAlgorithm::Sha256,
            digits: 6,
            period: 30,
            time_window: 1,
        }
    }
}

/// TOTP secret with metadata
#[cfg(feature = "mfa")]
#[derive(Debug, Clone)]
pub struct TotpSecret {
    /// Base32-encoded secret
    secret: String,
    /// Algorithm used
    algorithm: TotpAlgorithm,
    /// Number of digits
    digits: usize,
    /// Time step in seconds
    period: u64,
}

#[cfg(feature = "mfa")]
impl TotpSecret {
    /// Generate a new random TOTP secret
    pub fn generate(config: &TotpConfig) -> Result<Self, MfaError> {
        let secret = Secret::generate_secret();

        Ok(Self {
            secret: secret.to_encoded().to_string(),
            algorithm: config.algorithm,
            digits: config.digits,
            period: config.period,
        })
    }

    /// Create from existing base32-encoded secret
    pub fn from_encoded(secret: String, config: &TotpConfig) -> Result<Self, MfaError> {
        // Basic validation - just check it's not empty
        if secret.is_empty() {
            return Err(MfaError::InvalidSecret);
        }

        Ok(Self {
            secret,
            algorithm: config.algorithm,
            digits: config.digits,
            period: config.period,
        })
    }

    /// Get the base32-encoded secret string
    pub fn encoded(&self) -> &str {
        &self.secret
    }

    /// Generate otpauth:// URI for QR code
    pub fn to_uri(&self, issuer: &str, account: &str) -> String {
        format!(
            "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm={:?}&digits={}&period={}",
            issuer, account, self.secret, issuer, self.algorithm, self.digits, self.period
        )
    }

    /// Generate QR code as PNG bytes
    pub fn to_qr_code(&self, issuer: &str, account: &str) -> Result<Vec<u8>, MfaError> {
        let uri = self.to_uri(issuer, account);

        let code = QrCode::new(&uri).map_err(|e| MfaError::QrCodeError(e.to_string()))?;

        // Render as PNG with 200x200 minimum size
        let image = code
            .render::<qrcode::render::svg::Color>()
            .min_dimensions(200, 200)
            .build();

        // Convert SVG to bytes
        Ok(image.into_bytes())
    }

    /// Get TOTP configuration
    pub fn algorithm(&self) -> TotpAlgorithm {
        self.algorithm
    }

    pub fn digits(&self) -> usize {
        self.digits
    }

    pub fn period(&self) -> u64 {
        self.period
    }
}

/// TOTP verifier
#[cfg(feature = "mfa")]
pub struct TotpVerifier {
    config: TotpConfig,
}

#[cfg(feature = "mfa")]
impl TotpVerifier {
    /// Create a new TOTP verifier
    pub fn new(config: TotpConfig) -> Self {
        Self { config }
    }

    /// Verify a TOTP code against a secret
    pub fn verify(&self, secret: &TotpSecret, code: &str) -> Result<bool, MfaError> {
        if !self.config.enabled {
            return Ok(true);
        }

        // Validate code format
        if code.len() != self.config.digits {
            return Ok(false);
        }

        // Parse code
        let _code_num: u32 = code.parse().map_err(|_| MfaError::InvalidCode)?;

        // Create TOTP instance
        let totp = TOTP::new(
            secret.algorithm.into(),
            secret.digits,
            1, // skew
            secret.period,
            Secret::Encoded(secret.secret.clone())
                .to_bytes()
                .map_err(|_| MfaError::InvalidSecret)?,
            None,           // issuer
            "".to_string(), // account_name
        )
        .map_err(|e| MfaError::TotpError(e.to_string()))?;

        // Verify code
        let is_valid = totp
            .check_current(code)
            .map_err(|e| MfaError::TotpError(e.to_string()))?;

        Ok(is_valid)
    }

    /// Verify with custom timestamp (for testing)
    pub fn verify_with_timestamp(
        &self,
        secret: &TotpSecret,
        code: &str,
        timestamp: u64,
    ) -> Result<bool, MfaError> {
        if !self.config.enabled {
            return Ok(true);
        }

        let totp = TOTP::new(
            secret.algorithm.into(),
            secret.digits,
            1,
            secret.period,
            Secret::Encoded(secret.secret.clone())
                .to_bytes()
                .map_err(|_| MfaError::InvalidSecret)?,
            None,
            "".to_string(),
        )
        .map_err(|e| MfaError::TotpError(e.to_string()))?;

        let is_valid = totp.check(code, timestamp);

        Ok(is_valid)
    }

    /// Generate current TOTP code (for testing)
    pub fn generate_code(&self, secret: &TotpSecret) -> Result<String, MfaError> {
        let totp = TOTP::new(
            secret.algorithm.into(),
            secret.digits,
            1,
            secret.period,
            Secret::Encoded(secret.secret.clone())
                .to_bytes()
                .map_err(|_| MfaError::InvalidSecret)?,
            None,
            "".to_string(),
        )
        .map_err(|e| MfaError::TotpError(e.to_string()))?;

        totp.generate_current()
            .map_err(|e| MfaError::TotpError(e.to_string()))
    }

    /// Get configuration
    pub fn config(&self) -> &TotpConfig {
        &self.config
    }
}

#[cfg(all(test, feature = "mfa"))]
mod tests {
    use super::*;

    #[test]
    fn test_totp_secret_generation() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();

        assert!(!secret.encoded().is_empty());
        assert_eq!(secret.algorithm(), TotpAlgorithm::Sha256);
        assert_eq!(secret.digits(), 6);
        assert_eq!(secret.period(), 30);
    }

    #[test]
    fn test_totp_uri_generation() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();

        let uri = secret.to_uri("TestIssuer", "user@example.com");

        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("TestIssuer"));
        assert!(uri.contains("user@example.com"));
        assert!(uri.contains(secret.encoded()));
    }

    #[test]
    fn test_totp_qr_code_generation() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();

        let qr_code = secret.to_qr_code("TestIssuer", "user@example.com");

        assert!(qr_code.is_ok());
        let svg_data = qr_code.unwrap();
        assert!(!svg_data.is_empty());
        // SVG starts with "<?xml"
        assert_eq!(&svg_data[0..5], b"<?xml");
    }

    #[test]
    fn test_totp_verification() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        // Generate current code
        let code = verifier.generate_code(&secret).unwrap();

        // Verify it
        let is_valid = verifier.verify(&secret, &code).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_totp_invalid_code() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        // Try invalid code
        let is_valid = verifier.verify(&secret, "000000").unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_totp_wrong_length() {
        let config = TotpConfig::default();
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        // Code with wrong length
        let is_valid = verifier.verify(&secret, "12345").unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_totp_time_window() {
        let config = TotpConfig {
            time_window: 1,
            ..Default::default()
        };
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        // Get current timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate code for current time
        let code = verifier.generate_code(&secret).unwrap();

        // Should work at current time
        assert!(verifier.verify_with_timestamp(&secret, &code, now).unwrap());

        // Should work within time window (30 seconds before)
        assert!(verifier
            .verify_with_timestamp(&secret, &code, now - 30)
            .unwrap());

        // Should fail outside time window
        assert!(!verifier
            .verify_with_timestamp(&secret, &code, now - 90)
            .unwrap());
    }

    #[test]
    fn test_totp_disabled() {
        let config = TotpConfig {
            enabled: false,
            ..Default::default()
        };
        let secret = TotpSecret::generate(&TotpConfig::default()).unwrap();
        let verifier = TotpVerifier::new(config);

        // Should always return true when disabled
        let is_valid = verifier.verify(&secret, "invalid").unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_totp_different_algorithms() {
        for algorithm in [
            TotpAlgorithm::Sha1,
            TotpAlgorithm::Sha256,
            TotpAlgorithm::Sha512,
        ] {
            let config = TotpConfig {
                algorithm,
                ..Default::default()
            };
            let secret = TotpSecret::generate(&config).unwrap();
            let verifier = TotpVerifier::new(config);

            let code = verifier.generate_code(&secret).unwrap();
            assert!(verifier.verify(&secret, &code).unwrap());
        }
    }

    #[test]
    fn test_totp_8_digits() {
        let config = TotpConfig {
            digits: 8,
            ..Default::default()
        };
        let secret = TotpSecret::generate(&config).unwrap();
        let verifier = TotpVerifier::new(config);

        let code = verifier.generate_code(&secret).unwrap();
        assert_eq!(code.len(), 8);
        assert!(verifier.verify(&secret, &code).unwrap());
    }
}
