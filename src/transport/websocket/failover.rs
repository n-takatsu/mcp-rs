//! Failover management for WebSocket connections
//!
//! Provides automatic reconnection, session restoration, and error detection
//! for high-availability WebSocket connections.

use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::balancer::Endpoint;

/// Failover configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Initial retry delay
    pub initial_retry_delay: Duration,
    /// Maximum retry delay
    pub max_retry_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Enable session persistence
    pub session_persistence: bool,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay: Duration::from_secs(1),
            max_retry_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            connection_timeout: Duration::from_secs(30),
            session_persistence: true,
        }
    }
}

/// Connection state for session restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Session ID
    pub session_id: String,
    /// Last activity timestamp
    pub last_activity: i64,
    /// Pending messages
    pub pending_messages: Vec<String>,
    /// Connection metadata
    pub metadata: HashMap<String, String>,
}

impl SessionState {
    /// Creates a new session state
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            last_activity: chrono::Utc::now().timestamp(),
            pending_messages: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Adds a pending message
    pub fn add_pending_message(&mut self, message: String) {
        self.pending_messages.push(message);
        self.last_activity = chrono::Utc::now().timestamp();
    }

    /// Clears pending messages
    pub fn clear_pending_messages(&mut self) {
        self.pending_messages.clear();
    }
}

/// Failover status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailoverStatus {
    /// No failover active
    Normal,
    /// Failover in progress
    InProgress,
    /// Failover completed successfully
    Completed,
    /// Failover failed
    Failed,
}

/// Failover event
#[derive(Debug, Clone)]
pub struct FailoverEvent {
    /// Event timestamp
    pub timestamp: Instant,
    /// Source endpoint
    pub from_endpoint: Endpoint,
    /// Target endpoint
    pub to_endpoint: Endpoint,
    /// Failover status
    pub status: FailoverStatus,
    /// Error message (if failed)
    pub error_message: Option<String>,
}

/// Failover trait
#[async_trait]
pub trait Failover: Send + Sync {
    /// Registers a backup endpoint for failover
    async fn register_backup(&mut self, primary: Endpoint, backup: Endpoint) -> Result<()>;

    /// Triggers failover from one endpoint to another
    async fn trigger_failover(&self, endpoint: &Endpoint) -> Result<Endpoint>;

    /// Checks if failover is active for an endpoint
    fn is_failover_active(&self, endpoint: &Endpoint) -> bool;

    /// Gets failover history
    fn get_failover_history(&self, limit: usize) -> Vec<FailoverEvent>;

    /// Restores session state after failover
    async fn restore_session(&self, session_id: &str) -> Result<SessionState>;
}

/// Endpoint failover mapping
#[derive(Debug, Clone)]
struct FailoverMapping {
    primary: Endpoint,
    backups: Vec<Endpoint>,
    active_backup_index: usize,
    retry_count: u32,
}

/// Failover manager implementation
pub struct FailoverManager {
    config: FailoverConfig,
    mappings: Arc<RwLock<HashMap<String, FailoverMapping>>>,
    active_failovers: Arc<RwLock<HashMap<String, FailoverStatus>>>,
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    history: Arc<RwLock<Vec<FailoverEvent>>>,
}

impl FailoverManager {
    /// Creates a new failover manager
    pub fn new(config: FailoverConfig) -> Self {
        Self {
            config,
            mappings: Arc::new(RwLock::new(HashMap::new())),
            active_failovers: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Calculates retry delay with exponential backoff
    fn calculate_retry_delay(&self, attempt: u32) -> Duration {
        let delay_secs = self.config.initial_retry_delay.as_secs_f64()
            * self.config.backoff_multiplier.powi(attempt as i32);

        let capped_delay = delay_secs.min(self.config.max_retry_delay.as_secs_f64());
        Duration::from_secs_f64(capped_delay)
    }

    /// Selects next backup endpoint
    async fn select_next_backup(&self, primary_id: &str) -> Option<Endpoint> {
        let mut mappings = self.mappings.write().await;
        if let Some(mapping) = mappings.get_mut(primary_id) {
            if mapping.backups.is_empty() {
                return None;
            }

            let backup = mapping.backups[mapping.active_backup_index].clone();
            mapping.active_backup_index = (mapping.active_backup_index + 1) % mapping.backups.len();
            mapping.retry_count += 1;

            Some(backup)
        } else {
            None
        }
    }

    /// Records failover event
    async fn record_failover_event(&self, event: FailoverEvent) {
        let mut history = self.history.write().await;
        history.push(event);

        // Keep only last 1000 events
        if history.len() > 1000 {
            history.remove(0);
        }
    }

    /// Saves session state
    pub async fn save_session(&self, session: SessionState) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.session_id.clone(), session);
    }

    /// Removes session
    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    /// Gets all backup endpoints for a primary
    pub async fn get_backups(&self, primary_id: &str) -> Vec<Endpoint> {
        let mappings = self.mappings.read().await;
        mappings
            .get(primary_id)
            .map(|m| m.backups.clone())
            .unwrap_or_default()
    }

    /// Resets retry count for endpoint
    pub async fn reset_retry_count(&self, endpoint_id: &str) {
        let mut mappings = self.mappings.write().await;
        if let Some(mapping) = mappings.get_mut(endpoint_id) {
            mapping.retry_count = 0;
        }
    }

    /// Gets current retry count
    pub async fn get_retry_count(&self, endpoint_id: &str) -> u32 {
        let mappings = self.mappings.read().await;
        mappings
            .get(endpoint_id)
            .map(|m| m.retry_count)
            .unwrap_or(0)
    }
}

#[async_trait]
impl Failover for FailoverManager {
    async fn register_backup(&mut self, primary: Endpoint, backup: Endpoint) -> Result<()> {
        let mut mappings = self.mappings.write().await;

        mappings
            .entry(primary.id.clone())
            .or_insert_with(|| FailoverMapping {
                primary: primary.clone(),
                backups: Vec::new(),
                active_backup_index: 0,
                retry_count: 0,
            })
            .backups
            .push(backup);

        Ok(())
    }

    async fn trigger_failover(&self, endpoint: &Endpoint) -> Result<Endpoint> {
        // Mark failover as in progress
        {
            let mut active = self.active_failovers.write().await;
            active.insert(endpoint.id.clone(), FailoverStatus::InProgress);
        }

        // Try to find backup endpoint
        let backup = self.select_next_backup(&endpoint.id).await.ok_or_else(|| {
            Error::Internal(format!("No backup endpoint available for {}", endpoint.id))
        })?;

        // Record failover event
        let event = FailoverEvent {
            timestamp: Instant::now(),
            from_endpoint: endpoint.clone(),
            to_endpoint: backup.clone(),
            status: FailoverStatus::Completed,
            error_message: None,
        };
        self.record_failover_event(event).await;

        // Update status
        {
            let mut active = self.active_failovers.write().await;
            active.insert(endpoint.id.clone(), FailoverStatus::Completed);
        }

        Ok(backup)
    }

    fn is_failover_active(&self, endpoint: &Endpoint) -> bool {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let active = self.active_failovers.read().await;
                matches!(
                    active.get(&endpoint.id),
                    Some(FailoverStatus::InProgress) | Some(FailoverStatus::Completed)
                )
            })
        })
    }

    fn get_failover_history(&self, limit: usize) -> Vec<FailoverEvent> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let history = self.history.read().await;
                history.iter().rev().take(limit).cloned().collect()
            })
        })
    }

    async fn restore_session(&self, session_id: &str) -> Result<SessionState> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| Error::Internal(format!("Session not found: {}", session_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_failover_config_default() {
        let config = FailoverConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[tokio::test]
    async fn test_session_state() {
        let mut session = SessionState::new("session_123".to_string());
        session.add_pending_message("message1".to_string());
        session.add_pending_message("message2".to_string());

        assert_eq!(session.pending_messages.len(), 2);

        session.clear_pending_messages();
        assert_eq!(session.pending_messages.len(), 0);
    }

    #[tokio::test]
    async fn test_failover_manager_register_backup() {
        let config = FailoverConfig::default();
        let mut manager = FailoverManager::new(config);

        let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
        let backup = Endpoint::new("backup".to_string(), "ws://backup".to_string());

        manager
            .register_backup(primary.clone(), backup.clone())
            .await
            .unwrap();

        let backups = manager.get_backups(&primary.id).await;
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].id, "backup");
    }

    #[tokio::test]
    async fn test_failover_manager_trigger_failover() {
        let config = FailoverConfig::default();
        let mut manager = FailoverManager::new(config);

        let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
        let backup = Endpoint::new("backup".to_string(), "ws://backup".to_string());

        manager
            .register_backup(primary.clone(), backup.clone())
            .await
            .unwrap();

        let failover_endpoint = manager.trigger_failover(&primary).await.unwrap();
        assert_eq!(failover_endpoint.id, "backup");

        assert!(manager.is_failover_active(&primary));
    }

    #[tokio::test]
    async fn test_failover_manager_session_persistence() {
        let config = FailoverConfig::default();
        let manager = FailoverManager::new(config);

        let mut session = SessionState::new("session_456".to_string());
        session.add_pending_message("test_message".to_string());

        manager.save_session(session.clone()).await;

        let restored = manager.restore_session("session_456").await.unwrap();
        assert_eq!(restored.session_id, "session_456");
        assert_eq!(restored.pending_messages.len(), 1);
    }

    #[tokio::test]
    async fn test_failover_retry_count() {
        let config = FailoverConfig::default();
        let manager = FailoverManager::new(config);

        let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
        let backup = Endpoint::new("backup".to_string(), "ws://backup".to_string());

        let mut manager_mut = manager;
        manager_mut
            .register_backup(primary.clone(), backup)
            .await
            .unwrap();

        assert_eq!(manager_mut.get_retry_count(&primary.id).await, 0);

        manager_mut.trigger_failover(&primary).await.unwrap();
        assert_eq!(manager_mut.get_retry_count(&primary.id).await, 1);
    }

    #[tokio::test]
    async fn test_failover_history() {
        let config = FailoverConfig::default();
        let mut manager = FailoverManager::new(config);

        let primary = Endpoint::new("primary".to_string(), "ws://primary".to_string());
        let backup = Endpoint::new("backup".to_string(), "ws://backup".to_string());

        manager
            .register_backup(primary.clone(), backup)
            .await
            .unwrap();
        manager.trigger_failover(&primary).await.unwrap();

        let history = manager.get_failover_history(10);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].from_endpoint.id, "primary");
        assert_eq!(history[0].to_endpoint.id, "backup");
    }
}
