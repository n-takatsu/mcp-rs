// Session-based Authentication

use super::types::{AuthError, AuthResult, AuthUser};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// セッション設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// セッションの有効期限（秒）
    #[serde(default = "default_session_duration")]
    pub session_duration: u64,

    /// セッションの延長を有効化
    #[serde(default = "default_true")]
    pub enable_renewal: bool,

    /// セッションクッキー名
    #[serde(default = "default_cookie_name")]
    pub cookie_name: String,

    /// セッションクッキーのHTTPOnly属性
    #[serde(default = "default_true")]
    pub cookie_http_only: bool,

    /// セッションクッキーのSecure属性
    #[serde(default = "default_true")]
    pub cookie_secure: bool,

    /// セッションクッキーのSameSite属性
    #[serde(default = "default_same_site")]
    pub cookie_same_site: String,

    /// セッションストレージのクリーンアップ間隔（秒）
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval: u64,
}

fn default_session_duration() -> u64 {
    3600 // 1時間
}

fn default_true() -> bool {
    true
}

fn default_cookie_name() -> String {
    "mcp_session".to_string()
}

fn default_same_site() -> String {
    "Lax".to_string()
}

fn default_cleanup_interval() -> u64 {
    300 // 5分
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            session_duration: default_session_duration(),
            enable_renewal: default_true(),
            cookie_name: default_cookie_name(),
            cookie_http_only: default_true(),
            cookie_secure: default_true(),
            cookie_same_site: default_same_site(),
            cleanup_interval: default_cleanup_interval(),
        }
    }
}

/// セッショントークン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToken {
    /// セッションID
    pub session_id: String,

    /// ユーザー情報
    pub user: AuthUser,

    /// 作成日時
    pub created_at: u64,

    /// 有効期限
    pub expires_at: u64,

    /// 最終アクセス日時
    pub last_accessed_at: u64,

    /// IPアドレス
    pub ip_address: Option<String>,

    /// ユーザーエージェント
    pub user_agent: Option<String>,

    /// カスタムデータ
    #[serde(default)]
    pub data: HashMap<String, String>,
}

impl SessionToken {
    pub fn new(user: AuthUser, duration: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            session_id: Uuid::new_v4().to_string(),
            user,
            created_at: now,
            expires_at: now + duration,
            last_accessed_at: now,
            ip_address: None,
            user_agent: None,
            data: HashMap::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now < self.expires_at
    }

    pub fn renew(&mut self, duration: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_accessed_at = now;
        self.expires_at = now + duration;
    }
}

/// セッション認証システム
pub struct SessionAuth {
    config: SessionConfig,
    sessions: Arc<RwLock<HashMap<String, SessionToken>>>,
}

impl SessionAuth {
    pub fn new(config: SessionConfig) -> Self {
        let sessions = Arc::new(RwLock::new(HashMap::new()));

        // クリーンアップタスクを開始
        let cleanup_sessions = Arc::clone(&sessions);
        let cleanup_interval = config.cleanup_interval;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(cleanup_interval)).await;
                Self::cleanup_expired_sessions(&cleanup_sessions);
            }
        });

        Self { config, sessions }
    }

    /// 新しいセッションを作成
    pub fn create_session(&self, user: AuthUser) -> AuthResult<SessionToken> {
        let session = SessionToken::new(user, self.config.session_duration);
        let session_id = session.session_id.clone();

        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| AuthError::Internal(format!("Lock error: {}", e)))?;

        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    /// セッションを検証
    pub fn verify_session(&self, session_id: &str) -> AuthResult<SessionToken> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| AuthError::Internal(format!("Lock error: {}", e)))?;

        let session = sessions
            .get_mut(session_id)
            .ok_or(AuthError::Unauthorized("Invalid session".to_string()))?;

        if !session.is_valid() {
            sessions.remove(session_id);
            return Err(AuthError::TokenExpired);
        }

        // セッションを延長
        if self.config.enable_renewal {
            session.renew(self.config.session_duration);
        }

        Ok(session.clone())
    }

    /// セッションを更新
    pub fn update_session<F>(&self, session_id: &str, f: F) -> AuthResult<()>
    where
        F: FnOnce(&mut SessionToken),
    {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| AuthError::Internal(format!("Lock error: {}", e)))?;

        let session = sessions
            .get_mut(session_id)
            .ok_or(AuthError::Unauthorized("Invalid session".to_string()))?;

        f(session);
        Ok(())
    }

    /// セッションを破棄
    pub fn destroy_session(&self, session_id: &str) -> AuthResult<()> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| AuthError::Internal(format!("Lock error: {}", e)))?;

        sessions
            .remove(session_id)
            .ok_or(AuthError::Unauthorized("Invalid session".to_string()))?;

        Ok(())
    }

    /// ユーザーの全セッションを破棄
    pub fn destroy_user_sessions(&self, user_id: &str) -> AuthResult<usize> {
        let mut sessions = self
            .sessions
            .write()
            .map_err(|e| AuthError::Internal(format!("Lock error: {}", e)))?;

        let session_ids: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.user.id == user_id)
            .map(|(id, _)| id.clone())
            .collect();

        let count = session_ids.len();
        for id in session_ids {
            sessions.remove(&id);
        }

        Ok(count)
    }

    /// ユーザーのセッション一覧を取得
    pub fn list_user_sessions(&self, user_id: &str) -> AuthResult<Vec<SessionToken>> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| AuthError::Internal(format!("Lock error: {}", e)))?;

        Ok(sessions
            .values()
            .filter(|s| s.user.id == user_id)
            .cloned()
            .collect())
    }

    /// 有効なセッション数を取得
    pub fn active_session_count(&self) -> AuthResult<usize> {
        let sessions = self
            .sessions
            .read()
            .map_err(|e| AuthError::Internal(format!("Lock error: {}", e)))?;

        Ok(sessions.len())
    }

    /// 期限切れセッションをクリーンアップ
    fn cleanup_expired_sessions(sessions: &Arc<RwLock<HashMap<String, SessionToken>>>) {
        if let Ok(mut sessions) = sessions.write() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            sessions.retain(|_, session| session.expires_at > now);
        }
    }
}

impl Clone for SessionAuth {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            sessions: Arc::clone(&self.sessions),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::types::Role;

    fn create_test_user() -> AuthUser {
        let mut user = AuthUser::new("test-id".to_string(), "testuser".to_string());
        user.roles.insert(Role::User);
        user
    }

    #[test]
    fn test_session_config_default() {
        let config = SessionConfig::default();
        assert_eq!(config.session_duration, 3600);
        assert_eq!(config.cookie_name, "mcp_session");
    }

    #[tokio::test]
    async fn test_create_and_verify_session() {
        let config = SessionConfig::default();
        let session_auth = SessionAuth::new(config);
        let user = create_test_user();

        let session = session_auth.create_session(user).unwrap();
        assert!(session.is_valid());

        let verified = session_auth.verify_session(&session.session_id).unwrap();
        assert_eq!(verified.user.id, "test-id");
    }

    #[tokio::test]
    async fn test_destroy_session() {
        let config = SessionConfig::default();
        let session_auth = SessionAuth::new(config);
        let user = create_test_user();

        let session = session_auth.create_session(user).unwrap();
        session_auth.destroy_session(&session.session_id).unwrap();

        assert!(session_auth.verify_session(&session.session_id).is_err());
    }

    #[tokio::test]
    async fn test_session_expiration() {
        let config = SessionConfig {
            session_duration: 1, // 1秒
            enable_renewal: false,
            ..SessionConfig::default()
        };

        let session_auth = SessionAuth::new(config);
        let user = create_test_user();

        let session = session_auth.create_session(user).unwrap();

        // すぐに検証できる
        assert!(session_auth.verify_session(&session.session_id).is_ok());

        // 2秒待つ
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 期限切れ
        assert!(session_auth.verify_session(&session.session_id).is_err());
    }

    #[tokio::test]
    async fn test_destroy_user_sessions() {
        let config = SessionConfig::default();
        let session_auth = SessionAuth::new(config);
        let user = create_test_user();

        // 複数のセッションを作成
        session_auth.create_session(user.clone()).unwrap();
        session_auth.create_session(user.clone()).unwrap();
        session_auth.create_session(user.clone()).unwrap();

        let count = session_auth.destroy_user_sessions(&user.id).unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_update_session() {
        let config = SessionConfig::default();
        let session_auth = SessionAuth::new(config);
        let user = create_test_user();

        let session = session_auth.create_session(user).unwrap();

        session_auth
            .update_session(&session.session_id, |s| {
                s.data.insert("key".to_string(), "value".to_string());
            })
            .unwrap();

        let updated = session_auth.verify_session(&session.session_id).unwrap();
        assert_eq!(updated.data.get("key"), Some(&"value".to_string()));
    }
}
