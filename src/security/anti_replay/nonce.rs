//! Nonce Management System
//!
//! Provides cryptographically secure nonce generation and validation
//! to prevent replay attacks.

use super::types::{NonceEntry, ReplayError};
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::{thread_rng, Rng};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, warn};

/// 使ったnonceの記録。呼び出し元ごとに分ける（#543）
#[derive(Debug, Default)]
struct NonceStore {
    /// 呼び出し元 → nonce → 記録
    scopes: HashMap<String, HashMap<String, NonceEntry>>,
    /// すべての呼び出し元の記録の数
    total: usize,
    /// 記録の中で、いちばん早く期限が切れる時刻（無ければ`None`）。それより
    /// 前は、消せる記録が無いので、全件を見ない（#724）
    earliest_expiry: Option<DateTime<Utc>>,
}

impl NonceStore {
    /// 期限の過ぎた記録を消し、消した数を返す
    fn remove_expired(&mut self) -> usize {
        let now = Utc::now();
        let before = self.total;
        self.scopes.retain(|_, nonces| {
            nonces.retain(|_, entry| entry.expires_at > now);
            !nonces.is_empty()
        });
        self.total = self.scopes.values().map(HashMap::len).sum();
        self.earliest_expiry = self
            .scopes
            .values()
            .flat_map(HashMap::values)
            .map(|entry| entry.expires_at)
            .min();
        before - self.total
    }

    /// 期限の過ぎた記録があるときだけ、消す（#724）。以前は、上限に達した
    /// 呼び出し元が送るたびに、書き込みのロックを持ったまま全件を見たので、
    /// ほかの呼び出し元も待たされた
    fn remove_expired_if_due(&mut self) -> usize {
        match self.earliest_expiry {
            Some(earliest) if earliest <= Utc::now() => self.remove_expired(),
            _ => 0,
        }
    }

    /// 記録を足したときに、いちばん早い期限を更新する
    fn note_expiry(&mut self, expires_at: DateTime<Utc>) {
        if self
            .earliest_expiry
            .is_none_or(|earliest| expires_at < earliest)
        {
            self.earliest_expiry = Some(expires_at);
        }
    }
}

/// Nonce Manager
///
/// Remembers the nonces each caller has used, so that the same nonce is
/// accepted only once per caller. The callers are kept apart (#543): one
/// caller filling its share of the cache does not stop the others.
#[derive(Debug)]
pub struct NonceManager {
    /// In-memory nonce store
    store: Arc<RwLock<NonceStore>>,

    /// Nonce time-to-live
    ttl: Duration,

    /// Maximum cache size (all callers)
    max_cache_size: usize,

    /// Maximum cache size for one caller (#543)
    max_per_caller: usize,
}

impl NonceManager {
    /// Create a new NonceManager. Each caller may use the whole cache
    /// (`max_cache_size`); see `with_limits` to limit each caller.
    pub fn new(ttl_secs: u64, max_cache_size: usize) -> Self {
        Self::with_limits(ttl_secs, max_cache_size, max_cache_size)
    }

    /// Create a new NonceManager that keeps at most `max_per_caller` nonces
    /// for one caller (#543), and `max_cache_size` in all.
    pub fn with_limits(ttl_secs: u64, max_cache_size: usize, max_per_caller: usize) -> Self {
        let manager = Self {
            store: Arc::new(RwLock::new(NonceStore::default())),
            ttl: Duration::from_secs(ttl_secs),
            max_cache_size,
            max_per_caller,
        };

        // Start background cleanup task
        manager.start_cleanup_task(Duration::from_secs(60));

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
    /// Returns Ok(()) if the nonce is valid and not used before by the same
    /// caller (`user_id`; `None` is one caller of its own). Returns Err if
    /// the nonce is duplicated or invalid, or if the caller's share of the
    /// cache, or the whole cache, is full of unexpired nonces.
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

        let scope = user_id.clone().unwrap_or_default();
        let mut store = self.store.write().await;

        // Check if already used
        if let Some(entry) = store.scopes.get(&scope).and_then(|n| n.get(nonce)) {
            if !entry.is_expired() {
                warn!("Replay attack detected: nonce already used");
                return Err(ReplayError::NonceAlreadyUsed);
            }
        }

        // Check cache size limits
        let caller_count = store.scopes.get(&scope).map_or(0, HashMap::len);
        if caller_count >= self.max_per_caller || store.total >= self.max_cache_size {
            let removed = store.remove_expired_if_due();
            debug!("Nonce cache full, removed {} expired entries", removed);

            let caller_count = store.scopes.get(&scope).map_or(0, HashMap::len);
            if caller_count >= self.max_per_caller {
                warn!("Nonce cache full for one caller ({} entries)", caller_count);
                return Err(ReplayError::SecurityViolation(
                    "Nonce cache full for this caller".to_string(),
                ));
            }
            if store.total >= self.max_cache_size {
                warn!("Nonce cache full ({} entries)", store.total);
                return Err(ReplayError::SecurityViolation(
                    "Nonce cache full".to_string(),
                ));
            }
        }

        // Store the nonce (replacing an expired entry, if any)
        let entry = NonceEntry::new(nonce.to_string(), self.ttl.as_secs(), user_id);
        store.note_expiry(entry.expires_at);
        if store
            .scopes
            .entry(scope)
            .or_default()
            .insert(nonce.to_string(), entry)
            .is_none()
        {
            store.total += 1;
        }

        debug!("Nonce consumed (total: {})", store.total);

        Ok(())
    }

    /// Check if a nonce is already used by any caller (without consuming)
    pub async fn is_used(&self, nonce: &str) -> bool {
        let store = self.store.read().await;
        store
            .scopes
            .values()
            .filter_map(|nonces| nonces.get(nonce))
            .any(|entry| !entry.is_expired())
    }

    /// Get current nonce count
    pub async fn nonce_count(&self) -> usize {
        self.store.read().await.total
    }

    /// Cleanup expired nonces
    pub async fn cleanup_expired(&self) -> usize {
        let removed = self.store.write().await.remove_expired();
        if removed > 0 {
            debug!("Cleaned up {} expired nonces", removed);
        }
        removed
    }

    /// Start background cleanup task. It ends when the manager is dropped
    /// (#543; previously it ran forever)
    fn start_cleanup_task(&self, cleanup_interval: Duration) {
        let store = Arc::downgrade(&self.store);
        let ttl = self.ttl;

        tokio::spawn(async move {
            let mut ticker = interval(cleanup_interval);

            loop {
                ticker.tick().await;
                let Some(store) = store.upgrade() else {
                    return;
                };
                let removed = store.write().await.remove_expired();
                if removed > 0 {
                    debug!(
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
        *self.store.write().await = NonceStore::default();
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

    /// いちばん早い期限より前は、全件を見ない。期限を過ぎていれば消す（#724）
    #[test]
    fn test_remove_expired_only_when_due() {
        let mut store = NonceStore::default();
        assert_eq!(store.remove_expired_if_due(), 0);
        let mut add = |nonce: &str, ttl_secs: u64| {
            let entry = NonceEntry::new(nonce.to_string(), ttl_secs, None);
            store.note_expiry(entry.expires_at);
            store
                .scopes
                .entry(String::new())
                .or_default()
                .insert(nonce.to_string(), entry);
            store.total += 1;
        };
        add("a", 300);
        add("b", 600);
        let earliest = store.earliest_expiry.unwrap();
        // まだ期限の切れた記録は無い
        assert_eq!(store.remove_expired_if_due(), 0);
        assert_eq!(store.total, 2);
        // いちばん早い期限を過ぎたことにすると、消して、次の期限に進む
        store
            .scopes
            .get_mut("")
            .unwrap()
            .get_mut("a")
            .unwrap()
            .expires_at = Utc::now() - chrono::Duration::seconds(1);
        store.earliest_expiry = Some(Utc::now() - chrono::Duration::seconds(1));
        assert_eq!(store.remove_expired_if_due(), 1);
        assert_eq!(store.total, 1);
        assert!(store.earliest_expiry.unwrap() > earliest);
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

    #[tokio::test]
    async fn test_nonces_are_kept_per_caller() {
        // #543: 1つの呼び出し元が自分の枠を埋めても、ほかの呼び出し元は使える
        let manager = NonceManager::with_limits(300, 100, 3);
        let attacker = Some("ip:203.0.113.1".to_string());
        for _ in 0..3 {
            let nonce = NonceManager::generate_nonce();
            manager
                .validate_and_consume(&nonce, attacker.clone())
                .await
                .unwrap();
        }
        let result = manager
            .validate_and_consume(&NonceManager::generate_nonce(), attacker.clone())
            .await;
        assert!(matches!(result, Err(ReplayError::SecurityViolation(_))));

        let other = Some("ip:198.51.100.2".to_string());
        let nonce = NonceManager::generate_nonce();
        manager
            .validate_and_consume(&nonce, other.clone())
            .await
            .unwrap();
        // 同じ呼び出し元が同じnonceを使えば、リプレイ
        assert!(matches!(
            manager.validate_and_consume(&nonce, other).await,
            Err(ReplayError::NonceAlreadyUsed)
        ));
        // 別の呼び出し元のnonceとは、ぶつからない
        manager.validate_and_consume(&nonce, None).await.unwrap();
        assert_eq!(manager.nonce_count().await, 5);
    }
}
