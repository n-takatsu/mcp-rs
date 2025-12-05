//! MFA Session Integration Module
//!
//! This module provides session-level MFA integration, allowing MFA verification
//! to be tied to user sessions and enabling device trust for MFA bypass.

use super::{DeviceTrustManager, MfaError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// MFA verification method used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MfaMethod {
    /// Time-based One-Time Password
    Totp,
    /// SMS verification code
    Sms,
    /// Backup recovery code
    BackupCode,
    /// Trusted device (MFA bypassed)
    TrustedDevice,
}

/// MFA challenge state for pending verifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfaChallenge {
    /// User ID for this challenge
    pub user_id: String,
    /// Challenge ID (unique identifier)
    pub challenge_id: String,
    /// When the challenge was created (Unix timestamp)
    pub created_at: u64,
    /// When the challenge expires (Unix timestamp)
    pub expires_at: u64,
    /// Allowed MFA methods for this challenge
    pub allowed_methods: Vec<MfaMethod>,
    /// Number of failed verification attempts
    pub failed_attempts: u32,
    /// Maximum allowed attempts
    pub max_attempts: u32,
}

/// MFA session state for a user session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMfaState {
    /// Session ID
    pub session_id: String,
    /// User ID
    pub user_id: String,
    /// Whether MFA has been verified for this session
    pub mfa_verified: bool,
    /// When MFA was verified (Unix timestamp)
    pub verified_at: Option<u64>,
    /// MFA method used for verification
    pub method: Option<MfaMethod>,
    /// Whether the device is trusted
    pub device_trusted: bool,
    /// Device fingerprint (if available)
    pub device_fingerprint: Option<String>,
}

/// Session MFA configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMfaConfig {
    /// Enable session-level MFA
    pub enabled: bool,
    /// Require MFA for all sessions (ignore device trust)
    pub require_for_all_sessions: bool,
    /// Challenge expiration time in seconds
    pub challenge_expiration_seconds: u64,
    /// Maximum verification attempts per challenge
    pub max_attempts: u32,
    /// Session MFA validity in seconds (0 = entire session)
    pub session_validity_seconds: u64,
}

impl Default for SessionMfaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_for_all_sessions: false,
            challenge_expiration_seconds: 300, // 5 minutes
            max_attempts: 3,
            session_validity_seconds: 0, // Entire session
        }
    }
}

/// Session MFA Manager
///
/// Manages MFA verification at the session level, integrating with device trust
/// to allow MFA bypass for trusted devices.
pub struct SessionMfaManager {
    config: SessionMfaConfig,
    device_trust: Option<Arc<DeviceTrustManager>>,
    sessions: Arc<RwLock<HashMap<String, SessionMfaState>>>,
    challenges: Arc<RwLock<HashMap<String, MfaChallenge>>>,
}

impl SessionMfaManager {
    /// Create a new session MFA manager
    pub fn new(config: SessionMfaConfig, device_trust: Option<Arc<DeviceTrustManager>>) -> Self {
        Self {
            config,
            device_trust,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            challenges: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if MFA is required for a session
    ///
    /// Returns true if MFA verification is required based on:
    /// - Configuration (enabled, require_for_all_sessions)
    /// - Device trust status
    /// - Current session MFA state
    pub async fn is_mfa_required(
        &self,
        user_id: &str,
        session_id: &str,
        device_fingerprint: Option<&str>,
    ) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Check if session already has valid MFA
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            if session.mfa_verified {
                // Check session validity
                if self.config.session_validity_seconds == 0 {
                    return false; // Valid for entire session
                }

                if let Some(verified_at) = session.verified_at {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    if now - verified_at < self.config.session_validity_seconds {
                        return false; // Still within validity period
                    }
                }
            }
        }
        drop(sessions);

        // If require_for_all_sessions is true, always require MFA
        if self.config.require_for_all_sessions {
            return true;
        }

        // Check device trust
        if let (Some(device_trust), Some(fingerprint)) = (&self.device_trust, device_fingerprint) {
            if device_trust.is_device_trusted(user_id, fingerprint).await {
                return false; // Trusted device, no MFA required
            }
        }

        true // MFA required
    }

    /// Create a new MFA challenge
    pub async fn create_challenge(
        &self,
        user_id: &str,
        allowed_methods: Vec<MfaMethod>,
    ) -> Result<MfaChallenge, MfaError> {
        if !self.config.enabled {
            return Err(MfaError::NotConfigured);
        }

        let challenge_id = Self::generate_challenge_id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let challenge = MfaChallenge {
            user_id: user_id.to_string(),
            challenge_id: challenge_id.clone(),
            created_at: now,
            expires_at: now + self.config.challenge_expiration_seconds,
            allowed_methods,
            failed_attempts: 0,
            max_attempts: self.config.max_attempts,
        };

        let mut challenges = self.challenges.write().await;
        challenges.insert(challenge_id.clone(), challenge.clone());

        Ok(challenge)
    }

    /// Verify MFA for a session
    pub async fn verify_session_mfa(
        &self,
        session_id: &str,
        user_id: &str,
        challenge_id: &str,
        method: MfaMethod,
        device_fingerprint: Option<&str>,
    ) -> Result<(), MfaError> {
        if !self.config.enabled {
            return Err(MfaError::NotConfigured);
        }

        // Verify challenge exists and is valid
        let mut challenges = self.challenges.write().await;
        let challenge = challenges
            .get_mut(challenge_id)
            .ok_or(MfaError::InvalidCode)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > challenge.expires_at {
            challenges.remove(challenge_id);
            return Err(MfaError::CodeExpired);
        }

        if challenge.user_id != user_id {
            return Err(MfaError::InvalidCode);
        }

        if !challenge.allowed_methods.contains(&method) {
            return Err(MfaError::InvalidCode);
        }

        // Remove the challenge (it's been used)
        challenges.remove(challenge_id);
        drop(challenges);

        // Create or update session MFA state
        let mut sessions = self.sessions.write().await;
        let session_state = SessionMfaState {
            session_id: session_id.to_string(),
            user_id: user_id.to_string(),
            mfa_verified: true,
            verified_at: Some(now),
            method: Some(method),
            device_trusted: false,
            device_fingerprint: device_fingerprint.map(String::from),
        };

        sessions.insert(session_id.to_string(), session_state);

        Ok(())
    }

    /// Mark a session as having MFA verified via trusted device
    pub async fn mark_trusted_device(
        &self,
        session_id: &str,
        user_id: &str,
        device_fingerprint: &str,
    ) -> Result<(), MfaError> {
        if !self.config.enabled {
            return Err(MfaError::NotConfigured);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut sessions = self.sessions.write().await;
        let session_state = SessionMfaState {
            session_id: session_id.to_string(),
            user_id: user_id.to_string(),
            mfa_verified: true,
            verified_at: Some(now),
            method: Some(MfaMethod::TrustedDevice),
            device_trusted: true,
            device_fingerprint: Some(device_fingerprint.to_string()),
        };

        sessions.insert(session_id.to_string(), session_state);

        Ok(())
    }

    /// Record a failed verification attempt
    pub async fn record_failed_attempt(&self, challenge_id: &str) -> Result<(), MfaError> {
        let mut challenges = self.challenges.write().await;
        let challenge = challenges
            .get_mut(challenge_id)
            .ok_or(MfaError::InvalidCode)?;

        challenge.failed_attempts += 1;

        if challenge.failed_attempts >= challenge.max_attempts {
            challenges.remove(challenge_id);
            return Err(MfaError::TooManyAttempts);
        }

        Ok(())
    }

    /// Get session MFA state
    pub async fn get_session_state(&self, session_id: &str) -> Option<SessionMfaState> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Clear session MFA state (on logout)
    pub async fn clear_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    /// Cleanup expired challenges
    pub async fn cleanup_expired_challenges(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut challenges = self.challenges.write().await;
        challenges.retain(|_, challenge| challenge.expires_at > now);
    }

    /// Get active challenges count
    pub async fn active_challenges_count(&self) -> usize {
        let challenges = self.challenges.read().await;
        challenges.len()
    }

    /// Get active sessions count
    pub async fn active_sessions_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.len()
    }

    /// Generate a unique challenge ID
    fn generate_challenge_id() -> String {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_bytes: Vec<u8> = (0..16).map(|_| rng.gen()).collect();
        // Use base64 encoding instead of hex
        use base64::{engine::general_purpose, Engine as _};
        general_purpose::STANDARD.encode(random_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::mfa::DeviceTrustConfig;

    #[tokio::test]
    async fn test_session_mfa_config_default() {
        let config = SessionMfaConfig::default();
        assert!(config.enabled);
        assert!(!config.require_for_all_sessions);
        assert_eq!(config.challenge_expiration_seconds, 300);
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.session_validity_seconds, 0);
    }

    #[tokio::test]
    async fn test_create_challenge() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let allowed_methods = vec![MfaMethod::Totp, MfaMethod::Sms];
        let challenge = manager
            .create_challenge("user123", allowed_methods.clone())
            .await
            .unwrap();

        assert_eq!(challenge.user_id, "user123");
        assert_eq!(challenge.allowed_methods, allowed_methods);
        assert_eq!(challenge.failed_attempts, 0);
        assert_eq!(challenge.max_attempts, 3);
    }

    #[tokio::test]
    async fn test_mfa_required_without_session() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let required = manager.is_mfa_required("user123", "session123", None).await;
        assert!(required);
    }

    #[tokio::test]
    async fn test_mfa_not_required_after_verification() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        manager
            .verify_session_mfa(
                "session123",
                "user123",
                &challenge.challenge_id,
                MfaMethod::Totp,
                None,
            )
            .await
            .unwrap();

        let required = manager.is_mfa_required("user123", "session123", None).await;
        assert!(!required);
    }

    #[tokio::test]
    async fn test_mfa_not_required_for_trusted_device() {
        let mut config = SessionMfaConfig::default();
        config.require_for_all_sessions = false;

        let device_config = DeviceTrustConfig::default();
        let device_trust = Arc::new(DeviceTrustManager::new(device_config));

        // Trust a device
        let fingerprint = DeviceTrustManager::generate_fingerprint("ua", "ip", None);
        device_trust
            .trust_device("user123", &fingerprint, "ua", "ip", "Test Device")
            .await
            .unwrap();

        let manager = SessionMfaManager::new(config, Some(device_trust));

        let required = manager
            .is_mfa_required("user123", "session123", Some(&fingerprint))
            .await;
        assert!(!required);
    }

    #[tokio::test]
    async fn test_mfa_required_for_all_sessions() {
        let mut config = SessionMfaConfig::default();
        config.require_for_all_sessions = true;

        let device_config = DeviceTrustConfig::default();
        let device_trust = Arc::new(DeviceTrustManager::new(device_config));

        let fingerprint = DeviceTrustManager::generate_fingerprint("ua", "ip", None);
        device_trust
            .trust_device("user123", &fingerprint, "ua", "ip", "Test Device")
            .await
            .unwrap();

        let manager = SessionMfaManager::new(config, Some(device_trust));

        // Even with trusted device, MFA is required
        let required = manager
            .is_mfa_required("user123", "session123", Some(&fingerprint))
            .await;
        assert!(required);
    }

    #[tokio::test]
    async fn test_verify_session_mfa() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        let result = manager
            .verify_session_mfa(
                "session123",
                "user123",
                &challenge.challenge_id,
                MfaMethod::Totp,
                None,
            )
            .await;
        assert!(result.is_ok());

        let state = manager.get_session_state("session123").await.unwrap();
        assert!(state.mfa_verified);
        assert_eq!(state.method, Some(MfaMethod::Totp));
        assert_eq!(state.user_id, "user123");
    }

    #[tokio::test]
    async fn test_verify_with_wrong_user() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        let result = manager
            .verify_session_mfa(
                "session123",
                "wrong_user",
                &challenge.challenge_id,
                MfaMethod::Totp,
                None,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_with_disallowed_method() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        let result = manager
            .verify_session_mfa(
                "session123",
                "user123",
                &challenge.challenge_id,
                MfaMethod::Sms,
                None,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_failed_attempts() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        // First two attempts should succeed
        for _ in 0..2 {
            manager
                .record_failed_attempt(&challenge.challenge_id)
                .await
                .unwrap();
        }

        // Third attempt should fail with TooManyAttempts
        let result = manager.record_failed_attempt(&challenge.challenge_id).await;
        assert!(matches!(result, Err(MfaError::TooManyAttempts)));
    }

    #[tokio::test]
    async fn test_mark_trusted_device() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let fingerprint = "test_fingerprint";
        manager
            .mark_trusted_device("session123", "user123", fingerprint)
            .await
            .unwrap();

        let state = manager.get_session_state("session123").await.unwrap();
        assert!(state.mfa_verified);
        assert!(state.device_trusted);
        assert_eq!(state.method, Some(MfaMethod::TrustedDevice));
        assert_eq!(state.device_fingerprint, Some(fingerprint.to_string()));
    }

    #[tokio::test]
    async fn test_clear_session() {
        let config = SessionMfaConfig::default();
        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        manager
            .verify_session_mfa(
                "session123",
                "user123",
                &challenge.challenge_id,
                MfaMethod::Totp,
                None,
            )
            .await
            .unwrap();

        assert!(manager.get_session_state("session123").await.is_some());

        manager.clear_session("session123").await;

        assert!(manager.get_session_state("session123").await.is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired_challenges() {
        let mut config = SessionMfaConfig::default();
        config.challenge_expiration_seconds = 1; // 1 second

        let manager = SessionMfaManager::new(config, None);

        manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        assert_eq!(manager.active_challenges_count().await, 1);

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        manager.cleanup_expired_challenges().await;

        assert_eq!(manager.active_challenges_count().await, 0);
    }

    #[tokio::test]
    async fn test_challenge_expiration() {
        let mut config = SessionMfaConfig::default();
        config.challenge_expiration_seconds = 1;

        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let result = manager
            .verify_session_mfa(
                "session123",
                "user123",
                &challenge.challenge_id,
                MfaMethod::Totp,
                None,
            )
            .await;
        assert!(matches!(result, Err(MfaError::CodeExpired)));
    }

    #[tokio::test]
    async fn test_session_validity() {
        let mut config = SessionMfaConfig::default();
        config.session_validity_seconds = 1; // 1 second

        let manager = SessionMfaManager::new(config, None);

        let challenge = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await
            .unwrap();

        manager
            .verify_session_mfa(
                "session123",
                "user123",
                &challenge.challenge_id,
                MfaMethod::Totp,
                None,
            )
            .await
            .unwrap();

        // Initially MFA not required
        let required = manager.is_mfa_required("user123", "session123", None).await;
        assert!(!required);

        // Wait for session validity to expire
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // MFA should be required again
        let required = manager.is_mfa_required("user123", "session123", None).await;
        assert!(required);
    }

    #[tokio::test]
    async fn test_disabled_session_mfa() {
        let mut config = SessionMfaConfig::default();
        config.enabled = false;

        let manager = SessionMfaManager::new(config, None);

        // MFA should not be required when disabled
        let required = manager.is_mfa_required("user123", "session123", None).await;
        assert!(!required);

        // Creating challenge should fail
        let result = manager
            .create_challenge("user123", vec![MfaMethod::Totp])
            .await;
        assert!(matches!(result, Err(MfaError::NotConfigured)));
    }
}
