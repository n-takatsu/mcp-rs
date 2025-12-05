// JWT Authentication Implementation

use super::types::{AuthError, AuthResult, AuthUser};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JWT設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// 秘密鍵
    pub secret: String,
    
    /// アクセストークンの有効期限（秒）
    #[serde(default = "default_access_expiration")]
    pub access_token_expiration: u64,
    
    /// リフレッシュトークンの有効期限（秒）
    #[serde(default = "default_refresh_expiration")]
    pub refresh_token_expiration: u64,
    
    /// イシュアー
    #[serde(default = "default_issuer")]
    pub issuer: String,
    
    /// オーディエンス
    #[serde(default)]
    pub audience: Option<String>,
    
    /// アルゴリズム
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
}

fn default_access_expiration() -> u64 {
    3600 // 1時間
}

fn default_refresh_expiration() -> u64 {
    86400 * 7 // 7日
}

fn default_issuer() -> String {
    "mcp-rs".to_string()
}

fn default_algorithm() -> String {
    "HS256".to_string()
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: uuid::Uuid::new_v4().to_string(),
            access_token_expiration: default_access_expiration(),
            refresh_token_expiration: default_refresh_expiration(),
            issuer: default_issuer(),
            audience: None,
            algorithm: default_algorithm(),
        }
    }
}

/// JWTクレーム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// サブジェクト（ユーザーID）
    pub sub: String,
    
    /// ユーザー名
    pub username: String,
    
    /// メールアドレス
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    
    /// ロール
    pub roles: Vec<String>,
    
    /// イシュアー
    pub iss: String,
    
    /// オーディエンス
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    
    /// 発行時刻
    pub iat: u64,
    
    /// 有効期限
    pub exp: u64,
    
    /// トークンタイプ（access/refresh）
    pub token_type: String,
    
    /// JTI（JWT ID）
    pub jti: String,
}

impl JwtClaims {
    pub fn new(
        user: &AuthUser,
        issuer: String,
        audience: Option<String>,
        expiration: u64,
        token_type: String,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            sub: user.id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            roles: user.roles.iter().map(|r| format!("{:?}", r)).collect(),
            iss: issuer,
            aud: audience,
            iat: now,
            exp: now + expiration,
            token_type,
            jti: uuid::Uuid::new_v4().to_string(),
        }
    }
    
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.exp < now
    }
    
    pub fn is_access_token(&self) -> bool {
        self.token_type == "access"
    }
    
    pub fn is_refresh_token(&self) -> bool {
        self.token_type == "refresh"
    }
}

/// JWTトークンペア
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtTokenPair {
    /// アクセストークン
    pub access_token: String,
    
    /// リフレッシュトークン
    pub refresh_token: String,
    
    /// トークンタイプ
    pub token_type: String,
    
    /// 有効期限（秒）
    pub expires_in: u64,
}

/// JWT認証エラー
#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("Token generation failed: {0}")]
    GenerationFailed(String),
    
    #[error("Token validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Token expired")]
    Expired,
    
    #[error("Invalid token")]
    Invalid,
}

impl From<JwtError> for AuthError {
    fn from(err: JwtError) -> Self {
        match err {
            JwtError::Expired => AuthError::TokenExpired,
            JwtError::Invalid | JwtError::ValidationFailed(_) => {
                AuthError::TokenInvalid(err.to_string())
            }
            JwtError::GenerationFailed(msg) => AuthError::Internal(msg),
        }
    }
}

/// JWT認証システム
pub struct JwtAuth {
    config: JwtConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtAuth {
    pub fn new(config: JwtConfig) -> Self {
        let encoding_key = EncodingKey::from_secret(config.secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.secret.as_bytes());
        
        let mut validation = Validation::default();
        validation.set_issuer(&[&config.issuer]);
        if let Some(ref aud) = config.audience {
            validation.set_audience(&[aud]);
        }
        
        Self {
            config,
            encoding_key,
            decoding_key,
            validation,
        }
    }
    
    /// トークンペアを生成
    pub fn generate_token_pair(&self, user: &AuthUser) -> AuthResult<JwtTokenPair> {
        let access_token = self.generate_access_token(user)?;
        let refresh_token = self.generate_refresh_token(user)?;
        
        Ok(JwtTokenPair {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: self.config.access_token_expiration,
        })
    }
    
    /// アクセストークンを生成
    pub fn generate_access_token(&self, user: &AuthUser) -> AuthResult<String> {
        let claims = JwtClaims::new(
            user,
            self.config.issuer.clone(),
            self.config.audience.clone(),
            self.config.access_token_expiration,
            "access".to_string(),
        );
        
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| JwtError::GenerationFailed(e.to_string()).into())
    }
    
    /// リフレッシュトークンを生成
    pub fn generate_refresh_token(&self, user: &AuthUser) -> AuthResult<String> {
        let claims = JwtClaims::new(
            user,
            self.config.issuer.clone(),
            self.config.audience.clone(),
            self.config.refresh_token_expiration,
            "refresh".to_string(),
        );
        
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| JwtError::GenerationFailed(e.to_string()).into())
    }
    
    /// トークンを検証
    pub fn verify_token(&self, token: &str) -> AuthResult<JwtClaims> {
        let token_data = decode::<JwtClaims>(token, &self.decoding_key, &self.validation)
            .map_err(|e| JwtError::ValidationFailed(e.to_string()))?;
        
        let claims = token_data.claims;
        
        if claims.is_expired() {
            return Err(JwtError::Expired.into());
        }
        
        Ok(claims)
    }
    
    /// アクセストークンを検証
    pub fn verify_access_token(&self, token: &str) -> AuthResult<JwtClaims> {
        let claims = self.verify_token(token)?;
        
        if !claims.is_access_token() {
            return Err(JwtError::Invalid.into());
        }
        
        Ok(claims)
    }
    
    /// リフレッシュトークンを検証
    pub fn verify_refresh_token(&self, token: &str) -> AuthResult<JwtClaims> {
        let claims = self.verify_token(token)?;
        
        if !claims.is_refresh_token() {
            return Err(JwtError::Invalid.into());
        }
        
        Ok(claims)
    }
    
    /// リフレッシュトークンから新しいトークンペアを生成
    pub fn refresh(&self, refresh_token: &str, user: &AuthUser) -> AuthResult<JwtTokenPair> {
        // リフレッシュトークンを検証
        self.verify_refresh_token(refresh_token)?;
        
        // 新しいトークンペアを生成
        self.generate_token_pair(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::types::Role;
    use std::collections::HashSet;

    fn create_test_user() -> AuthUser {
        let mut user = AuthUser::new("test-id".to_string(), "testuser".to_string());
        user.email = Some("test@example.com".to_string());
        user.roles.insert(Role::User);
        user
    }

    #[test]
    fn test_jwt_config_default() {
        let config = JwtConfig::default();
        assert_eq!(config.issuer, "mcp-rs");
        assert_eq!(config.access_token_expiration, 3600);
    }

    #[test]
    fn test_generate_and_verify_token() {
        let config = JwtConfig::default();
        let jwt_auth = JwtAuth::new(config);
        let user = create_test_user();
        
        let token = jwt_auth.generate_access_token(&user).unwrap();
        let claims = jwt_auth.verify_access_token(&token).unwrap();
        
        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.username, user.username);
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_generate_token_pair() {
        let config = JwtConfig::default();
        let jwt_auth = JwtAuth::new(config);
        let user = create_test_user();
        
        let token_pair = jwt_auth.generate_token_pair(&user).unwrap();
        
        assert!(!token_pair.access_token.is_empty());
        assert!(!token_pair.refresh_token.is_empty());
        assert_eq!(token_pair.token_type, "Bearer");
    }

    #[test]
    fn test_refresh_token() {
        let config = JwtConfig::default();
        let jwt_auth = JwtAuth::new(config);
        let user = create_test_user();
        
        let token_pair = jwt_auth.generate_token_pair(&user).unwrap();
        let new_token_pair = jwt_auth.refresh(&token_pair.refresh_token, &user).unwrap();
        
        assert_ne!(token_pair.access_token, new_token_pair.access_token);
    }

    #[test]
    fn test_invalid_token_type() {
        let config = JwtConfig::default();
        let jwt_auth = JwtAuth::new(config);
        let user = create_test_user();
        
        let refresh_token = jwt_auth.generate_refresh_token(&user).unwrap();
        
        // リフレッシュトークンをアクセストークンとして検証（失敗するべき）
        assert!(jwt_auth.verify_access_token(&refresh_token).is_err());
    }
}
