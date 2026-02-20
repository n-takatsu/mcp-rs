//! Anti-Replay Attack Types and Errors

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use thiserror::Error;

/// Anti-Replay Error Types
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("Missing nonce header")]
    MissingNonce,

    #[error("Nonce already used")]
    NonceAlreadyUsed,

    #[error("Invalid nonce format: {0}")]
    InvalidNonce(String),

    #[error("Missing timestamp header")]
    MissingTimestamp,

    #[error("Timestamp out of acceptable window")]
    TimestampOutOfWindow,

    #[error("Invalid timestamp format: {0}")]
    InvalidTimestamp(String),

    #[error("Request is from the future")]
    FutureTimestamp,

    #[error("Request is too old")]
    ExpiredTimestamp,

    #[error("Device fingerprint mismatch: {0}")]
    DeviceMismatch(String),

    #[error("Unknown device detected")]
    UnknownDevice,

    #[error("Device verification failed: {0}")]
    DeviceVerificationFailed(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Security Headers extracted from request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityHeaders {
    /// Nonce value
    pub nonce: Option<String>,

    /// Request timestamp
    pub timestamp: Option<String>,

    /// Device ID (optional)
    pub device_id: Option<String>,

    /// User agent
    pub user_agent: Option<String>,

    /// Client IP address
    pub ip_address: Option<IpAddr>,
}

impl SecurityHeaders {
    /// Create empty security headers
    pub fn empty() -> Self {
        Self {
            nonce: None,
            timestamp: None,
            device_id: None,
            user_agent: None,
            ip_address: None,
        }
    }

    /// Check if all required headers are present
    pub fn has_required(&self) -> bool {
        self.nonce.is_some() && self.timestamp.is_some()
    }
}

/// Anti-Replay Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiReplayConfig {
    /// Enable nonce validation
    pub enable_nonce: bool,

    /// Enable timestamp validation
    pub enable_timestamp: bool,

    /// Enable device fingerprinting
    pub enable_device_fingerprint: bool,

    /// Time window for timestamp validation (seconds)
    pub time_window_secs: u64,

    /// Nonce TTL (seconds)
    pub nonce_ttl_secs: u64,

    /// Maximum nonce cache size
    pub max_nonce_cache: usize,

    /// Device fingerprint verification strictness (0.0-1.0)
    pub device_match_threshold: f32,

    /// Allow first-time devices without verification
    pub allow_new_devices: bool,

    /// Maximum devices per user
    pub max_devices_per_user: usize,
}

impl Default for AntiReplayConfig {
    fn default() -> Self {
        Self {
            enable_nonce: true,
            enable_timestamp: true,
            enable_device_fingerprint: false, // Optional by default
            time_window_secs: 30,             // ±30 seconds
            nonce_ttl_secs: 300,              // 5 minutes
            max_nonce_cache: 100_000,         // 100k entries
            device_match_threshold: 0.8,      // 80% match
            allow_new_devices: true,
            max_devices_per_user: 5,
        }
    }
}

/// Nonce entry with expiration
#[derive(Debug, Clone)]
pub struct NonceEntry {
    /// Nonce value
    pub nonce: String,

    /// When the nonce was created
    pub created_at: DateTime<Utc>,

    /// When the nonce expires
    pub expires_at: DateTime<Utc>,

    /// User ID associated with this nonce (optional)
    pub user_id: Option<String>,
}

impl NonceEntry {
    /// Create a new nonce entry
    pub fn new(nonce: String, ttl_secs: u64, user_id: Option<String>) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(ttl_secs as i64);

        Self {
            nonce,
            created_at: now,
            expires_at,
            user_id,
        }
    }

    /// Check if the nonce is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_headers_has_required() {
        let mut headers = SecurityHeaders::empty();
        assert!(!headers.has_required());

        headers.nonce = Some("test-nonce".to_string());
        assert!(!headers.has_required());

        headers.timestamp = Some("2026-02-20T10:00:00Z".to_string());
        assert!(headers.has_required());
    }

    #[test]
    fn test_nonce_entry_expiration() {
        let entry = NonceEntry::new("test-nonce".to_string(), 1, None);
        assert!(!entry.is_expired());

        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(entry.is_expired());
    }

    #[test]
    fn test_default_config() {
        let config = AntiReplayConfig::default();
        assert!(config.enable_nonce);
        assert!(config.enable_timestamp);
        assert_eq!(config.time_window_secs, 30);
        assert_eq!(config.nonce_ttl_secs, 300);
    }
}
