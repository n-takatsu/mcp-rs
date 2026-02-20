//! Device Fingerprinting
//!
//! Generates and validates device fingerprints based on User-Agent,
//! IP address, and TLS certificate information to detect device changes
//! and potential session hijacking.

use super::types::ReplayError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Device Fingerprint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceFingerprint {
    /// SHA256 hash of User-Agent
    pub user_agent_hash: String,

    /// Client IP address
    pub ip_address: IpAddr,

    /// TLS fingerprint (JA3 hash)
    pub tls_fingerprint: Option<String>,

    /// Unique device ID (composite hash)
    pub device_id: String,

    /// When this fingerprint was first seen
    pub created_at: DateTime<Utc>,

    /// Last time this fingerprint was used
    pub last_seen_at: DateTime<Utc>,

    /// Number of times this fingerprint was used
    pub usage_count: u64,
}

impl DeviceFingerprint {
    /// Create a device fingerprint from request components
    pub fn new(
        user_agent: Option<&str>,
        ip_address: IpAddr,
        tls_fingerprint: Option<String>,
    ) -> Self {
        let user_agent_hash = Self::hash_user_agent(user_agent);
        let device_id = Self::generate_device_id(&user_agent_hash, &ip_address, &tls_fingerprint);

        let now = Utc::now();

        Self {
            user_agent_hash,
            ip_address,
            tls_fingerprint,
            device_id,
            created_at: now,
            last_seen_at: now,
            usage_count: 1,
        }
    }

    /// Hash the User-Agent string
    fn hash_user_agent(user_agent: Option<&str>) -> String {
        let ua = user_agent.unwrap_or("unknown");
        let mut hasher = Sha256::new();
        hasher.update(ua.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Generate a unique device ID from components
    fn generate_device_id(
        user_agent_hash: &str,
        ip_address: &IpAddr,
        tls_fingerprint: &Option<String>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(user_agent_hash.as_bytes());
        hasher.update(ip_address.to_string().as_bytes());
        if let Some(tls) = tls_fingerprint {
            hasher.update(tls.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    /// Calculate similarity with another fingerprint (0.0 - 1.0)
    pub fn similarity(&self, other: &DeviceFingerprint) -> f32 {
        let mut score = 0.0;
        let mut total_weight = 0.0;

        // User-Agent hash match (weight: 0.4)
        if self.user_agent_hash == other.user_agent_hash {
            score += 0.4;
        }
        total_weight += 0.4;

        // IP address match (weight: 0.5)
        if self.ip_address == other.ip_address {
            score += 0.5;
        }
        total_weight += 0.5;

        // TLS fingerprint match (weight: 0.1)
        match (&self.tls_fingerprint, &other.tls_fingerprint) {
            (Some(a), Some(b)) if a == b => {
                score += 0.1;
                total_weight += 0.1;
            }
            (None, None) => {
                // Both don't have TLS info, considered a match
                score += 0.1;
                total_weight += 0.1;
            }
            _ => {
                // One has TLS info, one doesn't - slight penalty
                total_weight += 0.1;
            }
        }

        score / total_weight
    }

    /// Check if this fingerprint matches another within a threshold
    pub fn matches(&self, other: &DeviceFingerprint, threshold: f32) -> bool {
        self.similarity(other) >= threshold
    }

    /// Update last seen timestamp and usage count
    pub fn touch(&mut self) {
        self.last_seen_at = Utc::now();
        self.usage_count += 1;
    }
}

/// Device Fingerprint Manager
pub struct DeviceFingerprintManager {
    /// Trusted devices per user (user_id -> Vec<DeviceFingerprint>)
    trusted_devices: Arc<RwLock<HashMap<String, Vec<DeviceFingerprint>>>>,

    /// Maximum devices per user
    max_devices_per_user: usize,

    /// Device inactivity threshold (days)
    device_inactive_days: i64,
}

impl DeviceFingerprintManager {
    /// Create a new DeviceFingerprintManager
    pub fn new(max_devices_per_user: usize) -> Self {
        Self {
            trusted_devices: Arc::new(RwLock::new(HashMap::new())),
            max_devices_per_user,
            device_inactive_days: 90, // Remove devices inactive for 90 days
        }
    }

    /// Register a new device for a user
    pub async fn register_device(
        &self,
        user_id: &str,
        fingerprint: DeviceFingerprint,
    ) -> Result<(), ReplayError> {
        let mut devices = self.trusted_devices.write().await;
        let user_devices = devices.entry(user_id.to_string()).or_insert_with(Vec::new);

        // Check if device already exists
        if let Some(existing) = user_devices
            .iter_mut()
            .find(|d| d.device_id == fingerprint.device_id)
        {
            existing.touch();
            info!(
                "Device already registered for user {}: {}",
                user_id, fingerprint.device_id
            );
            return Ok(());
        }

        // Check device limit
        if user_devices.len() >= self.max_devices_per_user {
            // Remove oldest inactive device
            user_devices.sort_by_key(|d| d.last_seen_at);
            user_devices.remove(0);
            info!(
                "Removed oldest device for user {} (limit: {})",
                user_id, self.max_devices_per_user
            );
        }

        user_devices.push(fingerprint.clone());
        info!(
            "Registered new device for user {}: {} (total: {})",
            user_id,
            fingerprint.device_id,
            user_devices.len()
        );

        Ok(())
    }

    /// Verify a device fingerprint for a user
    pub async fn verify_device(
        &self,
        user_id: &str,
        fingerprint: &DeviceFingerprint,
        threshold: f32,
    ) -> Result<bool, ReplayError> {
        let mut devices = self.trusted_devices.write().await;
        let user_devices = devices.get_mut(user_id);

        if user_devices.is_none() {
            debug!("No trusted devices for user {}", user_id);
            return Ok(false);
        }

        let user_devices = user_devices.unwrap();

        // Find matching device
        for device in user_devices.iter_mut() {
            if device.matches(fingerprint, threshold) {
                device.touch();
                debug!(
                    "Device verified for user {}: {} (similarity: {:.2})",
                    user_id,
                    device.device_id,
                    device.similarity(fingerprint)
                );
                return Ok(true);
            }
        }

        warn!(
            "Unknown device detected for user {}: {}",
            user_id, fingerprint.device_id
        );
        Ok(false)
    }

    /// Get trusted devices for a user
    pub async fn get_user_devices(&self, user_id: &str) -> Vec<DeviceFingerprint> {
        let devices = self.trusted_devices.read().await;
        devices.get(user_id).cloned().unwrap_or_default()
    }

    /// Remove a specific device for a user
    pub async fn remove_device(&self, user_id: &str, device_id: &str) -> Result<(), ReplayError> {
        let mut devices = self.trusted_devices.write().await;
        if let Some(user_devices) = devices.get_mut(user_id) {
            user_devices.retain(|d| d.device_id != device_id);
            info!("Removed device {} for user {}", device_id, user_id);
        }
        Ok(())
    }

    /// Clean up inactive devices for all users
    pub async fn cleanup_inactive_devices(&self) -> usize {
        let mut devices = self.trusted_devices.write().await;
        let threshold = Utc::now() - chrono::Duration::days(self.device_inactive_days);
        let mut total_removed = 0;

        for (_user_id, user_devices) in devices.iter_mut() {
            let before = user_devices.len();
            user_devices.retain(|d| d.last_seen_at > threshold);
            total_removed += before - user_devices.len();
        }

        if total_removed > 0 {
            info!("Cleaned up {} inactive devices", total_removed);
        }

        total_removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_device_fingerprint_creation() {
        let fp = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            None,
        );

        assert!(!fp.user_agent_hash.is_empty());
        assert!(!fp.device_id.is_empty());
        assert_eq!(fp.usage_count, 1);
    }

    #[test]
    fn test_fingerprint_similarity() {
        let fp1 = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            None,
        );

        let fp2 = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            None,
        );

        let fp3 = DeviceFingerprint::new(
            Some("Chrome/90.0"),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            None,
        );

        assert_eq!(fp1.similarity(&fp2), 1.0); // Identical
        assert!(fp1.similarity(&fp3) < 0.5); // Different
    }

    #[test]
    fn test_fingerprint_matches() {
        let fp1 = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            None,
        );

        let fp2 = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            None,
        );

        assert!(fp1.matches(&fp2, 0.8));
    }

    #[tokio::test]
    async fn test_register_and_verify_device() {
        let manager = DeviceFingerprintManager::new(5);
        let user_id = "user123";

        let fp = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            None,
        );

        // Register device
        manager.register_device(user_id, fp.clone()).await.unwrap();

        // Verify same device
        let is_verified = manager.verify_device(user_id, &fp, 0.8).await.unwrap();
        assert!(is_verified);

        // Different device should not verify
        let different_fp = DeviceFingerprint::new(
            Some("Chrome/90.0"),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
            None,
        );

        let is_verified = manager
            .verify_device(user_id, &different_fp, 0.8)
            .await
            .unwrap();
        assert!(!is_verified);
    }

    #[tokio::test]
    async fn test_max_devices_per_user() {
        let manager = DeviceFingerprintManager::new(2);
        let user_id = "user123";

        // Register 3 devices
        for i in 0..3 {
            let fp = DeviceFingerprint::new(
                Some(&format!("Browser-{}", i)),
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, i as u8)),
                None,
            );
            manager.register_device(user_id, fp).await.unwrap();
        }

        // Should only have 2 devices (oldest removed)
        let devices = manager.get_user_devices(user_id).await;
        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_device() {
        let manager = DeviceFingerprintManager::new(5);
        let user_id = "user123";

        let fp = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            None,
        );

        manager.register_device(user_id, fp.clone()).await.unwrap();

        assert_eq!(manager.get_user_devices(user_id).await.len(), 1);

        manager.remove_device(user_id, &fp.device_id).await.unwrap();

        assert_eq!(manager.get_user_devices(user_id).await.len(), 0);
    }

    #[test]
    fn test_ipv6_fingerprint() {
        let fp = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
            None,
        );

        assert!(!fp.device_id.is_empty());
    }

    #[test]
    fn test_fingerprint_with_tls() {
        let fp1 = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            Some("ja3-hash-123".to_string()),
        );

        let fp2 = DeviceFingerprint::new(
            Some("Mozilla/5.0"),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            Some("ja3-hash-123".to_string()),
        );

        assert_eq!(fp1.similarity(&fp2), 1.0);
    }
}
