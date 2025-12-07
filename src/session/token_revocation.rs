//! Redis-based Token Revocation List
//!
//! This module provides a production-ready token revocation list using Redis.
//! It supports automatic expiration and efficient lookups.

use crate::error::SessionError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "redis")]
use redis::{AsyncCommands, Client};

/// Token revocation entry for Redis storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRevocationEntry {
    /// Token JTI (JWT ID)
    pub jti: String,

    /// Revocation timestamp
    pub revoked_at: i64,

    /// Reason for revocation
    pub reason: String,

    /// User ID associated with the token
    pub user_id: Option<String>,
}

/// Redis-based token revocation list
#[cfg(feature = "redis")]
pub struct RedisTokenRevocationList {
    client: Client,
    prefix: String,
    default_ttl: Duration,
}

#[cfg(feature = "redis")]
impl RedisTokenRevocationList {
    /// Create a new RedisTokenRevocationList
    pub async fn new(
        redis_url: &str,
        prefix: Option<String>,
        default_ttl: Option<Duration>,
    ) -> Result<Self, SessionError> {
        let client = Client::open(redis_url)
            .map_err(|e| SessionError::Internal(format!("Failed to connect to Redis: {}", e)))?;

        // Test connection
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| SessionError::Internal(format!("Redis connection failed: {}", e)))?;

        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| SessionError::Internal(format!("Redis PING failed: {}", e)))?;

        Ok(Self {
            client,
            prefix: prefix.unwrap_or_else(|| "mcp:revoked:".to_string()),
            default_ttl: default_ttl.unwrap_or(Duration::from_secs(86400 * 7)), // 7 days default
        })
    }

    /// Revoke a token
    pub async fn revoke_token(
        &self,
        jti: String,
        reason: String,
        user_id: Option<String>,
        ttl: Option<Duration>,
    ) -> Result<(), SessionError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| SessionError::Internal(format!("Redis connection failed: {}", e)))?;

        let entry = TokenRevocationEntry {
            jti: jti.clone(),
            revoked_at: chrono::Utc::now().timestamp(),
            reason,
            user_id,
        };

        let key = format!("{}{}", self.prefix, jti);
        let value = serde_json::to_string(&entry)
            .map_err(|e| SessionError::Internal(format!("Serialization failed: {}", e)))?;

        let ttl_seconds = ttl.unwrap_or(self.default_ttl).as_secs();

        conn.set_ex::<_, _, ()>(&key, value, ttl_seconds)
            .await
            .map_err(|e| SessionError::Internal(format!("Redis SET failed: {}", e)))?;

        tracing::info!("Token revoked in Redis: jti={}", jti);

        Ok(())
    }

    /// Check if a token is revoked
    pub async fn is_revoked(&self, jti: &str) -> Result<bool, SessionError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| SessionError::Internal(format!("Redis connection failed: {}", e)))?;

        let key = format!("{}{}", self.prefix, jti);

        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| SessionError::Internal(format!("Redis EXISTS failed: {}", e)))?;

        Ok(exists)
    }

    /// Get revocation details
    pub async fn get_revocation_info(
        &self,
        jti: &str,
    ) -> Result<Option<TokenRevocationEntry>, SessionError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| SessionError::Internal(format!("Redis connection failed: {}", e)))?;

        let key = format!("{}{}", self.prefix, jti);

        let value: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| SessionError::Internal(format!("Redis GET failed: {}", e)))?;

        if let Some(json) = value {
            let entry: TokenRevocationEntry = serde_json::from_str(&json)
                .map_err(|e| SessionError::Internal(format!("Deserialization failed: {}", e)))?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    /// Revoke all tokens for a user
    pub async fn revoke_all_user_tokens(
        &self,
        user_id: &str,
        jtis: Vec<String>,
        reason: String,
    ) -> Result<usize, SessionError> {
        let mut count = 0;
        for jti in jtis {
            self.revoke_token(jti, reason.clone(), Some(user_id.to_string()), None)
                .await?;
            count += 1;
        }

        tracing::info!("Revoked {} tokens for user: {}", count, user_id);
        Ok(count)
    }

    /// Clean up expired entries (Redis handles this automatically with TTL)
    /// This method is for manual cleanup if needed
    pub async fn cleanup_expired(&self) -> Result<(), SessionError> {
        // Redis automatically removes expired keys, so this is a no-op
        // This method exists for API compatibility
        tracing::debug!("Redis TTL-based cleanup (automatic)");
        Ok(())
    }
}

/// In-memory token revocation list (for development/testing)
pub struct InMemoryTokenRevocationList {
    tokens: tokio::sync::RwLock<std::collections::HashMap<String, TokenRevocationEntry>>,
}

impl InMemoryTokenRevocationList {
    pub fn new() -> Self {
        Self {
            tokens: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub async fn revoke_token(
        &self,
        jti: String,
        reason: String,
        user_id: Option<String>,
    ) -> Result<(), SessionError> {
        let mut tokens = self.tokens.write().await;

        let entry = TokenRevocationEntry {
            jti: jti.clone(),
            revoked_at: chrono::Utc::now().timestamp(),
            reason,
            user_id,
        };

        tokens.insert(jti.clone(), entry);
        tracing::info!("Token revoked in memory: jti={}", jti);

        Ok(())
    }

    pub async fn is_revoked(&self, jti: &str) -> bool {
        let tokens = self.tokens.read().await;
        tokens.contains_key(jti)
    }

    pub async fn get_revocation_info(&self, jti: &str) -> Option<TokenRevocationEntry> {
        let tokens = self.tokens.read().await;
        tokens.get(jti).cloned()
    }

    pub async fn cleanup_expired(&self, max_age_seconds: i64) -> usize {
        let mut tokens = self.tokens.write().await;
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - max_age_seconds;

        let initial_count = tokens.len();
        tokens.retain(|_, entry| entry.revoked_at >= cutoff);
        let removed = initial_count - tokens.len();

        if removed > 0 {
            tracing::info!("Cleaned up {} expired token revocations", removed);
        }

        removed
    }
}

impl Default for InMemoryTokenRevocationList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_revocation() {
        let revocation_list = InMemoryTokenRevocationList::new();

        let jti = "test-jti-123".to_string();

        // Initially not revoked
        assert!(!revocation_list.is_revoked(&jti).await);

        // Revoke token
        revocation_list
            .revoke_token(
                jti.clone(),
                "Test reason".to_string(),
                Some("user123".to_string()),
            )
            .await
            .unwrap();

        // Now should be revoked
        assert!(revocation_list.is_revoked(&jti).await);

        // Check revocation info
        let info = revocation_list.get_revocation_info(&jti).await;
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.reason, "Test reason");
        assert_eq!(info.user_id, Some("user123".to_string()));
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let revocation_list = InMemoryTokenRevocationList::new();

        // Add a token with an old timestamp manually
        {
            let mut tokens = revocation_list.tokens.write().await;
            let entry = TokenRevocationEntry {
                jti: "jti1".to_string(),
                revoked_at: chrono::Utc::now().timestamp() - 200, // 200 seconds ago
                reason: "Reason".to_string(),
                user_id: None,
            };
            tokens.insert("jti1".to_string(), entry);
        }

        // Cleanup with very long max age (nothing should be removed)
        let removed = revocation_list.cleanup_expired(86400).await;
        assert_eq!(removed, 0);

        // Cleanup with 100 second max age (old token should be removed)
        let removed = revocation_list.cleanup_expired(100).await;
        assert_eq!(removed, 1);
    }
}
