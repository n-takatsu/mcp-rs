// Authentication Provider - Unified authentication interface

use super::api_key::{ApiKey, ApiKeyManager};
use super::jwt::JwtAuth;
use super::oauth2::OAuth2Provider;
use super::session_auth::SessionAuth;
use super::types::{AuthError, AuthMethod, AuthProvider as AuthProviderType, AuthResult, AuthUser, Credentials, PasswordHasher};
use super::{ApiKeyConfig, JwtConfig, OAuth2Config, SessionConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 認証プロバイダートレイト
#[async_trait::async_trait]
pub trait AuthenticationProvider: Send + Sync {
    /// ユーザーを認証
    async fn authenticate(&self, credentials: Credentials) -> AuthResult<AuthUser>;
    
    /// トークンを検証
    async fn verify_token(&self, token: &str, method: AuthMethod) -> AuthResult<AuthUser>;
    
    /// トークンをリフレッシュ
    async fn refresh_token(&self, refresh_token: &str, user: &AuthUser) -> AuthResult<String>;
    
    /// ログアウト
    async fn logout(&self, token: &str) -> AuthResult<()>;
}

/// マルチ認証プロバイダー
#[derive(Clone)]
pub struct MultiAuthProvider {
    /// JWT認証
    jwt: Option<Arc<JwtAuth>>,
    
    /// OAuth2プロバイダー
    oauth2_providers: HashMap<AuthProviderType, Arc<OAuth2Provider>>,
    
    /// APIキーマネージャー
    api_key: Option<Arc<RwLock<ApiKeyManager>>>,
    
    /// セッション認証
    session: Option<Arc<SessionAuth>>,
    
    /// パスワードハッシャー
    password_hasher: PasswordHasher,
    
    /// ユーザーストア（デモ用、実際はDBに保存）
    users: Arc<RwLock<HashMap<String, StoredUser>>>,
}

/// 保存されたユーザー情報
#[derive(Debug, Clone)]
struct StoredUser {
    user: AuthUser,
    password_hash: Option<String>,
}

impl MultiAuthProvider {
    pub fn new(
        jwt_config: Option<JwtConfig>,
        oauth2_configs: Option<Vec<OAuth2Config>>,
        api_key_config: Option<ApiKeyConfig>,
        session_config: Option<SessionConfig>,
        password_salt_rounds: u32,
    ) -> Self {
        let jwt = jwt_config.map(|c| Arc::new(JwtAuth::new(c)));
        
        let mut oauth2_providers = HashMap::new();
        if let Some(configs) = oauth2_configs {
            for config in configs {
                let provider_type = config.provider.clone();
                oauth2_providers.insert(provider_type, Arc::new(OAuth2Provider::new(config)));
            }
        }
        
        let api_key = api_key_config.map(|c| Arc::new(RwLock::new(ApiKeyManager::new(c))));
        let session = session_config.map(|c| Arc::new(SessionAuth::new(c)));
        
        Self {
            jwt,
            oauth2_providers,
            api_key,
            session,
            password_hasher: PasswordHasher::new(password_salt_rounds),
            users: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// ユーザーを登録
    pub async fn register_user(
        &self,
        username: String,
        password: String,
        email: Option<String>,
    ) -> AuthResult<AuthUser> {
        // パスワード強度チェック
        PasswordHasher::check_strength(&password)?;
        
        // パスワードをハッシュ化
        let password_hash = self.password_hasher.hash(&password)?;
        
        let mut user = AuthUser::new(uuid::Uuid::new_v4().to_string(), username.clone());
        user.email = email;
        user.provider = AuthProviderType::Local;
        
        let stored_user = StoredUser {
            user: user.clone(),
            password_hash: Some(password_hash),
        };
        
        let mut users = self.users.write().await;
        
        // ユーザー名の重複チェック
        if users.values().any(|u| u.user.username == username) {
            return Err(AuthError::UserAlreadyExists);
        }
        
        users.insert(user.id.clone(), stored_user);
        Ok(user)
    }
    
    /// ユーザー名でユーザーを取得
    pub async fn get_user_by_username(&self, username: &str) -> AuthResult<AuthUser> {
        let users = self.users.read().await;
        users
            .values()
            .find(|u| u.user.username == username)
            .map(|u| u.user.clone())
            .ok_or(AuthError::UserNotFound)
    }
    
    /// ユーザーIDでユーザーを取得
    pub async fn get_user_by_id(&self, user_id: &str) -> AuthResult<AuthUser> {
        let users = self.users.read().await;
        users
            .get(user_id)
            .map(|u| u.user.clone())
            .ok_or(AuthError::UserNotFound)
    }
}

#[async_trait::async_trait]
impl AuthenticationProvider for MultiAuthProvider {
    async fn authenticate(&self, credentials: Credentials) -> AuthResult<AuthUser> {
        match credentials {
            Credentials::Password { username, password } => {
                let users = self.users.read().await;
                let stored_user = users
                    .values()
                    .find(|u| u.user.username == username)
                    .ok_or(AuthError::InvalidCredentials)?;
                
                let password_hash = stored_user
                    .password_hash
                    .as_ref()
                    .ok_or(AuthError::InvalidCredentials)?;
                
                if !self.password_hasher.verify(&password, password_hash)? {
                    return Err(AuthError::InvalidCredentials);
                }
                
                Ok(stored_user.user.clone())
            }
            
            Credentials::ApiKey { key } => {
                let api_key_manager = self
                    .api_key
                    .as_ref()
                    .ok_or(AuthError::ConfigError("API key not configured".to_string()))?;
                
                let mut manager = api_key_manager.write().await;
                let api_key = manager.verify_key(&key)?;
                
                self.get_user_by_id(&api_key.user_id).await
            }
            
            Credentials::Jwt { token } => {
                let jwt = self
                    .jwt
                    .as_ref()
                    .ok_or(AuthError::ConfigError("JWT not configured".to_string()))?;
                
                let claims = jwt.verify_access_token(&token)?;
                self.get_user_by_id(&claims.sub).await
            }
            
            Credentials::Session { session_id } => {
                let session = self
                    .session
                    .as_ref()
                    .ok_or(AuthError::ConfigError("Session not configured".to_string()))?;
                
                let session_token = session.verify_session(&session_id)?;
                Ok(session_token.user)
            }
            
            Credentials::OAuth2 { provider, token: _ } => {
                let _oauth2 = self
                    .oauth2_providers
                    .get(&provider)
                    .ok_or(AuthError::ConfigError(format!("OAuth2 provider {:?} not configured", provider)))?;
                
                // トークンを使ってユーザー情報を取得
                // 実際の実装ではOAuth2トークン構造体が必要
                Err(AuthError::Internal("OAuth2 token verification not implemented".to_string()))
            }
        }
    }
    
    async fn verify_token(&self, token: &str, method: AuthMethod) -> AuthResult<AuthUser> {
        match method {
            AuthMethod::Jwt | AuthMethod::Bearer => {
                let jwt = self
                    .jwt
                    .as_ref()
                    .ok_or(AuthError::ConfigError("JWT not configured".to_string()))?;
                
                let claims = jwt.verify_access_token(token)?;
                self.get_user_by_id(&claims.sub).await
            }
            
            AuthMethod::ApiKey => {
                let api_key_manager = self
                    .api_key
                    .as_ref()
                    .ok_or(AuthError::ConfigError("API key not configured".to_string()))?;
                
                let mut manager = api_key_manager.write().await;
                let api_key = manager.verify_key(token)?;
                self.get_user_by_id(&api_key.user_id).await
            }
            
            AuthMethod::Session => {
                let session = self
                    .session
                    .as_ref()
                    .ok_or(AuthError::ConfigError("Session not configured".to_string()))?;
                
                let session_token = session.verify_session(token)?;
                Ok(session_token.user)
            }
            
            _ => Err(AuthError::ConfigError(format!("Method {:?} not supported for token verification", method))),
        }
    }
    
    async fn refresh_token(&self, refresh_token: &str, user: &AuthUser) -> AuthResult<String> {
        let jwt = self
            .jwt
            .as_ref()
            .ok_or(AuthError::ConfigError("JWT not configured".to_string()))?;
        
        let token_pair = jwt.refresh(refresh_token, user)?;
        Ok(token_pair.access_token)
    }
    
    async fn logout(&self, token: &str) -> AuthResult<()> {
        // セッションの場合は破棄
        if let Some(session) = &self.session {
            session.destroy_session(token).ok();
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::types::Role;

    #[tokio::test]
    async fn test_register_and_authenticate_user() {
        let provider = MultiAuthProvider::new(
            Some(JwtConfig::default()),
            None,
            None,
            None,
            12,
        );
        
        // ユーザー登録
        let user = provider
            .register_user(
                "testuser".to_string(),
                "TestPass123!".to_string(),
                Some("test@example.com".to_string()),
            )
            .await
            .unwrap();
        
        assert_eq!(user.username, "testuser");
        
        // 認証
        let credentials = Credentials::Password {
            username: "testuser".to_string(),
            password: "TestPass123!".to_string(),
        };
        
        let auth_user = provider.authenticate(credentials).await.unwrap();
        assert_eq!(auth_user.username, "testuser");
    }

    #[tokio::test]
    async fn test_weak_password_rejection() {
        let provider = MultiAuthProvider::new(None, None, None, None, 12);
        
        let result = provider
            .register_user(
                "testuser".to_string(),
                "weak".to_string(),
                None,
            )
            .await;
        
        assert!(matches!(result, Err(AuthError::WeakPassword)));
    }
}
