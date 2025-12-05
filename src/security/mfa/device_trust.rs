use crate::security::mfa::MfaError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for device trust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTrustConfig {
    /// Whether device trust is enabled
    pub enabled: bool,
    /// Maximum number of trusted devices per user
    pub max_devices_per_user: usize,
    /// Device token validity in seconds (0 = never expires)
    pub token_validity_seconds: u64,
    /// Whether to require MFA on new devices
    pub require_mfa_on_new_device: bool,
}

impl Default for DeviceTrustConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_devices_per_user: 5,
            token_validity_seconds: 2592000, // 30 days
            require_mfa_on_new_device: true,
        }
    }
}

/// Device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Unique device ID (fingerprint hash)
    pub device_id: String,
    /// User agent string
    pub user_agent: String,
    /// IP address
    pub ip_address: String,
    /// Device name/description
    pub device_name: String,
    /// When the device was first trusted
    pub trusted_at: u64,
    /// When the device was last used
    pub last_used_at: u64,
    /// Whether the device is currently trusted
    pub is_trusted: bool,
}

/// Device trust manager
pub struct DeviceTrustManager {
    config: DeviceTrustConfig,
    /// Trusted devices indexed by user_id -> device_id
    devices: Arc<RwLock<HashMap<String, HashMap<String, DeviceInfo>>>>,
}

impl DeviceTrustManager {
    /// Create a new device trust manager
    pub fn new(config: DeviceTrustConfig) -> Self {
        Self {
            config,
            devices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate device fingerprint from device information
    pub fn generate_fingerprint(
        user_agent: &str,
        ip_address: &str,
        additional_data: Option<&str>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(user_agent.as_bytes());
        hasher.update(ip_address.as_bytes());
        if let Some(data) = additional_data {
            hasher.update(data.as_bytes());
        }
        let result = hasher.finalize();
        format!("{:x}", result)
    }

    /// Check if a device is trusted for a user
    pub async fn is_device_trusted(&self, user_id: &str, device_id: &str) -> bool {
        if !self.config.enabled {
            return true; // If disabled, all devices are trusted
        }

        let devices = self.devices.read().await;
        
        if let Some(user_devices) = devices.get(user_id) {
            if let Some(device) = user_devices.get(device_id) {
                if !device.is_trusted {
                    return false;
                }

                // Check if token is expired
                if self.config.token_validity_seconds > 0 {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    
                    let age = now - device.trusted_at;
                    if age > self.config.token_validity_seconds {
                        return false; // Expired
                    }
                }

                return true;
            }
        }

        false
    }

    /// Trust a new device for a user
    pub async fn trust_device(
        &self,
        user_id: &str,
        device_id: &str,
        user_agent: &str,
        ip_address: &str,
        device_name: &str,
    ) -> Result<(), MfaError> {
        if !self.config.enabled {
            return Ok(());
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| MfaError::ConfigError(e.to_string()))?
            .as_secs();

        let mut devices = self.devices.write().await;
        let user_devices = devices.entry(user_id.to_string()).or_insert_with(HashMap::new);

        // Check max devices limit
        if user_devices.len() >= self.config.max_devices_per_user
            && !user_devices.contains_key(device_id)
        {
            return Err(MfaError::ConfigError(
                "Maximum number of trusted devices reached".to_string(),
            ));
        }

        let device_info = DeviceInfo {
            device_id: device_id.to_string(),
            user_agent: user_agent.to_string(),
            ip_address: ip_address.to_string(),
            device_name: device_name.to_string(),
            trusted_at: now,
            last_used_at: now,
            is_trusted: true,
        };

        user_devices.insert(device_id.to_string(), device_info);

        Ok(())
    }

    /// Update last used timestamp for a device
    pub async fn update_device_activity(&self, user_id: &str, device_id: &str) -> Result<(), MfaError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| MfaError::ConfigError(e.to_string()))?
            .as_secs();

        let mut devices = self.devices.write().await;
        
        if let Some(user_devices) = devices.get_mut(user_id) {
            if let Some(device) = user_devices.get_mut(device_id) {
                device.last_used_at = now;
                return Ok(());
            }
        }

        Err(MfaError::NotConfigured)
    }

    /// Revoke trust for a specific device
    pub async fn revoke_device(&self, user_id: &str, device_id: &str) -> Result<(), MfaError> {
        let mut devices = self.devices.write().await;
        
        if let Some(user_devices) = devices.get_mut(user_id) {
            if let Some(device) = user_devices.get_mut(device_id) {
                device.is_trusted = false;
                return Ok(());
            }
        }

        Err(MfaError::NotConfigured)
    }

    /// Remove a device completely
    pub async fn remove_device(&self, user_id: &str, device_id: &str) -> Result<(), MfaError> {
        let mut devices = self.devices.write().await;
        
        if let Some(user_devices) = devices.get_mut(user_id) {
            user_devices.remove(device_id);
            return Ok(());
        }

        Err(MfaError::NotConfigured)
    }

    /// Get all trusted devices for a user
    pub async fn get_user_devices(&self, user_id: &str) -> Vec<DeviceInfo> {
        let devices = self.devices.read().await;
        
        if let Some(user_devices) = devices.get(user_id) {
            user_devices.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Clean up expired device tokens
    pub async fn cleanup_expired(&self) {
        if self.config.token_validity_seconds == 0 {
            return; // Tokens don't expire
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut devices = self.devices.write().await;
        
        for user_devices in devices.values_mut() {
            user_devices.retain(|_, device| {
                let age = now - device.trusted_at;
                age <= self.config.token_validity_seconds && device.is_trusted
            });
        }

        // Remove users with no devices
        devices.retain(|_, user_devices| !user_devices.is_empty());
    }

    /// Get total count of trusted devices across all users
    pub async fn total_trusted_devices(&self) -> usize {
        let devices = self.devices.read().await;
        devices.values().map(|user_devices| user_devices.len()).sum()
    }

    /// Get configuration
    pub fn config(&self) -> &DeviceTrustConfig {
        &self.config
    }
}

#[cfg(all(test, feature = "mfa"))]
mod tests {
    use super::*;

    #[test]
    fn test_device_fingerprint_generation() {
        let fp1 = DeviceTrustManager::generate_fingerprint(
            "Mozilla/5.0",
            "192.168.1.1",
            None,
        );
        let fp2 = DeviceTrustManager::generate_fingerprint(
            "Mozilla/5.0",
            "192.168.1.1",
            None,
        );
        let fp3 = DeviceTrustManager::generate_fingerprint(
            "Mozilla/5.0",
            "192.168.1.2",
            None,
        );

        assert_eq!(fp1, fp2); // Same inputs = same fingerprint
        assert_ne!(fp1, fp3); // Different IP = different fingerprint
        assert_eq!(fp1.len(), 64); // SHA256 hex string
    }

    #[tokio::test]
    async fn test_device_trust() {
        let config = DeviceTrustConfig::default();
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";
        let device_id = "device_fingerprint_abc";

        // Device should not be trusted initially
        assert!(!manager.is_device_trusted(user_id, device_id).await);

        // Trust the device
        manager
            .trust_device(user_id, device_id, "Mozilla/5.0", "192.168.1.1", "My Laptop")
            .await
            .unwrap();

        // Device should now be trusted
        assert!(manager.is_device_trusted(user_id, device_id).await);
    }

    #[tokio::test]
    async fn test_device_revocation() {
        let config = DeviceTrustConfig::default();
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";
        let device_id = "device_abc";

        // Trust and then revoke
        manager
            .trust_device(user_id, device_id, "Mozilla/5.0", "192.168.1.1", "Device")
            .await
            .unwrap();
        
        assert!(manager.is_device_trusted(user_id, device_id).await);

        manager.revoke_device(user_id, device_id).await.unwrap();

        assert!(!manager.is_device_trusted(user_id, device_id).await);
    }

    #[tokio::test]
    async fn test_max_devices_limit() {
        let config = DeviceTrustConfig {
            max_devices_per_user: 2,
            ..Default::default()
        };
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";

        // Trust first device
        manager
            .trust_device(user_id, "device1", "UA1", "IP1", "Device 1")
            .await
            .unwrap();

        // Trust second device
        manager
            .trust_device(user_id, "device2", "UA2", "IP2", "Device 2")
            .await
            .unwrap();

        // Third device should fail
        let result = manager
            .trust_device(user_id, "device3", "UA3", "IP3", "Device 3")
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_device_removal() {
        let config = DeviceTrustConfig::default();
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";
        let device_id = "device_abc";

        manager
            .trust_device(user_id, device_id, "Mozilla/5.0", "192.168.1.1", "Device")
            .await
            .unwrap();

        assert!(manager.is_device_trusted(user_id, device_id).await);

        manager.remove_device(user_id, device_id).await.unwrap();

        assert!(!manager.is_device_trusted(user_id, device_id).await);
    }

    #[tokio::test]
    async fn test_get_user_devices() {
        let config = DeviceTrustConfig::default();
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";

        manager
            .trust_device(user_id, "device1", "UA1", "IP1", "Device 1")
            .await
            .unwrap();
        
        manager
            .trust_device(user_id, "device2", "UA2", "IP2", "Device 2")
            .await
            .unwrap();

        let devices = manager.get_user_devices(user_id).await;
        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn test_device_expiration() {
        let config = DeviceTrustConfig {
            token_validity_seconds: 1, // 1 second
            ..Default::default()
        };
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";
        let device_id = "device_abc";

        manager
            .trust_device(user_id, device_id, "Mozilla/5.0", "192.168.1.1", "Device")
            .await
            .unwrap();

        assert!(manager.is_device_trusted(user_id, device_id).await);

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        assert!(!manager.is_device_trusted(user_id, device_id).await);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let config = DeviceTrustConfig {
            token_validity_seconds: 1,
            ..Default::default()
        };
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";

        manager
            .trust_device(user_id, "device1", "UA1", "IP1", "Device 1")
            .await
            .unwrap();

        assert_eq!(manager.total_trusted_devices().await, 1);

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        manager.cleanup_expired().await;

        assert_eq!(manager.total_trusted_devices().await, 0);
    }

    #[tokio::test]
    async fn test_device_activity_update() {
        let config = DeviceTrustConfig::default();
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";
        let device_id = "device_abc";

        manager
            .trust_device(user_id, device_id, "Mozilla/5.0", "192.168.1.1", "Device")
            .await
            .unwrap();

        let devices_before = manager.get_user_devices(user_id).await;
        let last_used_before = devices_before[0].last_used_at;

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        manager.update_device_activity(user_id, device_id).await.unwrap();

        let devices_after = manager.get_user_devices(user_id).await;
        let last_used_after = devices_after[0].last_used_at;

        assert!(last_used_after > last_used_before);
    }

    #[tokio::test]
    async fn test_disabled_device_trust() {
        let config = DeviceTrustConfig {
            enabled: false,
            ..Default::default()
        };
        let manager = DeviceTrustManager::new(config);

        let user_id = "user123";
        let device_id = "device_abc";

        // When disabled, all devices are trusted
        assert!(manager.is_device_trusted(user_id, device_id).await);

        // Trust operation should succeed but do nothing
        manager
            .trust_device(user_id, device_id, "Mozilla/5.0", "192.168.1.1", "Device")
            .await
            .unwrap();
    }
}
