use crate::error::SessionError;
use crate::session::types::{Session, SessionFilter, SessionId};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait SessionStorage: Send + Sync + std::fmt::Debug {
    async fn create(&self, session: Session) -> Result<Session, SessionError>;
    async fn get(&self, id: &SessionId) -> Result<Option<Session>, SessionError>;
    async fn update(&self, session: Session) -> Result<Session, SessionError>;
    async fn delete(&self, id: &SessionId) -> Result<bool, SessionError>;
    async fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, SessionError>;
}

#[derive(Debug)]
pub struct MemorySessionStorage {
    sessions: Arc<RwLock<HashMap<SessionId, Session>>>,
}

impl MemorySessionStorage {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemorySessionStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionStorage for MemorySessionStorage {
    async fn create(&self, session: Session) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.write().await;
        let session_id = session.id.clone();
        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    async fn get(&self, id: &SessionId) -> Result<Option<Session>, SessionError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(id).cloned())
    }

    async fn update(&self, session: Session) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.write().await;
        let session_id = session.id.clone();
        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    async fn delete(&self, id: &SessionId) -> Result<bool, SessionError> {
        let mut sessions = self.sessions.write().await;
        Ok(sessions.remove(id).is_some())
    }

    async fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, SessionError> {
        let sessions = self.sessions.read().await;
        let mut result: Vec<Session> = sessions.values().cloned().collect();

        // ユーザーIDでフィルター
        if let Some(ref user_id) = filter.user_id {
            result.retain(|session| session.user_id == *user_id);
        }

        // 状態でフィルター
        if let Some(ref state) = filter.state {
            result.retain(|session| session.state == *state);
        }

        Ok(result)
    }
}
