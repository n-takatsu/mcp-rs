//! 基本的なセッション管理テスト
//! 現在の実装に合わせた最小限のテストケース

use mcp_rs::{SessionFilter, SessionId, SessionManager, SessionState};

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
async fn test_session_creation() -> Result<(), Box<dyn std::error::Error>> {
    // SessionManager作成
    let manager = SessionManager::new();

    // セッション作成
    let session = manager.create_session("test_user".to_string()).await?;

    // セッション内容の検証
    assert_eq!(session.user_id, "test_user");
    assert_eq!(session.state, SessionState::Pending);
    assert!(session.created_at <= session.expires_at);

    Ok(())
}

#[tokio::test]
async fn test_session_get() -> Result<(), Box<dyn std::error::Error>> {
    // SessionManager作成
    let manager = SessionManager::new();

    // セッション作成
    let created_session = manager.create_session("test_user".to_string()).await?;
    let session_id = created_session.id.clone();

    // セッション取得
    let retrieved_session = manager.get_session(&session_id).await?;

    // 取得結果の検証
    assert!(retrieved_session.is_some());
    let session = retrieved_session.unwrap();
    assert_eq!(session.id.as_str(), session_id.as_str());
    assert_eq!(session.user_id, "test_user");

    Ok(())
}

#[tokio::test]
async fn test_session_activation() -> Result<(), Box<dyn std::error::Error>> {
    // SessionManager作成
    let manager = SessionManager::new();

    // セッション作成
    let session = manager.create_session("test_user".to_string()).await?;
    let session_id = session.id.clone();

    // セッションアクティベート
    let activated = manager.activate_session(&session_id).await?;

    // アクティベーション結果の検証
    assert!(activated.is_some());
    let active_session = activated.unwrap();
    assert_eq!(active_session.state, SessionState::Active);

    Ok(())
}

#[tokio::test]
async fn test_session_delete() -> Result<(), Box<dyn std::error::Error>> {
    // SessionManager作成
    let manager = SessionManager::new();

    // セッション作成
    let session = manager.create_session("test_user".to_string()).await?;
    let session_id = session.id.clone();

    // セッション削除
    let deleted = manager.delete_session(&session_id).await?;
    assert!(deleted);

    // 削除後の確認
    let retrieved = manager.get_session(&session_id).await?;
    assert!(retrieved.is_none());

    Ok(())
}

#[tokio::test]
async fn test_session_list() -> Result<(), Box<dyn std::error::Error>> {
    // SessionManager作成
    let manager = SessionManager::new();

    // 複数セッション作成
    let _session1 = manager.create_session("user1".to_string()).await?;
    let _session2 = manager.create_session("user2".to_string()).await?;

    // セッション一覧取得
    let filter = SessionFilter {
        user_id: None,
        state: None,
    };
    let sessions = manager.list_sessions(&filter).await?;

    // 結果検証
    assert_eq!(sessions.len(), 2);

    Ok(())
}
