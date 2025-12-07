//! JWT Integration for Session Management
//!
//! This module provides JWT token generation and validation integrated with session management.
//! It bridges the gap between the JWT authentication system and session storage.

use crate::error::SessionError;
use crate::security::auth::jwt::{JwtAuth, JwtClaims, JwtConfig, JwtTokenPair};
use crate::security::auth::types::{AuthUser, Role};
use crate::session::types::{Session, SessionId};
use crate::session::SessionManager;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Session JWT configuration
#[derive(Debug, Clone)]
pub struct SessionJwtConfig {
    /// JWT authentication configuration
    pub jwt_config: JwtConfig,

    /// Enable token refresh
    pub enable_refresh: bool,

    /// Token rotation on refresh
    pub rotate_on_refresh: bool,
}

impl Default for SessionJwtConfig {
    fn default() -> Self {
        Self {
            jwt_config: JwtConfig::default(),
            enable_refresh: true,
            rotate_on_refresh: true,
        }
    }
}

/// Token revocation entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokedToken {
    /// Token JTI (JWT ID)
    pub jti: String,

    /// Revocation timestamp
    pub revoked_at: chrono::DateTime<chrono::Utc>,

    /// Reason for revocation
    pub reason: String,
}

/// JWT-integrated Session Manager
pub struct SessionJwtManager {
    /// Session manager
    session_manager: Arc<SessionManager>,

    /// JWT authentication
    jwt_auth: Arc<JwtAuth>,

    /// Configuration
    config: SessionJwtConfig,

    /// Token revocation list (in-memory, should be Redis in production)
    revoked_tokens: Arc<RwLock<HashSet<String>>>,
}

impl SessionJwtManager {
    /// Create a new SessionJwtManager
    pub fn new(session_manager: Arc<SessionManager>, config: SessionJwtConfig) -> Self {
        let jwt_auth = Arc::new(JwtAuth::new(config.jwt_config.clone()));

        Self {
            session_manager,
            jwt_auth,
            config,
            revoked_tokens: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Create a session and generate JWT tokens
    pub async fn create_session_with_tokens(
        &self,
        user_id: String,
        username: String,
        email: Option<String>,
        roles: Vec<Role>,
    ) -> Result<SessionWithTokens, SessionError> {
        // Create session
        let mut session = self.session_manager.create_session(user_id.clone()).await?;

        // Store username in session metadata for later retrieval
        session
            .metadata
            .insert("username".to_string(), username.clone());
        if let Some(ref email_val) = email {
            session
                .metadata
                .insert("email".to_string(), email_val.clone());
        }

        // Create AuthUser for JWT generation
        // Use session.id as the subject (sub) instead of user_id
        let mut auth_user = AuthUser::new(session.id.to_string(), username);
        auth_user.email = email;
        auth_user.roles = roles.into_iter().collect();

        // Store user_id in metadata for reference
        session.metadata.insert("user_id".to_string(), user_id);

        // Generate JWT token pair
        let token_pair = self
            .jwt_auth
            .generate_token_pair(&auth_user)
            .map_err(|e| SessionError::Internal(format!("JWT generation failed: {}", e)))?;

        Ok(SessionWithTokens {
            session,
            tokens: token_pair,
        })
    }

    /// Verify JWT token and retrieve session
    pub async fn verify_token_and_get_session(
        &self,
        access_token: &str,
    ) -> Result<(Session, JwtClaims), SessionError> {
        // Verify JWT token
        let claims = self
            .jwt_auth
            .verify_access_token(access_token)
            .map_err(|e| SessionError::SecurityViolation(format!("Invalid token: {}", e)))?;

        // Check if token is revoked
        if self.is_token_revoked(&claims.jti).await {
            return Err(SessionError::SecurityViolation(
                "Token has been revoked".to_string(),
            ));
        }

        // Get session from storage
        let session_id = SessionId::from_string(claims.sub.clone());
        let session = self
            .session_manager
            .get_session(&session_id)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        // Touch session to update last_active
        self.session_manager.touch_session(&session.id).await?;

        Ok((session, claims))
    }

    /// Refresh tokens using refresh token
    pub async fn refresh_tokens(&self, refresh_token: &str) -> Result<JwtTokenPair, SessionError> {
        if !self.config.enable_refresh {
            return Err(SessionError::SecurityViolation(
                "Token refresh is disabled".to_string(),
            ));
        }

        // Verify refresh token
        let claims = self
            .jwt_auth
            .verify_refresh_token(refresh_token)
            .map_err(|e| {
                SessionError::SecurityViolation(format!("Invalid refresh token: {}", e))
            })?;

        // Check if token is revoked
        if self.is_token_revoked(&claims.jti).await {
            return Err(SessionError::SecurityViolation(
                "Refresh token has been revoked".to_string(),
            ));
        }

        // Get session to verify it still exists
        let session_id = SessionId::from_string(claims.sub.clone());
        let session = self
            .session_manager
            .get_session(&session_id)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;

        // Create AuthUser from session and claims
        let mut auth_user = AuthUser::new(session.id.to_string(), claims.username.clone());
        auth_user.email = claims.email.clone();
        auth_user.roles = claims
            .roles
            .iter()
            .filter_map(|r| match r.as_str() {
                "Admin" => Some(Role::Admin),
                "User" => Some(Role::User),
                "Guest" => Some(Role::Guest),
                _ => None,
            })
            .collect();

        // Generate new token pair
        let new_tokens = self
            .jwt_auth
            .generate_token_pair(&auth_user)
            .map_err(|e| SessionError::Internal(format!("Token generation failed: {}", e)))?;

        // Revoke old refresh token if rotation is enabled
        if self.config.rotate_on_refresh {
            self.revoke_token(claims.jti, "Token rotated".to_string())
                .await?;
        }

        Ok(new_tokens)
    }

    /// Revoke a token
    pub async fn revoke_token(&self, jti: String, reason: String) -> Result<(), SessionError> {
        let mut revoked = self.revoked_tokens.write().await;
        revoked.insert(jti.clone());

        tracing::info!("Token revoked: jti={}, reason={}", jti, reason);

        Ok(())
    }

    /// Check if a token is revoked
    pub async fn is_token_revoked(&self, jti: &str) -> bool {
        let revoked = self.revoked_tokens.read().await;
        revoked.contains(jti)
    }

    /// Logout: revoke all tokens associated with a session
    pub async fn logout(
        &self,
        session_id: &SessionId,
        access_jti: String,
        refresh_jti: String,
    ) -> Result<(), SessionError> {
        // Delete session
        self.session_manager.delete_session(session_id).await?;

        // Revoke both tokens
        self.revoke_token(access_jti, "User logout".to_string())
            .await?;
        self.revoke_token(refresh_jti, "User logout".to_string())
            .await?;

        Ok(())
    }

    /// Force logout all sessions for a user
    pub async fn force_logout_user(&self, user_id: &str) -> Result<usize, SessionError> {
        // Get all sessions for the user
        let filter = crate::session::types::SessionFilter {
            user_id: Some(user_id.to_string()),
            state: None,
        };

        let sessions = self.session_manager.list_sessions(&filter).await?;
        let count = sessions.len();

        // Delete all sessions
        for session in sessions {
            self.session_manager.delete_session(&session.id).await?;
        }

        tracing::info!(
            "Force logout: user_id={}, sessions_deleted={}",
            user_id,
            count
        );

        Ok(count)
    }

    /// Get session manager reference
    pub fn session_manager(&self) -> &Arc<SessionManager> {
        &self.session_manager
    }

    /// Get JWT auth reference
    pub fn jwt_auth(&self) -> &Arc<JwtAuth> {
        &self.jwt_auth
    }
}

/// Session with JWT tokens
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionWithTokens {
    pub session: Session,
    pub tokens: JwtTokenPair,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionState;

    #[tokio::test]
    async fn test_create_session_with_tokens() {
        let session_manager = Arc::new(SessionManager::new());
        let config = SessionJwtConfig::default();
        let jwt_manager = SessionJwtManager::new(session_manager, config);

        let result = jwt_manager
            .create_session_with_tokens(
                "user123".to_string(),
                "testuser".to_string(),
                Some("test@example.com".to_string()),
                vec![Role::User],
            )
            .await;

        assert!(result.is_ok());
        let session_with_tokens = result.unwrap();
        assert_eq!(session_with_tokens.session.state, SessionState::Pending);
        assert!(!session_with_tokens.tokens.access_token.is_empty());
        assert!(!session_with_tokens.tokens.refresh_token.is_empty());
    }

    #[tokio::test]
    async fn test_verify_token_and_get_session() {
        let session_manager = Arc::new(SessionManager::new());
        let config = SessionJwtConfig::default();
        let jwt_manager = SessionJwtManager::new(session_manager, config);

        // Create session with tokens
        let session_with_tokens = jwt_manager
            .create_session_with_tokens(
                "user123".to_string(),
                "testuser".to_string(),
                Some("test@example.com".to_string()),
                vec![Role::User],
            )
            .await
            .unwrap();

        // Verify token
        let result = jwt_manager
            .verify_token_and_get_session(&session_with_tokens.tokens.access_token)
            .await;

        assert!(result.is_ok());
        let (session, claims) = result.unwrap();
        assert_eq!(session.id, session_with_tokens.session.id);
        assert_eq!(claims.username, "testuser");
    }

    #[tokio::test]
    async fn test_refresh_tokens() {
        let session_manager = Arc::new(SessionManager::new());
        let config = SessionJwtConfig::default();
        let jwt_manager = SessionJwtManager::new(session_manager, config);

        // Create session with tokens
        let session_with_tokens = jwt_manager
            .create_session_with_tokens(
                "user123".to_string(),
                "testuser".to_string(),
                Some("test@example.com".to_string()),
                vec![Role::User],
            )
            .await
            .unwrap();

        // Refresh tokens
        let result = jwt_manager
            .refresh_tokens(&session_with_tokens.tokens.refresh_token)
            .await;

        assert!(result.is_ok());
        let new_tokens = result.unwrap();
        assert_ne!(
            new_tokens.access_token,
            session_with_tokens.tokens.access_token
        );
    }

    #[tokio::test]
    async fn test_logout() {
        let session_manager = Arc::new(SessionManager::new());
        let config = SessionJwtConfig::default();
        let jwt_manager = SessionJwtManager::new(session_manager.clone(), config);

        // Create session with tokens
        let session_with_tokens = jwt_manager
            .create_session_with_tokens(
                "user123".to_string(),
                "testuser".to_string(),
                Some("test@example.com".to_string()),
                vec![Role::User],
            )
            .await
            .unwrap();

        // Extract JTIs from tokens
        let access_claims = jwt_manager
            .jwt_auth
            .verify_access_token(&session_with_tokens.tokens.access_token)
            .unwrap();
        let refresh_claims = jwt_manager
            .jwt_auth
            .verify_refresh_token(&session_with_tokens.tokens.refresh_token)
            .unwrap();

        // Logout
        let result = jwt_manager
            .logout(
                &session_with_tokens.session.id,
                access_claims.jti,
                refresh_claims.jti,
            )
            .await;

        assert!(result.is_ok());

        // Verify session is deleted
        let session = session_manager
            .get_session(&session_with_tokens.session.id)
            .await
            .unwrap();
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn test_token_revocation() {
        let session_manager = Arc::new(SessionManager::new());
        let config = SessionJwtConfig::default();
        let jwt_manager = SessionJwtManager::new(session_manager, config);

        let jti = "test-jti".to_string();

        // Initially not revoked
        assert!(!jwt_manager.is_token_revoked(&jti).await);

        // Revoke token
        jwt_manager
            .revoke_token(jti.clone(), "Test revocation".to_string())
            .await
            .unwrap();

        // Now should be revoked
        assert!(jwt_manager.is_token_revoked(&jti).await);
    }

    #[tokio::test]
    async fn test_force_logout_user() {
        let session_manager = Arc::new(SessionManager::new());
        let config = SessionJwtConfig::default();
        let jwt_manager = SessionJwtManager::new(session_manager, config);

        // Create multiple sessions for the same user
        for _ in 0..3 {
            jwt_manager
                .create_session_with_tokens(
                    "user123".to_string(),
                    "testuser".to_string(),
                    Some("test@example.com".to_string()),
                    vec![Role::User],
                )
                .await
                .unwrap();
        }

        // Force logout
        let count = jwt_manager.force_logout_user("user123").await.unwrap();
        assert_eq!(count, 3);
    }
}
