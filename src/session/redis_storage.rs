//! Redis-based session storage implementation

use crate::error::SessionError;
use crate::session::types::{Session, SessionFilter, SessionId};
use crate::session::SessionStorage;
use async_trait::async_trait;
use redis::{aio::MultiplexedConnection, AsyncCommands, Client};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

/// Redis session storage implementation
#[derive(Debug, Clone)]
pub struct RedisSessionStorage {
    client: Client,
    connection: Arc<RwLock<Option<MultiplexedConnection>>>,
    key_prefix: String,
    ttl_seconds: u64,
}

impl RedisSessionStorage {
    /// Create a new Redis session storage
    pub async fn new(redis_url: &str, ttl_seconds: u64) -> Result<Self, SessionError> {
        let client = Client::open(redis_url)
            .map_err(|e| SessionError::Storage(format!("Failed to create Redis client: {}", e)))?;

        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| SessionError::Storage(format!("Failed to connect to Redis: {}", e)))?;

        Ok(Self {
            client,
            connection: Arc::new(RwLock::new(Some(connection))),
            key_prefix: "mcp:session:".to_string(),
            ttl_seconds,
        })
    }

    /// Get Redis connection, reconnecting if necessary
    async fn get_connection(&self) -> Result<MultiplexedConnection, SessionError> {
        let mut conn_lock = self.connection.write().await;

        // Try to use existing connection
        if let Some(ref mut conn) = *conn_lock {
            // Test connection with PING
            if redis::cmd("PING").query_async::<String>(conn).await.is_ok() {
                return Ok(conn.clone());
            }
            warn!("Redis connection lost, attempting to reconnect");
        }

        // Reconnect
        let new_conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| SessionError::Storage(format!("Failed to reconnect to Redis: {}", e)))?;

        *conn_lock = Some(new_conn.clone());
        debug!("Successfully reconnected to Redis");

        Ok(new_conn)
    }

    /// Generate Redis key for session
    fn session_key(&self, id: &SessionId) -> String {
        format!("{}{}", self.key_prefix, id)
    }

    /// Generate Redis key for user sessions index
    fn user_sessions_key(&self, user_id: &str) -> String {
        format!("{}user:{}:sessions", self.key_prefix, user_id)
    }
}

#[async_trait]
impl SessionStorage for RedisSessionStorage {
    async fn create(&self, session: Session) -> Result<Session, SessionError> {
        let mut conn = self.get_connection().await?;
        let key = self.session_key(&session.id);

        // Serialize session
        let serialized = serde_json::to_string(&session)
            .map_err(|e| SessionError::Internal(format!("Failed to serialize session: {}", e)))?;

        // Store session with TTL
        conn.set_ex::<_, _, ()>(&key, serialized, self.ttl_seconds)
            .await
            .map_err(|e| {
                SessionError::Storage(format!("Failed to store session in Redis: {}", e))
            })?;

        // Add to user sessions index
        let user_key = self.user_sessions_key(&session.user_id);
        conn.sadd::<_, _, ()>(&user_key, session.id.to_string())
            .await
            .map_err(|e| {
                SessionError::Storage(format!("Failed to update user sessions index: {}", e))
            })?;

        // Set TTL on user sessions index
        conn.expire::<_, ()>(&user_key, self.ttl_seconds as i64)
            .await
            .map_err(|e| {
                SessionError::Storage(format!("Failed to set TTL on user index: {}", e))
            })?;

        debug!(
            "Created session {} for user {}",
            session.id, session.user_id
        );
        Ok(session)
    }

    async fn get(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        let mut conn = self.get_connection().await?;
        let key = self.session_key(id);

        let serialized: Option<String> = conn.get(&key).await.map_err(|e| {
            SessionError::Storage(format!("Failed to retrieve session from Redis: {}", e))
        })?;

        match serialized {
            Some(data) => {
                let session: Session = serde_json::from_str(&data).map_err(|e| {
                    SessionError::Internal(format!("Failed to deserialize session: {}", e))
                })?;
                debug!("Retrieved session {}", id);
                Ok(Some(session))
            }
            None => {
                debug!("Session {} not found", id);
                Ok(None)
            }
        }
    }

    async fn update(&self, session: Session) -> Result<Session, SessionError> {
        let mut conn = self.get_connection().await?;
        let key = self.session_key(&session.id);

        // Serialize session
        let serialized = serde_json::to_string(&session)
            .map_err(|e| SessionError::Internal(format!("Failed to serialize session: {}", e)))?;

        // Update session with TTL
        conn.set_ex::<_, _, ()>(&key, serialized, self.ttl_seconds)
            .await
            .map_err(|e| {
                SessionError::Storage(format!("Failed to update session in Redis: {}", e))
            })?;

        debug!("Updated session {}", session.id);
        Ok(session)
    }

    async fn delete(&self, id: &SessionId) -> Result<bool, SessionError> {
        let mut conn = self.get_connection().await?;

        // First get the session to find user_id
        let session = self.get(id).await?;

        // Delete session key
        let key = self.session_key(id);
        let deleted: i32 = conn.del(&key).await.map_err(|e| {
            SessionError::Storage(format!("Failed to delete session from Redis: {}", e))
        })?;

        // Remove from user sessions index if session existed
        if let Some(session) = session {
            let user_key = self.user_sessions_key(&session.user_id);
            conn.srem::<_, _, ()>(&user_key, id.to_string())
                .await
                .map_err(|e| {
                    SessionError::Storage(format!(
                        "Failed to remove from user sessions index: {}",
                        e
                    ))
                })?;
            debug!("Deleted session {} for user {}", id, session.user_id);
        }

        Ok(deleted > 0)
    }

    async fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, SessionError> {
        let mut conn = self.get_connection().await?;

        // If filtering by user, use user sessions index
        if let Some(ref user_id) = filter.user_id {
            let user_key = self.user_sessions_key(user_id);
            let session_ids: Vec<String> = conn.smembers(&user_key).await.map_err(|e| {
                SessionError::Storage(format!("Failed to get user sessions from Redis: {}", e))
            })?;

            let mut sessions = Vec::new();
            for session_id in session_ids {
                if let Ok(Some(session)) = self.get(&session_id.into()).await {
                    // Apply state filter if specified
                    if let Some(ref state) = filter.state {
                        if session.state == *state {
                            sessions.push(session);
                        }
                    } else {
                        sessions.push(session);
                    }
                }
            }

            debug!("Listed {} sessions for user {}", sessions.len(), user_id);
            Ok(sessions)
        } else {
            // Without user filter, we need to scan all keys (less efficient)
            let pattern = format!("{}*", self.key_prefix);
            let mut sessions = Vec::new();

            // Use SCAN to iterate through keys
            let mut cursor = 0;
            loop {
                let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg(&pattern)
                    .arg("COUNT")
                    .arg(100)
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| {
                        SessionError::Storage(format!("Failed to scan Redis keys: {}", e))
                    })?;

                for key in keys {
                    // Skip user index keys
                    if key.contains(":user:") {
                        continue;
                    }

                    let serialized: Option<String> = conn.get(&key).await.map_err(|e| {
                        SessionError::Storage(format!(
                            "Failed to retrieve session from Redis: {}",
                            e
                        ))
                    })?;

                    if let Some(data) = serialized {
                        if let Ok(session) = serde_json::from_str::<Session>(&data) {
                            // Apply state filter if specified
                            if let Some(ref state) = filter.state {
                                if session.state == *state {
                                    sessions.push(session);
                                }
                            } else {
                                sessions.push(session);
                            }
                        }
                    }
                }

                cursor = new_cursor;
                if cursor == 0 {
                    break;
                }
            }

            debug!("Listed {} sessions total", sessions.len());
            Ok(sessions)
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Helper function to delete all user sessions from Redis
impl RedisSessionStorage {
    /// Delete all sessions for a user
    pub async fn delete_user_sessions(&self, user_id: &str) -> Result<usize, SessionError> {
        let mut conn = self.get_connection().await?;
        let user_key = self.user_sessions_key(user_id);

        let session_ids: Vec<String> = conn.smembers(&user_key).await.map_err(|e| {
            SessionError::Storage(format!("Failed to get user sessions from Redis: {}", e))
        })?;

        let mut deleted_count = 0;
        for session_id in &session_ids {
            if self.delete(&session_id.clone().into()).await? {
                deleted_count += 1;
            }
        }

        // Delete user sessions index
        conn.del::<_, ()>(&user_key).await.map_err(|e| {
            SessionError::Storage(format!("Failed to delete user sessions index: {}", e))
        })?;

        debug!("Deleted {} sessions for user {}", deleted_count, user_id);
        Ok(deleted_count)
    }

    /// Count total sessions in Redis
    pub async fn count_sessions(&self) -> Result<usize, SessionError> {
        let mut conn = self.get_connection().await?;
        let pattern = format!("{}*", self.key_prefix);

        let mut count = 0;
        let mut cursor = 0;
        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
                .map_err(|e| SessionError::Storage(format!("Failed to scan Redis keys: {}", e)))?;

            // Filter out user index keys
            count += keys.iter().filter(|k| !k.contains(":user:")).count();

            cursor = new_cursor;
            if cursor == 0 {
                break;
            }
        }

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionState;

    // Note: These tests require a running Redis instance
    // Run: docker run -d -p 6379:6379 redis:latest

    #[tokio::test]
    #[ignore] // Ignore by default, run with: cargo test -- --ignored
    async fn test_redis_create_and_get() {
        let storage = RedisSessionStorage::new("redis://127.0.0.1:6379", 3600)
            .await
            .expect("Failed to connect to Redis");

        let session = Session {
            id: "test-session-1".into(),
            user_id: "user123".to_string(),
            state: SessionState::Active,
            created_at: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            metadata: std::collections::HashMap::new(),
        };

        // Create session
        let created = storage.create(session.clone()).await.unwrap();
        assert_eq!(created.id, session.id);

        // Get session
        let retrieved = storage.get(&session.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().user_id, "user123");

        // Cleanup
        storage.delete(&session.id).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_delete_user_sessions() {
        let storage = RedisSessionStorage::new("redis://127.0.0.1:6379", 3600)
            .await
            .expect("Failed to connect to Redis");

        // Create multiple sessions for same user
        for i in 1..=3 {
            let session = Session {
                id: format!("test-session-{}", i).into(),
                user_id: "user456".to_string(),
                state: SessionState::Active,
                created_at: chrono::Utc::now(),
                last_active: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                metadata: std::collections::HashMap::new(),
            };
            storage.create(session).await.unwrap();
        }

        // Delete all user sessions
        let deleted = storage.delete_user_sessions("user456").await.unwrap();
        assert_eq!(deleted, 3);

        // Verify sessions are deleted
        let sessions = storage
            .list(&SessionFilter {
                user_id: Some("user456".to_string()),
                state: None,
            })
            .await
            .unwrap();
        assert_eq!(sessions.len(), 0);
    }
}
