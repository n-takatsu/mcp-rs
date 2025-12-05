//! Multi-Factor Authentication (MFA) Module
//!
//! Provides comprehensive MFA functionality including:
//! - TOTP (Time-based One-Time Password)
//! - Backup codes
//! - Device trust management
//! - Session integration

#[cfg(feature = "mfa")]
pub mod totp;
#[cfg(feature = "mfa")]
pub mod backup_codes;
#[cfg(feature = "mfa")]
pub mod sms;
#[cfg(feature = "mfa")]
pub mod device_trust;
#[cfg(feature = "mfa")]
pub mod session;

#[cfg(feature = "mfa")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "mfa")]
use std::fmt;

/// MFA errors
#[cfg(feature = "mfa")]
#[derive(Debug, Clone)]
pub enum MfaError {
    /// Invalid MFA code
    InvalidCode,

    /// MFA code expired
    CodeExpired,

    /// Too many verification attempts
    TooManyAttempts,

    /// MFA not configured for user
    NotConfigured,

    /// Invalid secret format
    InvalidSecret,

    /// QR code generation failed
    QrCodeError(String),

    /// TOTP error
    TotpError(String),

    /// Cryptographic error
    CryptoError(String),

    /// Configuration error
    ConfigError(String),
}

#[cfg(feature = "mfa")]
impl fmt::Display for MfaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MfaError::InvalidCode => write!(f, "Invalid MFA code"),
            MfaError::CodeExpired => write!(f, "MFA code expired"),
            MfaError::TooManyAttempts => write!(f, "Too many verification attempts"),
            MfaError::NotConfigured => write!(f, "MFA not configured for user"),
            MfaError::InvalidSecret => write!(f, "Invalid secret format"),
            MfaError::QrCodeError(msg) => write!(f, "QR code generation failed: {}", msg),
            MfaError::TotpError(msg) => write!(f, "TOTP error: {}", msg),
            MfaError::CryptoError(msg) => write!(f, "Cryptographic error: {}", msg),
            MfaError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

#[cfg(feature = "mfa")]
impl std::error::Error for MfaError {}

/// MFA configuration
#[cfg(feature = "mfa")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfaConfig {
    /// Enable/disable MFA
    pub enabled: bool,
    /// Require MFA for all users
    pub required_for_all: bool,
    /// Roles that require MFA
    pub required_roles: Vec<String>,
    /// TOTP configuration
    pub totp: totp::TotpConfig,
    /// Backup codes configuration
    pub backup_codes: backup_codes::BackupCodeConfig,
    /// SMS authentication configuration
    pub sms: sms::SmsConfig,
    /// Device trust configuration
    pub device_trust: device_trust::DeviceTrustConfig,
    /// Session-level MFA configuration
    pub session: session::SessionMfaConfig,
}

#[cfg(feature = "mfa")]
impl Default for MfaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            required_for_all: false,
            required_roles: vec!["admin".to_string()],
            totp: totp::TotpConfig::default(),
            backup_codes: backup_codes::BackupCodeConfig::default(),
            sms: sms::SmsConfig::default(),
            device_trust: device_trust::DeviceTrustConfig::default(),
            session: session::SessionMfaConfig::default(),
        }
    }
}

#[cfg(feature = "mfa")]
pub use totp::{TotpAlgorithm, TotpConfig, TotpSecret, TotpVerifier};
#[cfg(feature = "mfa")]
pub use backup_codes::{BackupCode, BackupCodeConfig, BackupCodeManager};
#[cfg(feature = "mfa")]
pub use sms::{SmsAuthenticator, SmsConfig, SmsProviderConfig};
#[cfg(feature = "mfa")]
pub use device_trust::{DeviceInfo, DeviceTrustConfig, DeviceTrustManager};
#[cfg(feature = "mfa")]
pub use session::{MfaChallenge, MfaMethod, SessionMfaConfig, SessionMfaManager, SessionMfaState};
