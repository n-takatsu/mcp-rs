//! Session management integration tests
//!
//! This test module verifies session management functionality
//! including in-memory and Redis storage backends.

use mcp_rs::error::SessionError;
use mcp_rs::session::{
    MemorySessionStorage, Session, SessionFilter, SessionId, SessionManager, SessionState,
    SessionStorage,
};
use std::sync::Arc;

#[tokio::test]
async fn test_memory_session_creation() {
    let storage = MemorySessionStorage::new();

    let session = Session {
        id: SessionId::new(),
        user_id: "user123".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        metadata: std::collections::HashMap::new(),
    };

    let created = storage.create(session.clone()).await.unwrap();
    assert_eq!(created.id, session.id);
    assert_eq!(created.user_id, "user123");
}

#[tokio::test]
async fn test_memory_session_retrieval() {
    let storage = MemorySessionStorage::new();

    let session = Session {
        id: SessionId::new(),
        user_id: "user456".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        metadata: std::collections::HashMap::new(),
    };

    storage.create(session.clone()).await.unwrap();

    let retrieved = storage.get(&session.id).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().user_id, "user456");
}

#[tokio::test]
async fn test_memory_session_deletion() {
    let storage = MemorySessionStorage::new();

    let session = Session {
        id: SessionId::new(),
        user_id: "user789".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        metadata: std::collections::HashMap::new(),
    };

    storage.create(session.clone()).await.unwrap();

    let deleted = storage.delete(&session.id).await.unwrap();
    assert!(deleted);

    let retrieved = storage.get(&session.id).await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_memory_session_filtering() {
    let storage = MemorySessionStorage::new();

    // Create sessions for different users
    for i in 1..=3 {
        let session = Session {
            id: SessionId::new(),
            user_id: "user_filter".to_string(),
            state: SessionState::Active,
            created_at: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            metadata: std::collections::HashMap::new(),
        };
        storage.create(session).await.unwrap();
    }

    // Create session for different user
    let other_session = Session {
        id: SessionId::new(),
        user_id: "other_user".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        metadata: std::collections::HashMap::new(),
    };
    storage.create(other_session).await.unwrap();

    // Filter by user
    let filter = SessionFilter {
        user_id: Some("user_filter".to_string()),
        state: None,
    };

    let sessions = storage.list(&filter).await.unwrap();
    assert_eq!(sessions.len(), 3);
    assert!(sessions.iter().all(|s| s.user_id == "user_filter"));
}

#[tokio::test]
async fn test_session_manager_basic_operations() {
    let manager = SessionManager::new();

    // Create session
    let session = manager
        .create_session("user_mgr".to_string())
        .await
        .unwrap();
    assert_eq!(session.user_id, "user_mgr");
    assert_eq!(session.state, SessionState::Pending);

    // Get session
    let retrieved = manager.get_session(&session.id).await.unwrap();
    assert!(retrieved.is_some());

    // Activate session
    let activated = manager.activate_session(&session.id).await.unwrap();
    assert!(activated.is_some());
    assert_eq!(activated.unwrap().state, SessionState::Active);

    // Delete session
    let deleted = manager.delete_session(&session.id).await.unwrap();
    assert!(deleted);

    // Verify deletion
    let after_delete = manager.get_session(&session.id).await.unwrap();
    assert!(after_delete.is_none());
}

#[tokio::test]
async fn test_session_expiration() {
    let session = Session {
        id: SessionId::new(),
        user_id: "user_exp".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() - chrono::Duration::seconds(10), // Already expired
        metadata: std::collections::HashMap::new(),
    };

    assert!(session.is_expired());
}

#[tokio::test]
async fn test_session_not_expired() {
    let session = Session {
        id: SessionId::new(),
        user_id: "user_active".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        metadata: std::collections::HashMap::new(),
    };

    assert!(!session.is_expired());
}

#[tokio::test]
async fn test_session_touch() {
    let manager = SessionManager::with_ttl(1); // 1 hour TTL

    let session = manager
        .create_session("user_touch".to_string())
        .await
        .unwrap();

    // Wait a bit and touch the session
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let touched = manager.touch_session(&session.id).await.unwrap();
    assert!(touched.is_some());

    let touched_session = touched.unwrap();
    assert!(touched_session.last_active > session.created_at);
}

#[tokio::test]
async fn test_session_role_check() {
    let mut session = Session {
        id: SessionId::new(),
        user_id: "user_roles".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        metadata: std::collections::HashMap::new(),
    };

    session
        .metadata
        .insert("roles".to_string(), "admin, moderator, user".to_string());

    assert!(session.has_role("admin"));
    assert!(session.has_role("moderator"));
    assert!(session.has_role("user"));
    assert!(!session.has_role("guest"));
}

#[tokio::test]
async fn test_session_age_and_ttl() {
    let session = Session {
        id: SessionId::new(),
        user_id: "user_age".to_string(),
        state: SessionState::Active,
        created_at: chrono::Utc::now() - chrono::Duration::seconds(30),
        last_active: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        metadata: std::collections::HashMap::new(),
    };

    // Age should be around 30 seconds
    let age = session.age_seconds();
    assert!(age >= 29 && age <= 31);

    // TTL should be around 1 hour (3600 seconds)
    let ttl = session.ttl_seconds();
    assert!(ttl >= 3590 && ttl <= 3610);
}

#[tokio::test]
async fn test_session_id_conversion() {
    let id1 = SessionId::new();
    let id_string = id1.to_string();

    let id2: SessionId = id_string.clone().into();
    assert_eq!(id1, id2);

    let id3: SessionId = id_string.as_str().into();
    assert_eq!(id1, id3);
}

#[tokio::test]
async fn test_multiple_users_sessions() {
    let manager = SessionManager::new();

    // Create multiple sessions for multiple users
    for user_num in 1..=3 {
        for session_num in 1..=2 {
            manager
                .create_session(format!("user{}", user_num))
                .await
                .unwrap();
        }
    }

    // List sessions for user1
    let filter = SessionFilter {
        user_id: Some("user1".to_string()),
        state: None,
    };
    let sessions = manager.list_sessions(&filter).await.unwrap();
    assert_eq!(sessions.len(), 2);
}

// Redis-specific tests (requires Redis server)
#[cfg(feature = "redis")]
mod redis_tests {
    use super::*;
    use mcp_rs::session::RedisSessionStorage;

    #[tokio::test]
    #[ignore] // Run with: cargo test --features redis -- --ignored
    async fn test_redis_session_creation() {
        let storage = RedisSessionStorage::new("redis://127.0.0.1:6379", 3600)
            .await
            .expect("Failed to connect to Redis");

        let session = Session {
            id: SessionId::new(),
            user_id: "redis_user1".to_string(),
            state: SessionState::Active,
            created_at: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            metadata: std::collections::HashMap::new(),
        };

        let created = storage.create(session.clone()).await.unwrap();
        assert_eq!(created.id, session.id);

        // Cleanup
        storage.delete(&session.id).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_redis_manager() {
        let manager = SessionManager::new_redis("redis://127.0.0.1:6379", 3600)
            .await
            .expect("Failed to create Redis session manager");

        let session = manager
            .create_session("redis_mgr_user".to_string())
            .await
            .unwrap();

        let retrieved = manager.get_session(&session.id).await.unwrap();
        assert!(retrieved.is_some());

        // Cleanup
        manager.delete_session(&session.id).await.unwrap();
    }
}
