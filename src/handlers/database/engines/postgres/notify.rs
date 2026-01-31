//! PostgreSQL LISTEN/NOTIFY Support
//!
//! Provides pub/sub functionality using PostgreSQL's LISTEN/NOTIFY

use crate::handlers::database::types::DatabaseError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;

/// Notification received from PostgreSQL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub channel: String,
    pub payload: String,
    pub received_at: DateTime<Utc>,
}

/// Simple pub/sub manager using pool connections
pub struct PubSubManager {
    pool: PgPool,
}

impl PubSubManager {
    /// Create a new pub/sub manager from a connection pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Send notification to a channel
    pub async fn notify(&self, channel: &str, payload: &str) -> Result<(), DatabaseError> {
        let sql = format!("NOTIFY {}, '{}'", channel, payload);

        sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("NOTIFY failed: {}", e)))?;

        Ok(())
    }

    /// Send JSON notification
    pub async fn notify_json(
        &self,
        channel: &str,
        payload: &JsonValue,
    ) -> Result<(), DatabaseError> {
        let payload_str = serde_json::to_string(payload).map_err(|e| {
            DatabaseError::SerializationFailed(format!("JSON serialization failed: {}", e))
        })?;

        self.notify(channel, &payload_str).await
    }
}

/// Simple listener for one-off LISTEN operations
/// Note: Full LISTEN/NOTIFY requires separate dedicated connection using PgListener
pub struct SimpleListener;

/// Notification queue for buffering messages
pub struct NotificationQueue;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let notif = Notification {
            channel: "test".to_string(),
            payload: "hello".to_string(),
            received_at: Utc::now(),
        };

        assert_eq!(notif.channel, "test");
        assert_eq!(notif.payload, "hello");
    }
}
