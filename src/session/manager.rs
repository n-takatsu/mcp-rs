use crate::error::SessionError;
use crate::session::storage::{MemorySessionStorage, SessionStorage};
use crate::session::types::{Session, SessionFilter, SessionId, SessionState};
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;

#[derive(Debug)]
pub struct SessionManager {
    storage: Arc<dyn SessionStorage>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(MemorySessionStorage::new()),
        }
    }

    pub async fn create_session(&self, user_id: String) -> Result<Session, SessionError> {
        let session = Session {
            id: SessionId::new(),
            state: SessionState::Pending,
            user_id,
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(24),
        };

        self.storage.create(session).await
    }

    pub async fn get_session(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        self.storage.get(id).await
    }

    pub async fn activate_session(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        if let Some(mut session) = self.storage.get(id).await? {
            session.state = SessionState::Active;
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
        self.storage.list(filter).await
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
