use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(id: String) -> Self {
        Self(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionState {
    Pending,
    Active,
    Suspended,
    Expired,
    Invalidated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub state: SessionState,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

impl Session {
    /// Check if session has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    /// Check if user has a specific role (from metadata)
    pub fn has_role(&self, role: &str) -> bool {
        self.metadata
            .get("roles")
            .map(|roles| roles.split(',').any(|r| r.trim() == role))
            .unwrap_or(false)
    }

    /// Get session age in seconds
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.created_at).num_seconds()
    }

    /// Get time until expiration in seconds
    pub fn ttl_seconds(&self) -> i64 {
        (self.expires_at - Utc::now()).num_seconds()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFilter {
    pub user_id: Option<String>,
    pub state: Option<SessionState>,
}
