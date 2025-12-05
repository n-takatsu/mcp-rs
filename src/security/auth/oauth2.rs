// OAuth2 Integration Implementation

use super::types::{AuthError, AuthProvider, AuthResult, AuthUser, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// OAuth2設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Config {
    /// プロバイダー名
    pub provider: AuthProvider,

    /// クライアントID
    pub client_id: String,

    /// クライアントシークレット
    #[serde(skip_serializing)]
    pub client_secret: String,

    /// 認可エンドポイント
    pub authorization_endpoint: String,

    /// トークンエンドポイント
    pub token_endpoint: String,

    /// ユーザー情報エンドポイント
    pub userinfo_endpoint: String,

    /// リダイレクトURI
    pub redirect_uri: String,

    /// スコープ
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// 状態検証の有効化
    #[serde(default = "default_true")]
    pub enable_state_validation: bool,

    /// PKCE（Proof Key for Code Exchange）の使用
    #[serde(default = "default_true")]
    pub use_pkce: bool,
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "profile".to_string(),
        "email".to_string(),
    ]
}

fn default_true() -> bool {
    true
}

impl OAuth2Config {
    /// Google OAuth2設定を作成
    pub fn google(client_id: String, client_secret: String, redirect_uri: String) -> Self {
        Self {
            provider: AuthProvider::Google,
            client_id,
            client_secret,
            authorization_endpoint: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_endpoint: "https://oauth2.googleapis.com/token".to_string(),
            userinfo_endpoint: "https://www.googleapis.com/oauth2/v3/userinfo".to_string(),
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            enable_state_validation: true,
            use_pkce: true,
        }
    }

    /// GitHub OAuth2設定を作成
    pub fn github(client_id: String, client_secret: String, redirect_uri: String) -> Self {
        Self {
            provider: AuthProvider::GitHub,
            client_id,
            client_secret,
            authorization_endpoint: "https://github.com/login/oauth/authorize".to_string(),
            token_endpoint: "https://github.com/login/oauth/access_token".to_string(),
            userinfo_endpoint: "https://api.github.com/user".to_string(),
            redirect_uri,
            scopes: vec!["user:email".to_string()],
            enable_state_validation: true,
            use_pkce: false, // GitHubはPKCEをサポートしていない
        }
    }

    /// Microsoft OAuth2設定を作成
    pub fn microsoft(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        tenant: String,
    ) -> Self {
        Self {
            provider: AuthProvider::Microsoft,
            client_id,
            client_secret,
            authorization_endpoint: format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
                tenant
            ),
            token_endpoint: format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
                tenant
            ),
            userinfo_endpoint: "https://graph.microsoft.com/v1.0/me".to_string(),
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "profile".to_string(),
                "email".to_string(),
            ],
            enable_state_validation: true,
            use_pkce: true,
        }
    }
}

/// OAuth2トークン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Token {
    /// アクセストークン
    pub access_token: String,

    /// トークンタイプ
    pub token_type: String,

    /// 有効期限（秒）
    pub expires_in: Option<u64>,

    /// リフレッシュトークン
    pub refresh_token: Option<String>,

    /// スコープ
    pub scope: Option<String>,

    /// IDトークン（OpenID Connect）
    pub id_token: Option<String>,

    /// 発行時刻
    #[serde(default = "current_timestamp")]
    pub issued_at: u64,
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

impl OAuth2Token {
    pub fn is_expired(&self) -> bool {
        if let Some(expires_in) = self.expires_in {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > self.issued_at + expires_in
        } else {
            false
        }
    }
}

/// OAuth2エラー
#[derive(Debug, thiserror::Error)]
pub enum OAuth2Error {
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),

    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),

    #[error("User info fetch failed: {0}")]
    UserInfoFailed(String),

    #[error("Invalid state")]
    InvalidState,

    #[error("Provider not configured: {0:?}")]
    ProviderNotConfigured(AuthProvider),
}

impl From<OAuth2Error> for AuthError {
    fn from(err: OAuth2Error) -> Self {
        AuthError::ProviderError(err.to_string())
    }
}

/// OAuth2ユーザー情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2UserInfo {
    /// ユーザーID（プロバイダー固有）
    pub id: String,

    /// メールアドレス
    pub email: Option<String>,

    /// 名前
    pub name: Option<String>,

    /// ユーザー名
    pub username: Option<String>,

    /// プロフィール画像URL
    pub picture: Option<String>,

    /// メール確認済み
    #[serde(default)]
    pub email_verified: bool,

    /// ロケール
    pub locale: Option<String>,

    /// 追加属性
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// OAuth2プロバイダー
pub struct OAuth2Provider {
    config: OAuth2Config,
    http_client: reqwest::Client,
    state_store: HashMap<String, String>,
}

impl OAuth2Provider {
    pub fn new(config: OAuth2Config) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            state_store: HashMap::new(),
        }
    }

    /// 認可URLを生成
    pub fn generate_authorization_url(&mut self) -> AuthResult<String> {
        let state = if self.config.enable_state_validation {
            let state = uuid::Uuid::new_v4().to_string();
            self.state_store.insert(state.clone(), state.clone());
            Some(state)
        } else {
            None
        };

        let mut params = vec![
            ("client_id", self.config.client_id.clone()),
            ("redirect_uri", self.config.redirect_uri.clone()),
            ("response_type", "code".to_string()),
            ("scope", self.config.scopes.join(" ")),
        ];

        if let Some(state) = state {
            params.push(("state", state));
        }

        let query_string = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        Ok(format!(
            "{}?{}",
            self.config.authorization_endpoint, query_string
        ))
    }

    /// 認可コードをトークンに交換
    pub async fn exchange_code(
        &self,
        code: String,
        state: Option<String>,
    ) -> AuthResult<OAuth2Token> {
        // 状態検証
        if self.config.enable_state_validation {
            let state = state.ok_or(OAuth2Error::InvalidState)?;
            if !self.state_store.contains_key(&state) {
                return Err(OAuth2Error::InvalidState.into());
            }
        }

        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", &code);
        params.insert("client_id", &self.config.client_id);
        params.insert("client_secret", &self.config.client_secret);
        params.insert("redirect_uri", &self.config.redirect_uri);

        let response = self
            .http_client
            .post(&self.config.token_endpoint)
            .form(&params)
            .send()
            .await
            .map_err(|e| OAuth2Error::TokenExchangeFailed(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(OAuth2Error::TokenExchangeFailed(error_text).into());
        }

        let mut token: OAuth2Token = response
            .json()
            .await
            .map_err(|e| OAuth2Error::TokenExchangeFailed(e.to_string()))?;

        token.issued_at = current_timestamp();
        Ok(token)
    }

    /// ユーザー情報を取得
    pub async fn get_user_info(&self, token: &OAuth2Token) -> AuthResult<OAuth2UserInfo> {
        let response = self
            .http_client
            .get(&self.config.userinfo_endpoint)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| OAuth2Error::UserInfoFailed(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(OAuth2Error::UserInfoFailed(error_text).into());
        }

        let user_info: OAuth2UserInfo = response
            .json()
            .await
            .map_err(|e| OAuth2Error::UserInfoFailed(e.to_string()))?;

        Ok(user_info)
    }

    /// OAuth2ユーザー情報をAuthUserに変換
    pub fn user_info_to_auth_user(&self, user_info: OAuth2UserInfo) -> AuthUser {
        let mut user = AuthUser::new(
            user_info.id.clone(),
            user_info.username.clone().unwrap_or(user_info.id),
        );

        user.email = user_info.email;
        user.provider = self.config.provider.clone();
        user.roles.insert(Role::User);

        if let Some(name) = user_info.name {
            user.metadata.insert("name".to_string(), name);
        }
        if let Some(picture) = user_info.picture {
            user.metadata.insert("picture".to_string(), picture);
        }

        user
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_config() {
        let config = OAuth2Config::google(
            "client-id".to_string(),
            "client-secret".to_string(),
            "http://localhost/callback".to_string(),
        );

        assert_eq!(config.provider, AuthProvider::Google);
        assert!(config.use_pkce);
        assert!(config.scopes.contains(&"openid".to_string()));
    }

    #[test]
    fn test_github_config() {
        let config = OAuth2Config::github(
            "client-id".to_string(),
            "client-secret".to_string(),
            "http://localhost/callback".to_string(),
        );

        assert_eq!(config.provider, AuthProvider::GitHub);
        assert!(!config.use_pkce);
    }

    #[test]
    fn test_oauth2_token_expiration() {
        let mut token = OAuth2Token {
            access_token: "test".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            refresh_token: None,
            scope: None,
            id_token: None,
            issued_at: current_timestamp(),
        };

        assert!(!token.is_expired());

        // 過去の発行時刻に設定
        token.issued_at = current_timestamp() - 7200;
        assert!(token.is_expired());
    }

    #[test]
    fn test_generate_authorization_url() {
        let config = OAuth2Config::google(
            "test-client-id".to_string(),
            "test-secret".to_string(),
            "http://localhost/callback".to_string(),
        );

        let mut provider = OAuth2Provider::new(config);
        let url = provider.generate_authorization_url().unwrap();

        assert!(url.contains("client_id=test-client-id"));
        assert!(url.contains("redirect_uri="));
        assert!(url.contains("response_type=code"));
    }
}
