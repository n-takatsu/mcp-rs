//! Session Management Integration Tests
//!
//! セッション管理システムの統合テスト

use chrono::{Duration, Utc};
use mcp_rs::session::{
    manager::SessionManager,
    types::{SessionFilter, SessionId, SessionState},
};
use std::sync::Arc;

#[tokio::test]
async fn test_session_id_creation() -> Result<(), Box<dyn std::error::Error>> {
    // SessionId作成テスト
    let id1 = SessionId::new();
    let id2 = SessionId::new();

    // 異なるIDが生成されることを確認
    assert_ne!(id1.as_str(), id2.as_str());

    // UUID形式の検証
    assert_eq!(id1.as_str().len(), 36);
    assert!(id1.as_str().contains('-'));

    Ok(())
}

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

    // セッション作成
    let session = manager
        .create_session("test_user".to_string())
        .await
        .unwrap();

    // セッション有効化
    let activated_session = manager.activate_session(&session.id).await.unwrap();

    assert!(activated_session.is_some());
    let session_data = activated_session.unwrap();
    assert_eq!(session_data.state, SessionState::Active);
    assert_eq!(session_data.id, session.id);
    assert_eq!(session_data.user_id, "test_user");
}

#[tokio::test]
async fn test_session_retrieval() {
    let manager = SessionManager::new();

    // セッション作成
    let created_session = manager
        .create_session("test_user".to_string())
        .await
        .unwrap();

    // セッション取得
    let retrieved_session = manager.get_session(&created_session.id).await.unwrap();

    assert!(retrieved_session.is_some());
    let session = retrieved_session.unwrap();
    assert_eq!(session.id, created_session.id);
    assert_eq!(session.user_id, "test_user");
}

#[tokio::test]
async fn test_session_listing() {
    let manager = SessionManager::new();

    // 複数セッション作成
    let session1 = manager.create_session("user1".to_string()).await.unwrap();
    let session2 = manager.create_session("user2".to_string()).await.unwrap();

    // セッション一覧取得
    let empty_filter = SessionFilter {
        user_id: None,
        state: None,
    };
    let sessions = manager.list_sessions(&empty_filter).await.unwrap();

    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().any(|s| s.id == session1.id));
    assert!(sessions.iter().any(|s| s.id == session2.id));
}

#[tokio::test]
async fn test_session_filtering_by_user() {
    let manager = SessionManager::new();

    // 異なるユーザーのセッション作成
    let _session1 = manager.create_session("user1".to_string()).await.unwrap();
    let session2 = manager.create_session("user2".to_string()).await.unwrap();

    // user2でフィルタリング
    let filter = SessionFilter {
        user_id: Some("user2".to_string()),
        state: None,
    };

    let filtered_sessions = manager.list_sessions(&filter).await.unwrap();

    assert_eq!(filtered_sessions.len(), 1);
    assert_eq!(filtered_sessions[0].id, session2.id);
    assert_eq!(filtered_sessions[0].user_id, "user2");
}

#[tokio::test]
async fn test_session_expiration() {
    let manager = SessionManager::new();

    // セッション作成
    let session = manager
        .create_session("test_user".to_string())
        .await
        .unwrap();

    // 期限切れのテスト（実際の期限切れ処理のテスト）
    assert!(session.expires_at > Utc::now());

    // セッションが将来の期限を持っていることを確認
    let future_time = Utc::now() + Duration::hours(23);
    assert!(session.expires_at > future_time);
}

#[tokio::test]
async fn test_session_state_transitions() {
    let manager = SessionManager::new();

    // セッション作成（Pending状態）
    let session = manager
        .create_session("test_user".to_string())
        .await
        .unwrap();
    assert_eq!(session.state, SessionState::Pending);

    // 有効化（Active状態）
    let activated_session = manager.activate_session(&session.id).await.unwrap();
    assert!(activated_session.is_some());
    assert_eq!(activated_session.unwrap().state, SessionState::Active);
}

#[tokio::test]
async fn test_concurrent_session_operations() {
    let manager = Arc::new(SessionManager::new());
    let mut handles = vec![];

    // 複数の並行セッション作成
    for i in 0..10 {
        let manager_clone = manager.clone();
        let handle =
            tokio::spawn(async move { manager_clone.create_session(format!("user_{}", i)).await });
        handles.push(handle);
    }

    // 全ての操作が完了することを確認
    let mut successful_sessions = 0;
    for handle in handles {
        if handle.await.unwrap().is_ok() {
            successful_sessions += 1;
        }
    }

    assert_eq!(successful_sessions, 10);
}
