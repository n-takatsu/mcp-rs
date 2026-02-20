//! Anti-Replay Attack Prevention Module
//!
//! This module provides comprehensive protection against replay attacks
//! through multiple security layers:
//!
//! 1. **Nonce Management**: Cryptographically secure, one-time-use tokens
//! 2. **Timestamp Validation**: Strict time window enforcement
//! 3. **Device Fingerprinting**: Device identity verification
//!
//! # Example Usage
//!
//! ```rust
//! use mcp_rs::security::anti_replay::{AntiReplayConfig, AntiReplayMiddleware};
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = AntiReplayConfig::default();
//!     let middleware = Arc::new(AntiReplayMiddleware::new(config));
//!     
//!     // Use with Axum router
//!     // app.layer(middleware::from_fn(move |req, next| {
//!     //     let mw = middleware.clone();
//!     //     async move { mw.middleware(req, next).await }
//!     // }));
//! }
//! ```
//!
//! # Security Headers Required
//!
//! Clients must include these headers in requests:
//!
//! - `X-Nonce`: Base64-encoded 32-byte random value (required)
//! - `X-Timestamp`: RFC3339 UTC timestamp (required)
//! - `X-Device-Id`: Device identifier (optional)
//! - `User-Agent`: Browser/client identifier (recommended)

pub mod device_fingerprint;
pub mod middleware;
pub mod nonce;
pub mod timestamp;
pub mod types;

pub use device_fingerprint::{DeviceFingerprint, DeviceFingerprintManager};
pub use middleware::AntiReplayMiddleware;
pub use nonce::NonceManager;
pub use timestamp::TimestampValidator;
pub use types::{AntiReplayConfig, ReplayError, SecurityHeaders};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_full_anti_replay_workflow() {
        let config = AntiReplayConfig::default();
        let middleware = AntiReplayMiddleware::new(config);

        // Generate nonce
        let nonce = NonceManager::generate_nonce();
        assert!(!nonce.is_empty());

        // Validate nonce
        let result = middleware
            .nonce_manager()
            .validate_and_consume(&nonce, None)
            .await;
        assert!(result.is_ok());

        // Replay should fail
        let result = middleware
            .nonce_manager()
            .validate_and_consume(&nonce, None)
            .await;
        assert!(matches!(result, Err(ReplayError::NonceAlreadyUsed)));

        // Validate timestamp
        let timestamp = Utc::now().to_rfc3339();
        let result = middleware
            .timestamp_validator()
            .validate_timestamp(&timestamp);
        assert!(result.is_ok());

        // Old timestamp should fail
        let old_timestamp = (Utc::now() - chrono::Duration::seconds(100)).to_rfc3339();
        let result = middleware
            .timestamp_validator()
            .validate_timestamp(&old_timestamp);
        assert!(matches!(result, Err(ReplayError::ExpiredTimestamp)));
    }

    #[tokio::test]
    async fn test_device_fingerprint_workflow() {
        let config = AntiReplayConfig {
            enable_device_fingerprint: true,
            allow_new_devices: true,
            ..Default::default()
        };
        let middleware = AntiReplayMiddleware::new(config);

        let user_id = "test-user-123";
        let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100));

        // Create first device
        let fp1 = DeviceFingerprint::new(Some("Mozilla/5.0"), ip, None);

        // Register device
        middleware
            .device_manager()
            .register_device(user_id, fp1.clone())
            .await
            .unwrap();

        // Verify same device
        let verified = middleware
            .device_manager()
            .verify_device(user_id, &fp1, 0.8)
            .await
            .unwrap();
        assert!(verified);

        // Different device
        let fp2 = DeviceFingerprint::new(
            Some("Chrome/90.0"),
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1)),
            None,
        );

        let verified = middleware
            .device_manager()
            .verify_device(user_id, &fp2, 0.8)
            .await
            .unwrap();
        assert!(!verified);
    }

    #[test]
    fn test_config_defaults() {
        let config = AntiReplayConfig::default();

        assert!(config.enable_nonce);
        assert!(config.enable_timestamp);
        assert_eq!(config.time_window_secs, 30);
        assert_eq!(config.nonce_ttl_secs, 300);
        assert_eq!(config.max_nonce_cache, 100_000);
    }
}
