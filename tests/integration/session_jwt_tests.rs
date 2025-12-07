//! Integration tests for Session JWT functionality

use mcp_rs::security::auth::types::Role;
use mcp_rs::session::{SessionJwtConfig, SessionJwtManager, SessionManager};
use std::sync::Arc;

#[tokio::test]
async fn test_session_jwt_full_workflow() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig::default();
    let jwt_manager = SessionJwtManager::new(session_manager, config);

    // Create session with tokens
    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user123".to_string(),
            "testuser".to_string(),
            Some("test@example.com".to_string()),
            vec![Role::User, Role::Admin],
        )
        .await
        .unwrap();

    assert!(!session_with_tokens.tokens.access_token.is_empty());
    assert!(!session_with_tokens.tokens.refresh_token.is_empty());
    assert_eq!(session_with_tokens.tokens.token_type, "Bearer");

    // Verify access token
    let (session, claims) = jwt_manager
        .verify_token_and_get_session(&session_with_tokens.tokens.access_token)
        .await
        .unwrap();

    assert_eq!(session.id, session_with_tokens.session.id);
    assert_eq!(claims.username, "testuser");
    assert_eq!(claims.email, Some("test@example.com".to_string()));
    assert!(claims.roles.contains(&"User".to_string()));
    assert!(claims.roles.contains(&"Admin".to_string()));

    // Refresh tokens
    let new_tokens = jwt_manager
        .refresh_tokens(&session_with_tokens.tokens.refresh_token)
        .await
        .unwrap();

    assert_ne!(
        new_tokens.access_token,
        session_with_tokens.tokens.access_token
    );
    assert_ne!(
        new_tokens.refresh_token,
        session_with_tokens.tokens.refresh_token
    );

    // Old refresh token should be revoked (rotation enabled)
    let old_refresh_claims = jwt_manager
        .jwt_auth()
        .verify_token(&session_with_tokens.tokens.refresh_token)
        .unwrap();
    assert!(jwt_manager.is_token_revoked(&old_refresh_claims.jti).await);
}

#[tokio::test]
async fn test_token_revocation() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig::default();
    let jwt_manager = SessionJwtManager::new(session_manager, config);

    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user123".to_string(),
            "testuser".to_string(),
            None,
            vec![Role::User],
        )
        .await
        .unwrap();

    // Get JTI from access token
    let access_claims = jwt_manager
        .jwt_auth()
        .verify_access_token(&session_with_tokens.tokens.access_token)
        .unwrap();

    // Revoke the token
    jwt_manager
        .revoke_token(access_claims.jti.clone(), "Test revocation".to_string())
        .await
        .unwrap();

    // Verify should fail
    let result = jwt_manager
        .verify_token_and_get_session(&session_with_tokens.tokens.access_token)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_logout() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig::default();
    let jwt_manager = SessionJwtManager::new(session_manager.clone(), config);

    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user123".to_string(),
            "testuser".to_string(),
            None,
            vec![Role::User],
        )
        .await
        .unwrap();

    // Get JTIs
    let access_claims = jwt_manager
        .jwt_auth()
        .verify_access_token(&session_with_tokens.tokens.access_token)
        .unwrap();
    let refresh_claims = jwt_manager
        .jwt_auth()
        .verify_refresh_token(&session_with_tokens.tokens.refresh_token)
        .unwrap();

    // Logout
    jwt_manager
        .logout(
            &session_with_tokens.session.id,
            access_claims.jti.clone(),
            refresh_claims.jti.clone(),
        )
        .await
        .unwrap();

    // Session should be deleted
    let session = session_manager
        .get_session(&session_with_tokens.session.id)
        .await
        .unwrap();
    assert!(session.is_none());

    // Tokens should be revoked
    assert!(jwt_manager.is_token_revoked(&access_claims.jti).await);
    assert!(jwt_manager.is_token_revoked(&refresh_claims.jti).await);
}

#[tokio::test]
async fn test_force_logout_user() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig::default();
    let jwt_manager = SessionJwtManager::new(session_manager, config);

    // Create 3 sessions for the same user
    for _ in 0..3 {
        jwt_manager
            .create_session_with_tokens(
                "user123".to_string(),
                "testuser".to_string(),
                None,
                vec![Role::User],
            )
            .await
            .unwrap();
    }

    // Force logout
    let count = jwt_manager.force_logout_user("user123").await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn test_refresh_disabled() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig {
        enable_refresh: false,
        ..Default::default()
    };

    let jwt_manager = SessionJwtManager::new(session_manager, config);

    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user123".to_string(),
            "testuser".to_string(),
            None,
            vec![Role::User],
        )
        .await
        .unwrap();

    // Try to refresh (should fail)
    let result = jwt_manager
        .refresh_tokens(&session_with_tokens.tokens.refresh_token)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_rotation_disabled() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig {
        rotate_on_refresh: false,
        ..Default::default()
    };

    let jwt_manager = SessionJwtManager::new(session_manager, config);

    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user123".to_string(),
            "testuser".to_string(),
            None,
            vec![Role::User],
        )
        .await
        .unwrap();

    // Get old refresh token JTI
    let old_refresh_claims = jwt_manager
        .jwt_auth()
        .verify_refresh_token(&session_with_tokens.tokens.refresh_token)
        .unwrap();

    // Refresh tokens
    jwt_manager
        .refresh_tokens(&session_with_tokens.tokens.refresh_token)
        .await
        .unwrap();

    // Old refresh token should NOT be revoked (rotation disabled)
    assert!(!jwt_manager.is_token_revoked(&old_refresh_claims.jti).await);
}

#[tokio::test]
async fn test_invalid_access_token() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig::default();
    let jwt_manager = SessionJwtManager::new(session_manager, config);

    // Try to verify an invalid token
    let result = jwt_manager
        .verify_token_and_get_session("invalid.token.here")
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_session_not_found() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig::default();
    let jwt_manager = SessionJwtManager::new(session_manager.clone(), config);

    // Create and immediately delete session
    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user123".to_string(),
            "testuser".to_string(),
            None,
            vec![Role::User],
        )
        .await
        .unwrap();

    session_manager
        .delete_session(&session_with_tokens.session.id)
        .await
        .unwrap();

    // Try to verify token (session deleted)
    let result = jwt_manager
        .verify_token_and_get_session(&session_with_tokens.tokens.access_token)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_multiple_roles() {
    let session_manager = Arc::new(SessionManager::new());
    let config = SessionJwtConfig::default();
    let jwt_manager = SessionJwtManager::new(session_manager, config);

    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user123".to_string(),
            "testuser".to_string(),
            None,
            vec![Role::Admin, Role::User, Role::Guest],
        )
        .await
        .unwrap();

    let (_, claims) = jwt_manager
        .verify_token_and_get_session(&session_with_tokens.tokens.access_token)
        .await
        .unwrap();

    assert_eq!(claims.roles.len(), 3);
    assert!(claims.roles.contains(&"Admin".to_string()));
    assert!(claims.roles.contains(&"User".to_string()));
    assert!(claims.roles.contains(&"Guest".to_string()));
}
