use crate::error::SessionError;
use crate::session::storage::{MemorySessionStorage, SessionStorage};
use crate::session::types::{Session, SessionFilter, SessionId, SessionState};
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;

#[cfg(feature = "redis")]
use crate::session::redis_storage::RedisSessionStorage;

#[derive(Debug)]
pub struct SessionManager {
    storage: Arc<dyn SessionStorage>,
    ttl_hours: i64,
}

impl SessionManager {
    /// Create a new SessionManager with in-memory storage
    pub fn new() -> Self {
        Self {
            storage: Arc::new(MemorySessionStorage::new()),
            ttl_hours: 24,
        }
    }

    /// Create a SessionManager with custom TTL
    pub fn with_ttl(ttl_hours: i64) -> Self {
        Self {
            storage: Arc::new(MemorySessionStorage::new()),
            ttl_hours,
        }
    }

    /// Create a SessionManager with custom storage
    pub fn with_storage(storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            storage,
            ttl_hours: 24,
        }
    }

    /// Create a SessionManager with Redis storage
    #[cfg(feature = "redis")]
    pub async fn new_redis(redis_url: &str, ttl_seconds: u64) -> Result<Self, SessionError> {
        let storage = RedisSessionStorage::new(redis_url, ttl_seconds).await?;
        Ok(Self {
            storage: Arc::new(storage),
            ttl_hours: (ttl_seconds / 3600) as i64,
        })
    }

    pub async fn create_session(&self, user_id: String) -> Result<Session, SessionError> {
        let session = Session {
            id: SessionId::new(),
            state: SessionState::Pending,
            user_id,
            created_at: Utc::now(),
            last_active: Utc::now(),
            expires_at: Utc::now() + Duration::hours(self.ttl_hours),
            metadata: std::collections::HashMap::new(),
        };

        self.storage.create(session).await
    }

    pub async fn get_session(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        let session = self.storage.get(id).await?;

        // Check if session is expired
        if let Some(ref s) = session {
            if s.is_expired() {
                self.storage.delete(id).await?;
                return Ok(None);
            }
        }

        Ok(session)
    }

    pub async fn activate_session(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        if let Some(mut session) = self.storage.get(id).await? {
            if session.is_expired() {
                self.storage.delete(id).await?;
                return Ok(None);
            }

            session.state = SessionState::Active;
            session.last_active = Utc::now();
            let updated = self.storage.update(session).await?;
            Ok(Some(updated))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_session(&self, id: &SessionId) -> Result<bool, SessionError> {
        self.storage.delete(id).await
    }

    pub async fn list_sessions(
        &self,
        filter: &SessionFilter,
    ) -> Result<Vec<Session>, SessionError> {
        let sessions = self.storage.list(filter).await?;

        // Filter out expired sessions
        let active_sessions: Vec<Session> =
            sessions.into_iter().filter(|s| !s.is_expired()).collect();

        Ok(active_sessions)
    }

    /// Delete all sessions for a user
    #[cfg(feature = "redis")]
    pub async fn delete_user_sessions(&self, user_id: &str) -> Result<usize, SessionError> {
        // If using Redis storage, use optimized method
        if let Some(redis_storage) = self.storage.as_any().downcast_ref::<RedisSessionStorage>() {
            return redis_storage.delete_user_sessions(user_id).await;
        }

        // Fallback: list and delete one by one
        let filter = SessionFilter {
            user_id: Some(user_id.to_string()),
            state: None,
        };
        let sessions = self.storage.list(&filter).await?;
        let mut count = 0;

        for session in sessions {
            if self.storage.delete(&session.id).await? {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Update session activity (extends TTL if using Redis)
    pub async fn touch_session(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        if let Some(mut session) = self.storage.get(id).await? {
            if session.is_expired() {
                self.storage.delete(id).await?;
                return Ok(None);
            }

            session.last_active = Utc::now();
            session.expires_at = Utc::now() + Duration::hours(self.ttl_hours);
            let updated = self.storage.update(session).await?;
            Ok(Some(updated))
        } else {
            Ok(None)
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
