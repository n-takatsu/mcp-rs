//! Nonce Management System
//!
//! Provides cryptographically secure nonce generation and validation
//! to prevent replay attacks.

use super::types::{NonceEntry, ReplayError};
use base64::Engine;
use chrono::Utc;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, info, warn};

/// Nonce Manager
#[derive(Debug)]
pub struct NonceManager {
    /// In-memory nonce store (nonce -> entry)
    store: Arc<RwLock<HashMap<String, NonceEntry>>>,

    /// Nonce time-to-live
    ttl: Duration,

    /// Maximum cache size
    max_cache_size: usize,

    /// Cleanup interval
    cleanup_interval: Duration,
}

impl NonceManager {
    /// Create a new NonceManager
    pub fn new(ttl_secs: u64, max_cache_size: usize) -> Self {
        let manager = Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_secs),
            max_cache_size,
            cleanup_interval: Duration::from_secs(60), // Cleanup every minute
        };

        // Start background cleanup task
        manager.start_cleanup_task();

        manager
    }

    /// Generate a cryptographically secure nonce
    pub fn generate_nonce() -> String {
        let mut rng = thread_rng();
        let nonce: [u8; 32] = rng.gen();
        base64::engine::general_purpose::STANDARD.encode(nonce)
    }

    /// Validate and consume a nonce
    ///
    /// Returns Ok(()) if the nonce is valid and not used before.
    /// Returns Err if the nonce is duplicated or invalid.
    pub async fn validate_and_consume(
        &self,
        nonce: &str,
        user_id: Option<String>,
    ) -> Result<(), ReplayError> {
        // Check format
        if nonce.is_empty() || nonce.len() > 256 {
            return Err(ReplayError::InvalidNonce(
                "Invalid nonce length".to_string(),
            ));
        }

        let mut store = self.store.write().await;

        // Check if already used
        if let Some(entry) = store.get(nonce) {
            if !entry.is_expired() {
                warn!("Replay attack detected: nonce already used: {}", nonce);
                return Err(ReplayError::NonceAlreadyUsed);
            }
            // Remove expired entry
            store.remove(nonce);
        }

        // Check cache size limit
        if store.len() >= self.max_cache_size {
            warn!(
                "Nonce cache full ({} entries), clearing expired entries",
                store.len()
            );
            self.cleanup_expired_locked(&mut store).await;

            // If still full, reject
            if store.len() >= self.max_cache_size {
                return Err(ReplayError::SecurityViolation(
                    "Nonce cache full".to_string(),
                ));
            }
        }

        // Store the nonce
        let entry = NonceEntry::new(nonce.to_string(), self.ttl.as_secs(), user_id);
        store.insert(nonce.to_string(), entry);

        debug!("Nonce consumed: {} (total: {})", nonce, store.len());

        Ok(())
    }

    /// Check if a nonce is already used (without consuming)
    pub async fn is_used(&self, nonce: &str) -> bool {
        let store = self.store.read().await;
        if let Some(entry) = store.get(nonce) {
            !entry.is_expired()
        } else {
            false
        }
    }

    /// Get current nonce count
    pub async fn nonce_count(&self) -> usize {
        let store = self.store.read().await;
        store.len()
    }

    /// Cleanup expired nonces (locked version for internal use)
    async fn cleanup_expired_locked(&self, store: &mut HashMap<String, NonceEntry>) -> usize {
        let now = Utc::now();
        let before_count = store.len();

        store.retain(|_, entry| entry.expires_at > now);

        let removed = before_count - store.len();
        if removed > 0 {
            debug!("Cleaned up {} expired nonces", removed);
        }

        removed
    }

    /// Cleanup expired nonces
    pub async fn cleanup_expired(&self) -> usize {
        let mut store = self.store.write().await;
        self.cleanup_expired_locked(&mut store).await
    }

    /// Start background cleanup task
    fn start_cleanup_task(&self) {
        let store = Arc::clone(&self.store);
        let cleanup_interval = self.cleanup_interval;
        let ttl = self.ttl;

        tokio::spawn(async move {
            let mut ticker = interval(cleanup_interval);

            loop {
                ticker.tick().await;

                let mut store = store.write().await;
                let now = Utc::now();
                let before_count = store.len();

                store.retain(|_, entry| entry.expires_at > now);

                let removed = before_count - store.len();
                if removed > 0 {
                    info!(
                        "Background cleanup: removed {} expired nonces (TTL: {:?})",
                        removed, ttl
                    );
                }
            }
        });
    }

    /// Clear all nonces (for testing)
    #[cfg(test)]
    pub async fn clear(&self) {
        let mut store = self.store.write().await;
        store.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_nonce() {
        let nonce1 = NonceManager::generate_nonce();
        let nonce2 = NonceManager::generate_nonce();

        assert_ne!(nonce1, nonce2);
        assert!(!nonce1.is_empty());
        assert!(nonce1.len() > 40); // Base64 encoded 32 bytes
    }

    #[tokio::test]
    async fn test_validate_and_consume() {
        let manager = NonceManager::new(300, 1000);
        let nonce = NonceManager::generate_nonce();

        // First use should succeed
        let result = manager.validate_and_consume(&nonce, None).await;
        assert!(result.is_ok());

        // Second use should fail
        let result = manager.validate_and_consume(&nonce, None).await;
        assert!(matches!(result, Err(ReplayError::NonceAlreadyUsed)));
    }

    #[tokio::test]
    async fn test_nonce_expiration() {
        let manager = NonceManager::new(1, 1000); // 1 second TTL
        let nonce = NonceManager::generate_nonce();

        manager.validate_and_consume(&nonce, None).await.unwrap();
        assert!(manager.is_used(&nonce).await);

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should be able to use again after expiration
        let result = manager.validate_and_consume(&nonce, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_nonce_format() {
        let manager = NonceManager::new(300, 1000);

        // Empty nonce
        let result = manager.validate_and_consume("", None).await;
        assert!(matches!(result, Err(ReplayError::InvalidNonce(_))));

        // Too long nonce
        let long_nonce = "a".repeat(300);
        let result = manager.validate_and_consume(&long_nonce, None).await;
        assert!(matches!(result, Err(ReplayError::InvalidNonce(_))));
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let manager = NonceManager::new(1, 1000);

        // Add some nonces
        for _ in 0..10 {
            let nonce = NonceManager::generate_nonce();
            manager.validate_and_consume(&nonce, None).await.unwrap();
        }

        assert_eq!(manager.nonce_count().await, 10);

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Cleanup
        let removed = manager.cleanup_expired().await;
        assert_eq!(removed, 10);
        assert_eq!(manager.nonce_count().await, 0);
    }

    #[tokio::test]
    async fn test_max_cache_size() {
        let manager = NonceManager::new(300, 5); // Max 5 entries

        // Add 5 nonces
        for _ in 0..5 {
            let nonce = NonceManager::generate_nonce();
            manager.validate_and_consume(&nonce, None).await.unwrap();
        }

        assert_eq!(manager.nonce_count().await, 5);

        // 6th should fail
        let nonce = NonceManager::generate_nonce();
        let result = manager.validate_and_consume(&nonce, None).await;
        assert!(matches!(result, Err(ReplayError::SecurityViolation(_))));
    }
}
