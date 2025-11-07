//! 現在のセッション管理システム向けテスト
//! 実際の実装に基づいたテストケース

use chrono::{Duration, Utc};
use mcp_rs::session::{
    manager::SessionManager,
    storage::{MemorySessionStorage, SessionStorage},
    types::{Session, SessionFilter, SessionId, SessionState},
};
use std::sync::Arc;

#[tokio::test]
async fn test_session_creation() {
    let manager = SessionManager::new();

    let session = manager
        .create_session("test_user".to_string())
        .await
        .unwrap();

    assert_eq!(session.user_id, "test_user");
    assert_eq!(session.state, SessionState::Pending);
    assert!(session.created_at <= Utc::now());
    assert!(session.expires_at > Utc::now());
}

#[tokio::test]
async fn test_session_activation() {
    let manager = SessionManager::new();

    let session = manager
        .create_session("test_user".to_string())
        .await
        .unwrap();
    let session_id = session.id.clone();

    let activated = manager
        .activate_session(&session_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(activated.state, SessionState::Active);
    assert_eq!(activated.id, session_id);
}

#[tokio::test]
async fn test_session_retrieval() {
    let manager = SessionManager::new();

    let session = manager
        .create_session("test_user".to_string())
        .await
        .unwrap();
    let session_id = session.id.clone();

    let retrieved = manager.get_session(&session_id).await.unwrap().unwrap();

    assert_eq!(retrieved.id, session_id);
    assert_eq!(retrieved.user_id, "test_user");
}

#[tokio::test]
async fn test_session_listing() {
    let manager = SessionManager::new();

    // 複数のセッションを作成
    let _session1 = manager.create_session("user1".to_string()).await.unwrap();
    let _session2 = manager.create_session("user2".to_string()).await.unwrap();

    let filter = SessionFilter {
        user_id: None,
        state: None,
    };

    let sessions = manager.list_sessions(&filter).await.unwrap();

    assert!(sessions.len() >= 2);
}

#[tokio::test]
async fn test_session_filtering_by_user() {
    let manager = SessionManager::new();

    let _session1 = manager.create_session("user1".to_string()).await.unwrap();
    let _session2 = manager.create_session("user2".to_string()).await.unwrap();
    let _session3 = manager.create_session("user1".to_string()).await.unwrap();

    let filter = SessionFilter {
        user_id: Some("user1".to_string()),
        state: None,
    };

    let sessions = manager.list_sessions(&filter).await.unwrap();

    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.user_id == "user1"));
}

#[tokio::test]
async fn test_session_filtering_by_state() {
    let manager = SessionManager::new();

    let session1 = manager.create_session("user1".to_string()).await.unwrap();
    let _session2 = manager.create_session("user2".to_string()).await.unwrap();

    // 1つをアクティベート
    manager.activate_session(&session1.id).await.unwrap();

    let filter = SessionFilter {
        user_id: None,
        state: Some(SessionState::Active),
    };

    let sessions = manager.list_sessions(&filter).await.unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].state, SessionState::Active);
}

#[tokio::test]
async fn test_memory_storage_basic_operations() {
    let storage = Arc::new(MemorySessionStorage::new());

    let session = Session {
        id: SessionId::new(),
        user_id: "test_user".to_string(),
        state: SessionState::Pending,
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(24),
    };

    let session_id = session.id.clone();

    // 作成
    let created = storage.create(session).await.unwrap();
    assert_eq!(created.id, session_id);

    // 取得
    let retrieved = storage.get(&session_id).await.unwrap().unwrap();
    assert_eq!(retrieved.id, session_id);

    // 更新
    let mut updated = retrieved.clone();
    updated.state = SessionState::Active;
    storage.update(updated).await.unwrap();

    let retrieved_updated = storage.get(&session_id).await.unwrap().unwrap();
    assert_eq!(retrieved_updated.state, SessionState::Active);

    // 削除
    storage.delete(&session_id).await.unwrap();
    let deleted = storage.get(&session_id).await.unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
async fn test_session_id_generation() {
    let id1 = SessionId::new();
    let id2 = SessionId::new();

    // IDは異なるべき
    assert_ne!(id1.as_str(), id2.as_str());

    // UUIDフォーマットであるべき
    assert!(id1.as_str().len() > 30);
    assert!(id2.as_str().len() > 30);
}

#[tokio::test]
async fn test_session_id_from_string() {
    let original_str = "test-session-id-123";
    let session_id = SessionId::from_string(original_str.to_string());

    assert_eq!(session_id.as_str(), original_str);
}

#[tokio::test]
async fn test_concurrent_session_operations() {
    use tokio::task;

    let manager = Arc::new(SessionManager::new());

    // 複数の同時セッション作成
    let mut handles = vec![];

    for i in 0..10 {
        let manager_clone = manager.clone();
        let handle =
            task::spawn(async move { manager_clone.create_session(format!("user_{}", i)).await });
        handles.push(handle);
    }

    // すべてのタスクを待機
    for handle in handles {
        let result = handle.await;
        assert!(result.unwrap().is_ok());
    }

    // 作成されたセッションを確認
    let filter = SessionFilter {
        user_id: None,
        state: None,
    };

    let sessions = manager.list_sessions(&filter).await.unwrap();
    assert_eq!(sessions.len(), 10);
}
