// Authentication System Module
//
// 包括的な認証システムの実装
// - JWT認証
// - OAuth2統合
// - API Key認証
// - セッション管理
// - 認証ミドルウェア
// - REST API エンドポイント

pub mod api;
pub mod api_key;
pub mod jwt;
pub mod middleware;
pub mod oauth2;
pub mod provider;
pub mod repository;
pub mod session_auth;
pub mod types;

pub use api::{
    create_auth_router, AuthApiState, LoginRequest, LoginResponse, RefreshRequest,
    RefreshResponse, RegisterRequest, RegisterResponse, UserInfo,
};
pub use api_key::{ApiKey, ApiKeyConfig, ApiKeyManager, ApiKeyPermission};
pub use jwt::{JwtAuth, JwtClaims, JwtConfig, JwtError, JwtTokenPair};
pub use middleware::{AuthMiddleware, AuthRequirement};
pub use oauth2::{OAuth2Config, OAuth2Error, OAuth2Provider, OAuth2Token};
pub use provider::{AuthenticationProvider, MultiAuthProvider};
pub use repository::{UserRepository, memory::InMemoryUserRepository};

#[cfg(feature = "postgresql-backend")]
pub use repository::postgres::PostgresUserRepository;

#[cfg(feature = "redis-backend")]
pub use repository::redis::{RedisSessionStore, SessionData};

pub use session_auth::{SessionAuth, SessionConfig, SessionToken};
pub use types::{
    AuthError, AuthMethod, AuthProvider, AuthResult, AuthUser, Credentials, PasswordHasher,
    Permission, Role,
};

/// 認証システムのバージョン
pub const AUTH_VERSION: &str = "1.0.0";

/// 認証設定の統合構造体
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AuthConfig {
    /// JWT認証設定
    #[serde(default)]
    pub jwt: Option<JwtConfig>,

    /// OAuth2認証設定
    #[serde(default)]
    pub oauth2: Option<Vec<OAuth2Config>>,

    /// APIキー認証設定
    #[serde(default)]
    pub api_key: Option<ApiKeyConfig>,

    /// セッション認証設定
    #[serde(default)]
    pub session: Option<SessionConfig>,

    /// デフォルト認証方式
    #[serde(default = "default_auth_method")]
    pub default_method: AuthMethod,

    /// 認証の有効期限（秒）
    #[serde(default = "default_token_expiration")]
    pub token_expiration: u64,

    /// リフレッシュトークンの有効期限（秒）
    #[serde(default = "default_refresh_expiration")]
    pub refresh_expiration: u64,

    /// パスワードのソルトラウンド
    #[serde(default = "default_salt_rounds")]
    pub password_salt_rounds: u32,
}

fn default_auth_method() -> AuthMethod {
    AuthMethod::Jwt
}

fn default_token_expiration() -> u64 {
    3600 // 1時間
}

fn default_refresh_expiration() -> u64 {
    86400 * 7 // 7日
}

fn default_salt_rounds() -> u32 {
    12
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt: Some(JwtConfig::default()),
            oauth2: None,
            api_key: Some(ApiKeyConfig::default()),
            session: Some(SessionConfig::default()),
            default_method: default_auth_method(),
            token_expiration: default_token_expiration(),
            refresh_expiration: default_refresh_expiration(),
            password_salt_rounds: default_salt_rounds(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert_eq!(config.default_method, AuthMethod::Jwt);
        assert_eq!(config.token_expiration, 3600);
        assert!(config.jwt.is_some());
    }

    #[test]
    fn test_auth_version() {
        assert_eq!(AUTH_VERSION, "1.0.0");
    }
}
