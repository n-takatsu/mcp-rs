// Authentication Types and Core Structures

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// 認証エラー型
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    
    #[error("Token expired")]
    TokenExpired,
    
    #[error("Token invalid: {0}")]
    TokenInvalid(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    
    #[error("Forbidden: {0}")]
    Forbidden(String),
    
    #[error("User not found")]
    UserNotFound,
    
    #[error("User already exists")]
    UserAlreadyExists,
    
    #[error("Password too weak")]
    WeakPassword,
    
    #[error("MFA required")]
    MfaRequired,
    
    #[error("MFA invalid")]
    MfaInvalid,
    
    #[error("Provider error: {0}")]
    ProviderError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// 認証結果型
pub type AuthResult<T> = Result<T, AuthError>;

/// 認証方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    /// JWT認証
    Jwt,
    /// OAuth2認証
    OAuth2,
    /// APIキー認証
    ApiKey,
    /// セッション認証
    Session,
    /// Basic認証
    Basic,
    /// Bearer認証
    Bearer,
}

/// 認証プロバイダー
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthProvider {
    /// ローカル認証
    Local,
    /// Google OAuth2
    Google,
    /// GitHub OAuth2
    GitHub,
    /// Microsoft OAuth2
    Microsoft,
    /// カスタムOAuth2
    Custom(String),
}

/// ユーザーロール
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// 管理者
    Admin,
    /// ユーザー
    User,
    /// ゲスト
    Guest,
    /// カスタムロール
    Custom(String),
}

impl Role {
    /// ロールの優先度を取得
    pub fn priority(&self) -> u8 {
        match self {
            Role::Admin => 100,
            Role::User => 50,
            Role::Guest => 10,
            Role::Custom(_) => 25,
        }
    }
    
    /// 管理者権限を持つか
    pub fn is_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

/// パーミッション
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Permission {
    /// リソース名
    pub resource: String,
    /// アクション（read, write, delete等）
    pub action: String,
}

impl Permission {
    pub fn new(resource: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            action: action.into(),
        }
    }
    
    pub fn read(resource: impl Into<String>) -> Self {
        Self::new(resource, "read")
    }
    
    pub fn write(resource: impl Into<String>) -> Self {
        Self::new(resource, "write")
    }
    
    pub fn delete(resource: impl Into<String>) -> Self {
        Self::new(resource, "delete")
    }
    
    pub fn admin(resource: impl Into<String>) -> Self {
        Self::new(resource, "admin")
    }
}

/// 認証済みユーザー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    /// ユーザーID
    pub id: String,
    /// ユーザー名
    pub username: String,
    /// メールアドレス
    pub email: Option<String>,
    /// ロール
    pub roles: HashSet<Role>,
    /// パーミッション
    pub permissions: HashSet<Permission>,
    /// プロバイダー
    pub provider: AuthProvider,
    /// メタデータ
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl AuthUser {
    pub fn new(id: String, username: String) -> Self {
        Self {
            id,
            username,
            email: None,
            roles: HashSet::new(),
            permissions: HashSet::new(),
            provider: AuthProvider::Local,
            metadata: std::collections::HashMap::new(),
        }
    }
    
    /// ロールを持つか確認
    pub fn has_role(&self, role: &Role) -> bool {
        self.roles.contains(role)
    }
    
    /// パーミッションを持つか確認
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }
    
    /// リソースへのアクションが許可されているか
    pub fn can(&self, resource: &str, action: &str) -> bool {
        let permission = Permission::new(resource, action);
        self.has_permission(&permission) || self.has_role(&Role::Admin)
    }
    
    /// 管理者か確認
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|r| r.is_admin())
    }
}

/// 認証クレデンシャル
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Credentials {
    /// ユーザー名とパスワード
    Password {
        username: String,
        #[serde(skip_serializing)]
        password: String,
    },
    /// APIキー
    ApiKey {
        key: String,
    },
    /// OAuth2トークン
    OAuth2 {
        provider: AuthProvider,
        token: String,
    },
    /// JWTトークン
    Jwt {
        token: String,
    },
    /// セッショントークン
    Session {
        session_id: String,
    },
}

/// パスワードハッシャー
#[derive(Clone)]
pub struct PasswordHasher {
    salt_rounds: u32,
}

impl PasswordHasher {
    pub fn new(salt_rounds: u32) -> Self {
        Self { salt_rounds }
    }
    
    /// パスワードをハッシュ化
    pub fn hash(&self, password: &str) -> AuthResult<String> {
        use argon2::{
            password_hash::{PasswordHasher as _, SaltString},
            Argon2,
        };
        
        let salt = SaltString::generate(&mut rand::thread_rng());
        let argon2 = Argon2::default();
        
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| AuthError::Internal(format!("Password hashing failed: {}", e)))
    }
    
    /// パスワードを検証
    pub fn verify(&self, password: &str, hash: &str) -> AuthResult<bool> {
        use argon2::{
            password_hash::{PasswordHash, PasswordVerifier},
            Argon2,
        };
        
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AuthError::Internal(format!("Invalid password hash: {}", e)))?;
        
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
    
    /// パスワード強度をチェック
    pub fn check_strength(password: &str) -> AuthResult<()> {
        if password.len() < 8 {
            return Err(AuthError::WeakPassword);
        }
        
        let has_lowercase = password.chars().any(|c| c.is_lowercase());
        let has_uppercase = password.chars().any(|c| c.is_uppercase());
        let has_digit = password.chars().any(|c| c.is_numeric());
        let has_special = password.chars().any(|c| !c.is_alphanumeric());
        
        if !(has_lowercase && has_uppercase && has_digit && has_special) {
            return Err(AuthError::WeakPassword);
        }
        
        Ok(())
    }
}

impl Default for PasswordHasher {
    fn default() -> Self {
        Self::new(12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_priority() {
        assert!(Role::Admin.priority() > Role::User.priority());
        assert!(Role::User.priority() > Role::Guest.priority());
    }

    #[test]
    fn test_auth_user_permissions() {
        let mut user = AuthUser::new("1".to_string(), "test".to_string());
        user.permissions.insert(Permission::read("posts"));
        
        assert!(user.can("posts", "read"));
        assert!(!user.can("posts", "write"));
    }

    #[test]
    fn test_auth_user_admin() {
        let mut user = AuthUser::new("1".to_string(), "admin".to_string());
        user.roles.insert(Role::Admin);
        
        assert!(user.is_admin());
        assert!(user.can("anything", "anything"));
    }

    #[test]
    fn test_password_strength() {
        assert!(PasswordHasher::check_strength("Weak123!").is_ok());
        assert!(PasswordHasher::check_strength("weak").is_err());
        assert!(PasswordHasher::check_strength("NOLOWER123!").is_err());
    }

    #[test]
    fn test_password_hash_verify() {
        let hasher = PasswordHasher::default();
        let password = "TestPassword123!";
        let hash = hasher.hash(password).unwrap();
        
        assert!(hasher.verify(password, &hash).unwrap());
        assert!(!hasher.verify("WrongPassword", &hash).unwrap());
    }
}
