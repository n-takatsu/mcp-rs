// API Key Authentication Implementation

use super::types::{AuthError, AuthResult, Permission, Role};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// APIキー設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// キーの長さ
    #[serde(default = "default_key_length")]
    pub key_length: usize,
    
    /// キープレフィックス
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
    
    /// デフォルトの有効期限（秒、Noneで無期限）
    #[serde(default)]
    pub default_expiration: Option<u64>,
    
    /// レート制限（秒あたりのリクエスト数）
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,
}

fn default_key_length() -> usize {
    32
}

fn default_key_prefix() -> String {
    "mcp".to_string()
}

fn default_rate_limit() -> u32 {
    100
}

impl Default for ApiKeyConfig {
    fn default() -> Self {
        Self {
            key_length: default_key_length(),
            key_prefix: default_key_prefix(),
            default_expiration: None,
            rate_limit: default_rate_limit(),
        }
    }
}

/// APIキーパーミッション
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ApiKeyPermission {
    /// リソース
    pub resource: String,
    /// アクション（ソート済み）
    pub actions: Vec<String>,
}

impl ApiKeyPermission {
    pub fn new(resource: String) -> Self {
        Self {
            resource,
            actions: Vec::new(),
        }
    }
    
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        let action = action.into();
        if !self.actions.contains(&action) {
            self.actions.push(action);
            self.actions.sort();
        }
        self
    }
    
    pub fn with_actions(mut self, actions: Vec<String>) -> Self {
        for action in actions {
            if !self.actions.contains(&action) {
                self.actions.push(action);
            }
        }
        self.actions.sort();
        self
    }
    
    pub fn all() -> Self {
        Self {
            resource: "*".to_string(),
            actions: vec!["*".to_string()],
        }
    }
}

/// APIキー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// キーID
    pub id: String,
    
    /// キー値（ハッシュ化）
    #[serde(skip_serializing)]
    pub key_hash: String,
    
    /// 名前/説明
    pub name: String,
    
    /// ユーザーID
    pub user_id: String,
    
    /// パーミッション
    pub permissions: HashSet<ApiKeyPermission>,
    
    /// ロール
    pub roles: HashSet<Role>,
    
    /// 作成日時
    pub created_at: u64,
    
    /// 有効期限（UNIXタイムスタンプ、Noneで無期限）
    pub expires_at: Option<u64>,
    
    /// 最終使用日時
    pub last_used_at: Option<u64>,
    
    /// 使用回数
    pub usage_count: u64,
    
    /// 有効/無効
    pub enabled: bool,
    
    /// メタデータ
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl ApiKey {
    pub fn new(
        key_hash: String,
        name: String,
        user_id: String,
        expires_at: Option<u64>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            id: Uuid::new_v4().to_string(),
            key_hash,
            name,
            user_id,
            permissions: HashSet::new(),
            roles: HashSet::new(),
            created_at: now,
            expires_at,
            last_used_at: None,
            usage_count: 0,
            enabled: true,
            metadata: HashMap::new(),
        }
    }
    
    /// キーが有効か確認
    pub fn is_valid(&self) -> bool {
        if !self.enabled {
            return false;
        }
        
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now > expires_at {
                return false;
            }
        }
        
        true
    }
    
    /// パーミッションを確認
    pub fn has_permission(&self, resource: &str, action: &str) -> bool {
        // 管理者ロールは全て許可
        if self.roles.iter().any(|r| r.is_admin()) {
            return true;
        }
        
        // ワイルドカード許可
        if self.permissions.iter().any(|p| {
            (p.resource == "*" || p.resource == resource)
                && (p.actions.contains(&"*".to_string()) || p.actions.contains(&action.to_string()))
        }) {
            return true;
        }
        
        false
    }
    
    /// 使用を記録
    pub fn record_usage(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_used_at = Some(now);
        self.usage_count += 1;
    }
}

/// APIキーマネージャー
pub struct ApiKeyManager {
    config: ApiKeyConfig,
    keys: HashMap<String, ApiKey>,
}

impl ApiKeyManager {
    pub fn new(config: ApiKeyConfig) -> Self {
        Self {
            config,
            keys: HashMap::new(),
        }
    }
    
    /// 新しいAPIキーを生成
    pub fn generate_key(
        &mut self,
        name: String,
        user_id: String,
        expires_in: Option<u64>,
    ) -> AuthResult<(String, ApiKey)> {
        // ランダムキーを生成
        let raw_key = self.generate_random_key();
        let full_key = format!("{}_{}", self.config.key_prefix, raw_key);
        
        // キーをハッシュ化
        let key_hash = self.hash_key(&full_key)?;
        
        // 有効期限を計算
        let expires_at = expires_in.or(self.config.default_expiration).map(|exp| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + exp
        });
        
        // APIキーオブジェクトを作成
        let api_key = ApiKey::new(key_hash.clone(), name, user_id, expires_at);
        
        // 保存
        self.keys.insert(key_hash, api_key.clone());
        
        Ok((full_key, api_key))
    }
    
    /// APIキーを検証
    pub fn verify_key(&mut self, key: &str) -> AuthResult<&mut ApiKey> {
        let key_hash = self.hash_key(key)?;
        
        let api_key = self
            .keys
            .get_mut(&key_hash)
            .ok_or(AuthError::InvalidCredentials)?;
        
        if !api_key.is_valid() {
            return Err(AuthError::TokenExpired);
        }
        
        api_key.record_usage();
        Ok(api_key)
    }
    
    /// APIキーを無効化
    pub fn revoke_key(&mut self, key: &str) -> AuthResult<()> {
        let key_hash = self.hash_key(key)?;
        
        let api_key = self
            .keys
            .get_mut(&key_hash)
            .ok_or(AuthError::InvalidCredentials)?;
        
        api_key.enabled = false;
        Ok(())
    }
    
    /// ユーザーのAPIキー一覧を取得
    pub fn list_user_keys(&self, user_id: &str) -> Vec<&ApiKey> {
        self.keys
            .values()
            .filter(|k| k.user_id == user_id)
            .collect()
    }
    
    /// APIキーにパーミッションを追加
    pub fn add_permission(
        &mut self,
        key: &str,
        permission: ApiKeyPermission,
    ) -> AuthResult<()> {
        let key_hash = self.hash_key(key)?;
        
        let api_key = self
            .keys
            .get_mut(&key_hash)
            .ok_or(AuthError::InvalidCredentials)?;
        
        api_key.permissions.insert(permission);
        Ok(())
    }
    
    /// APIキーにロールを追加
    pub fn add_role(&mut self, key: &str, role: Role) -> AuthResult<()> {
        let key_hash = self.hash_key(key)?;
        
        let api_key = self
            .keys
            .get_mut(&key_hash)
            .ok_or(AuthError::InvalidCredentials)?;
        
        api_key.roles.insert(role);
        Ok(())
    }
    
    /// ランダムキーを生成
    fn generate_random_key(&self) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        
        let mut rng = rand::thread_rng();
        (0..self.config.key_length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
    
    /// キーをハッシュ化
    fn hash_key(&self, key: &str) -> AuthResult<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_config_default() {
        let config = ApiKeyConfig::default();
        assert_eq!(config.key_length, 32);
        assert_eq!(config.key_prefix, "mcp");
    }

    #[test]
    fn test_generate_api_key() {
        let config = ApiKeyConfig::default();
        let mut manager = ApiKeyManager::new(config);
        
        let (key, api_key) = manager
            .generate_key("test-key".to_string(), "user-1".to_string(), None)
            .unwrap();
        
        assert!(key.starts_with("mcp_"));
        assert_eq!(api_key.name, "test-key");
        assert_eq!(api_key.user_id, "user-1");
        assert!(api_key.is_valid());
    }

    #[test]
    fn test_verify_api_key() {
        let config = ApiKeyConfig::default();
        let mut manager = ApiKeyManager::new(config);
        
        let (key, _) = manager
            .generate_key("test-key".to_string(), "user-1".to_string(), None)
            .unwrap();
        
        let verified = manager.verify_key(&key).unwrap();
        assert_eq!(verified.name, "test-key");
        assert_eq!(verified.usage_count, 1);
    }

    #[test]
    fn test_revoke_api_key() {
        let config = ApiKeyConfig::default();
        let mut manager = ApiKeyManager::new(config);
        
        let (key, _) = manager
            .generate_key("test-key".to_string(), "user-1".to_string(), None)
            .unwrap();
        
        manager.revoke_key(&key).unwrap();
        assert!(manager.verify_key(&key).is_err());
    }

    #[test]
    fn test_api_key_permissions() {
        let config = ApiKeyConfig::default();
        let mut manager = ApiKeyManager::new(config);
        
        let (key, _) = manager
            .generate_key("test-key".to_string(), "user-1".to_string(), None)
            .unwrap();
        
        let permission = ApiKeyPermission::new("posts".to_string())
            .with_action("read")
            .with_action("write");
        
        manager.add_permission(&key, permission).unwrap();
        
        let api_key = manager.verify_key(&key).unwrap();
        assert!(api_key.has_permission("posts", "read"));
        assert!(api_key.has_permission("posts", "write"));
        assert!(!api_key.has_permission("posts", "delete"));
    }

    #[test]
    fn test_api_key_expiration() {
        let config = ApiKeyConfig::default();
        let mut manager = ApiKeyManager::new(config);
        
        // 1秒で期限切れ
        let (key, _) = manager
            .generate_key("test-key".to_string(), "user-1".to_string(), Some(1))
            .unwrap();
        
        // すぐに検証できる
        assert!(manager.verify_key(&key).is_ok());
        
        // 2秒待つ
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        // 期限切れで失敗
        assert!(manager.verify_key(&key).is_err());
    }
}
