//! コンテナセキュリティ管理
//!
//! 最小権限、セキュリティプロファイル、シークレット管理を提供します。

use crate::docker_runtime::{DockerError, Result};
use base64::{engine::general_purpose, Engine as _};
use bollard::models::{DeviceRequest, HostConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// セキュリティレベル
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SecurityLevel {
    /// 最小権限（最も制限的）
    Minimal,
    /// 標準
    Standard,
    /// 拡張（追加機能が必要）
    Extended,
    /// カスタム
    Custom,
}

/// セキュリティプロファイル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityProfile {
    /// セキュリティレベル
    pub level: SecurityLevel,

    /// 読み取り専用ルートファイルシステム
    pub read_only_rootfs: bool,

    /// 特権モード無効化
    pub privileged: bool,

    /// ネットワークアクセス制限
    pub no_new_privileges: bool,

    /// Capabilities（Linux）
    pub cap_add: Vec<String>,
    pub cap_drop: Vec<String>,

    /// AppArmor プロファイル
    pub apparmor_profile: Option<String>,

    /// Seccomp プロファイル
    pub seccomp_profile: Option<String>,

    /// SELinux ラベル
    pub selinux_label: Option<String>,

    /// ユーザー/グループID
    pub user: Option<String>,

    /// 許可するデバイス
    pub devices: Vec<String>,
}

impl SecurityProfile {
    /// 最小権限プロファイルを作成
    pub fn minimal() -> Self {
        Self {
            level: SecurityLevel::Minimal,
            read_only_rootfs: true,
            privileged: false,
            no_new_privileges: true,
            cap_add: vec![],
            cap_drop: vec!["ALL".to_string()],
            apparmor_profile: Some("docker-default".to_string()),
            seccomp_profile: Some("runtime/default".to_string()),
            selinux_label: None,
            user: Some("nobody:nogroup".to_string()),
            devices: vec![],
        }
    }

    /// 標準プロファイルを作成
    pub fn standard() -> Self {
        Self {
            level: SecurityLevel::Standard,
            read_only_rootfs: false,
            privileged: false,
            no_new_privileges: true,
            cap_add: vec![],
            cap_drop: vec![
                "AUDIT_WRITE".to_string(),
                "MKNOD".to_string(),
                "NET_RAW".to_string(),
                "SETFCAP".to_string(),
            ],
            apparmor_profile: Some("docker-default".to_string()),
            seccomp_profile: Some("runtime/default".to_string()),
            selinux_label: None,
            user: Some("1000:1000".to_string()),
            devices: vec![],
        }
    }

    /// DockerのHostConfigに適用
    pub fn apply_to_host_config(&self, mut host_config: HostConfig) -> HostConfig {
        host_config.readonly_rootfs = Some(self.read_only_rootfs);
        host_config.privileged = Some(self.privileged);
        host_config.cap_add = Some(self.cap_add.clone());
        host_config.cap_drop = Some(self.cap_drop.clone());
        host_config.security_opt = self.build_security_opts();

        host_config
    }

    /// セキュリティオプションを構築
    fn build_security_opts(&self) -> Option<Vec<String>> {
        let mut opts = Vec::new();

        if self.no_new_privileges {
            opts.push("no-new-privileges:true".to_string());
        }

        if let Some(ref profile) = self.apparmor_profile {
            opts.push(format!("apparmor={}", profile));
        }

        if let Some(ref profile) = self.seccomp_profile {
            opts.push(format!("seccomp={}", profile));
        }

        if let Some(ref label) = self.selinux_label {
            opts.push(format!("label={}", label));
        }

        if opts.is_empty() {
            None
        } else {
            Some(opts)
        }
    }
}

impl Default for SecurityProfile {
    fn default() -> Self {
        Self::standard()
    }
}

/// シークレット（機密情報）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    /// シークレット名
    pub name: String,

    /// 暗号化された値
    pub encrypted_value: String,

    /// 作成日時
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// 最終更新日時
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// シークレット管理
pub struct SecretManager {
    /// シークレット保管（暗号化済み）
    secrets: Arc<RwLock<HashMap<String, Secret>>>,

    /// 暗号化キー（実際にはより安全な管理が必要）
    encryption_key: Vec<u8>,
}

impl SecretManager {
    /// 新しいSecretManagerを作成
    pub fn new(encryption_key: Vec<u8>) -> Self {
        Self {
            secrets: Arc::new(RwLock::new(HashMap::new())),
            encryption_key,
        }
    }

    /// シークレットを追加（暗号化して保存）
    pub async fn add_secret(&self, name: String, value: String) -> Result<()> {
        let encrypted = self.encrypt(&value)?;

        let secret = Secret {
            name: name.clone(),
            encrypted_value: encrypted,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let mut secrets = self.secrets.write().await;
        secrets.insert(name, secret);

        tracing::info!("Secret added successfully");
        Ok(())
    }

    /// シークレットを取得（復号化して返す）
    pub async fn get_secret(&self, name: &str) -> Result<String> {
        let secrets = self.secrets.read().await;

        let secret = secrets
            .get(name)
            .ok_or_else(|| DockerError::ApiError(format!("Secret {} not found", name)))?;

        self.decrypt(&secret.encrypted_value)
    }

    /// シークレットを削除
    pub async fn remove_secret(&self, name: &str) -> Result<()> {
        let mut secrets = self.secrets.write().await;

        secrets
            .remove(name)
            .ok_or_else(|| DockerError::ApiError(format!("Secret {} not found", name)))?;

        tracing::info!("Secret removed successfully");
        Ok(())
    }

    /// すべてのシークレット名をリスト
    pub async fn list_secrets(&self) -> Vec<String> {
        let secrets = self.secrets.read().await;
        secrets.keys().cloned().collect()
    }

    /// 暗号化（AES-GCM）
    fn encrypt(&self, plaintext: &str) -> Result<String> {
        use aes_gcm::{
            aead::{Aead, KeyInit, OsRng},
            Aes256Gcm, Nonce,
        };
        use rand::RngCore;

        // 暗号化キーの準備
        let key = if self.encryption_key.len() == 32 {
            self.encryption_key.clone()
        } else {
            // キーを32バイトに調整
            let mut padded = vec![0u8; 32];
            let len = self.encryption_key.len().min(32);
            padded[..len].copy_from_slice(&self.encryption_key[..len]);
            padded
        };

        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| DockerError::ApiError(format!("Encryption error: {}", e)))?;

        // ランダムなnonceを生成
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // 暗号化
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| DockerError::ApiError(format!("Encryption failed: {}", e)))?;

        // nonce + ciphertextをbase64エンコード
        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ciphertext);

        Ok(general_purpose::STANDARD.encode(&combined))
    }

    /// 復号化（AES-GCM）
    fn decrypt(&self, encrypted: &str) -> Result<String> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };

        // base64デコード
        let combined = general_purpose::STANDARD
            .decode(encrypted)
            .map_err(|e| DockerError::ApiError(format!("Decoding error: {}", e)))?;

        if combined.len() < 12 {
            return Err(DockerError::ApiError("Invalid encrypted data".to_string()));
        }

        // nonce と ciphertext を分離
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        // 復号化キーの準備
        let key = if self.encryption_key.len() == 32 {
            self.encryption_key.clone()
        } else {
            let mut padded = vec![0u8; 32];
            let len = self.encryption_key.len().min(32);
            padded[..len].copy_from_slice(&self.encryption_key[..len]);
            padded
        };

        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| DockerError::ApiError(format!("Decryption error: {}", e)))?;

        // 復号化
        let plaintext = cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|e| DockerError::ApiError(format!("Decryption failed: {}", e)))?;

        String::from_utf8(plaintext)
            .map_err(|e| DockerError::ApiError(format!("UTF-8 conversion error: {}", e)))
    }
}

/// セキュリティマネージャー
pub struct SecurityManager {
    /// デフォルトプロファイル
    default_profile: SecurityProfile,

    /// カスタムプロファイル
    custom_profiles: Arc<RwLock<HashMap<String, SecurityProfile>>>,

    /// シークレット管理
    secret_manager: SecretManager,
}

impl SecurityManager {
    /// 新しいSecurityManagerを作成
    pub fn new(encryption_key: Vec<u8>) -> Self {
        Self {
            default_profile: SecurityProfile::standard(),
            custom_profiles: Arc::new(RwLock::new(HashMap::new())),
            secret_manager: SecretManager::new(encryption_key),
        }
    }

    /// デフォルトプロファイルを設定
    pub fn set_default_profile(&mut self, profile: SecurityProfile) {
        self.default_profile = profile;
    }

    /// デフォルトプロファイルを取得
    pub fn get_default_profile(&self) -> &SecurityProfile {
        &self.default_profile
    }

    /// カスタムプロファイルを追加
    pub async fn add_profile(&self, name: String, profile: SecurityProfile) {
        let mut profiles = self.custom_profiles.write().await;
        profiles.insert(name, profile);
    }

    /// プロファイルを取得
    pub async fn get_profile(&self, name: &str) -> Option<SecurityProfile> {
        let profiles = self.custom_profiles.read().await;
        profiles.get(name).cloned()
    }

    /// シークレットマネージャーへのアクセス
    pub fn secrets(&self) -> &SecretManager {
        &self.secret_manager
    }

    /// セキュリティ違反をチェック
    pub fn validate_config(&self, profile: &SecurityProfile) -> Result<()> {
        // 特権モードは禁止
        if profile.privileged {
            return Err(DockerError::SecurityViolation(
                "Privileged mode is not allowed".to_string(),
            ));
        }

        // すべてのCapabilityを追加することは禁止
        if profile.cap_add.iter().any(|c| c.to_uppercase() == "ALL") {
            return Err(DockerError::SecurityViolation(
                "Adding ALL capabilities is not allowed".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_profile_minimal() {
        let profile = SecurityProfile::minimal();
        assert_eq!(profile.level, SecurityLevel::Minimal);
        assert!(profile.read_only_rootfs);
        assert!(!profile.privileged);
        assert!(profile.no_new_privileges);
    }

    #[test]
    fn test_security_profile_standard() {
        let profile = SecurityProfile::standard();
        assert_eq!(profile.level, SecurityLevel::Standard);
        assert!(!profile.privileged);
        assert!(profile.no_new_privileges);
    }

    #[tokio::test]
    async fn test_secret_manager() {
        let key = b"test-encryption-key-32-bytes!!!".to_vec();
        let manager = SecretManager::new(key);

        // シークレット追加
        manager
            .add_secret("test".to_string(), "secret_value".to_string())
            .await
            .unwrap();

        // シークレット取得
        let value = manager.get_secret("test").await.unwrap();
        assert_eq!(value, "secret_value");

        // リスト確認
        let secrets = manager.list_secrets().await;
        assert!(secrets.contains(&"test".to_string()));

        // シークレット削除
        manager.remove_secret("test").await.unwrap();
        assert!(manager.get_secret("test").await.is_err());
    }

    #[tokio::test]
    async fn test_security_manager() {
        let key = b"test-encryption-key-32-bytes!!!".to_vec();
        let manager = SecurityManager::new(key);

        // デフォルトプロファイル
        let default = manager.get_default_profile();
        assert_eq!(default.level, SecurityLevel::Standard);

        // カスタムプロファイル追加
        let custom = SecurityProfile::minimal();
        manager.add_profile("custom".to_string(), custom).await;

        // カスタムプロファイル取得
        let retrieved = manager.get_profile("custom").await.unwrap();
        assert_eq!(retrieved.level, SecurityLevel::Minimal);
    }

    #[test]
    fn test_validate_config() {
        let key = b"test-encryption-key-32-bytes!!!".to_vec();
        let manager = SecurityManager::new(key);

        // 安全な設定
        let safe_profile = SecurityProfile::standard();
        assert!(manager.validate_config(&safe_profile).is_ok());

        // 危険な設定（特権モード）
        let mut dangerous = SecurityProfile::standard();
        dangerous.privileged = true;
        assert!(manager.validate_config(&dangerous).is_err());

        // 危険な設定（ALL capabilities）
        let mut dangerous2 = SecurityProfile::standard();
        dangerous2.cap_add = vec!["ALL".to_string()];
        assert!(manager.validate_config(&dangerous2).is_err());
    }
}
