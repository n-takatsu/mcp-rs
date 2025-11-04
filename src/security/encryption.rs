//! 認証情報の暗号化・復号化機能
//! AES-GCM-256による安全な暗号化と、PBKDF2によるキー導出を実装

#![allow(deprecated)] // generic-array v1.x移行中の一時的対応

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use std::fmt;

/// 暗号化された認証情報を管理する構造体
#[derive(Clone)]
pub struct SecureCredentials {
    pub username: String,
    password: Secret<String>,
}

/// 設定ファイル用の暗号化された認証情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedCredentials {
    /// ユーザー名（平文）
    pub username: String,
    /// 暗号化されたパスワード（Base64エンコード）
    pub encrypted_password: String,
    /// 暗号化用のノンス（Base64エンコード）
    pub nonce: String,
    /// パスワード派生用のソルト（Base64エンコード）
    pub salt: String,
}

/// 暗号化エラー
#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("暗号化に失敗しました: {0}")]
    EncryptionFailed(String),
    #[error("復号化に失敗しました: {0}")]
    DecryptionFailed(String),
    #[error("キー派生に失敗しました: {0}")]
    KeyDerivationFailed(String),
    #[error("Base64デコードに失敗しました: {0}")]
    Base64DecodeError(String),
    #[error("無効な入力です: {0}")]
    InvalidInput(String),
}

impl SecureCredentials {
    /// 平文の認証情報から新しいSecureCredentialsを作成
    pub fn new(username: String, password: String) -> Self {
        Self {
            username,
            password: Secret::new(password),
        }
    }

    /// 暗号化された認証情報から復号化してSecureCredentialsを作成
    pub fn from_encrypted(
        encrypted: &EncryptedCredentials,
        master_password: &str,
    ) -> Result<Self, EncryptionError> {
        // ソルトをデコード
        let salt = general_purpose::STANDARD
            .decode(&encrypted.salt)
            .map_err(|e| EncryptionError::Base64DecodeError(e.to_string()))?;

        // キーを派生
        let key = derive_key(master_password, &salt)?;

        // 暗号化データとノンスをデコード
        let encrypted_password = general_purpose::STANDARD
            .decode(&encrypted.encrypted_password)
            .map_err(|e| EncryptionError::Base64DecodeError(e.to_string()))?;

        let nonce_bytes = general_purpose::STANDARD
            .decode(&encrypted.nonce)
            .map_err(|e| EncryptionError::Base64DecodeError(e.to_string()))?;

        if nonce_bytes.len() != 12 {
            return Err(EncryptionError::InvalidInput(
                "無効なノンスサイズです".to_string(),
            ));
        }

        let nonce = Nonce::from_slice(&nonce_bytes);

        // AES-GCMで復号化
        let cipher = Aes256Gcm::new(&key);
        let decrypted = cipher
            .decrypt(nonce, encrypted_password.as_ref())
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;

        let password = String::from_utf8(decrypted)
            .map_err(|e| EncryptionError::DecryptionFailed(format!("UTF-8変換エラー: {}", e)))?;

        Ok(Self {
            username: encrypted.username.clone(),
            password: Secret::new(password),
        })
    }

    /// 認証情報を暗号化してEncryptedCredentialsを作成
    pub fn encrypt(&self, master_password: &str) -> Result<EncryptedCredentials, EncryptionError> {
        // ランダムソルトを生成（32バイト）
        let mut salt_bytes = [0u8; 32];
        use aes_gcm::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut salt_bytes);

        // キーを派生
        let key = derive_key(master_password, &salt_bytes)?;

        // AES-GCMで暗号化
        let cipher = Aes256Gcm::new(&key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        let encrypted_password = cipher
            .encrypt(&nonce, self.password.expose_secret().as_bytes())
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        Ok(EncryptedCredentials {
            username: self.username.clone(),
            encrypted_password: general_purpose::STANDARD.encode(&encrypted_password),
            nonce: general_purpose::STANDARD.encode(nonce),
            salt: general_purpose::STANDARD.encode(salt_bytes),
        })
    }

    /// パスワードを取得（Secretのまま返す）
    pub fn get_password(&self) -> &Secret<String> {
        &self.password
    }

    /// Basic認証用のヘッダー値を生成
    pub fn to_basic_auth(&self) -> String {
        let credentials = format!("{}:{}", self.username, self.password.expose_secret());
        general_purpose::STANDARD.encode(credentials.as_bytes())
    }

    /// パスワードを変更
    pub fn change_password(&mut self, new_password: String) {
        self.password = Secret::new(new_password);
    }
}

// Debugトレイトでパスワードを露出しないようにカスタム実装
impl fmt::Debug for SecureCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureCredentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for SecureCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SecureCredentials {{ username: {}, password: [REDACTED] }}",
            self.username
        )
    }
}

/// PBKDF2を使用してパスワードからキーを派生
fn derive_key(password: &str, salt: &[u8]) -> Result<Key<Aes256Gcm>, EncryptionError> {
    use pbkdf2::pbkdf2_hmac_array;
    use sha2::Sha256;

    // テスト環境では反復回数を減らして高速化
    #[cfg(test)]
    const ITERATIONS: u32 = 1_000; // テスト用: 1,000回
    #[cfg(not(test))]
    const ITERATIONS: u32 = 100_000; // 本番用: 100,000回

    // PBKDF2-HMAC-SHA256を使用してキーを導出
    let key_bytes: [u8; 32] =
        pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), salt, ITERATIONS);

    Ok(*Key::<Aes256Gcm>::from_slice(&key_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_credentials_creation() {
        let creds = SecureCredentials::new("testuser".to_string(), "testpass".to_string());
        assert_eq!(creds.username, "testuser");
        assert_eq!(creds.password.expose_secret(), "testpass");
    }

    #[test]
    fn test_encryption_roundtrip() {
        let original =
            SecureCredentials::new("user123".to_string(), "secret_password_123".to_string());
        let master_password = "master_key_for_encryption";

        // 暗号化
        let encrypted = original.encrypt(master_password).expect("暗号化に失敗");

        // 復号化
        let decrypted =
            SecureCredentials::from_encrypted(&encrypted, master_password).expect("復号化に失敗");

        assert_eq!(original.username, decrypted.username);
        assert_eq!(
            original.password.expose_secret(),
            decrypted.password.expose_secret()
        );
    }

    #[test]
    fn test_wrong_master_password() {
        let original =
            SecureCredentials::new("user123".to_string(), "secret_password_123".to_string());
        let correct_password = "correct_master_password";
        let wrong_password = "wrong_master_password";

        let encrypted = original.encrypt(correct_password).expect("暗号化に失敗");

        // 間違ったパスワードで復号化を試行
        let result = SecureCredentials::from_encrypted(&encrypted, wrong_password);
        assert!(result.is_err());
    }

    #[test]
    fn test_basic_auth_generation() {
        let creds = SecureCredentials::new("admin".to_string(), "password123".to_string());
        let basic_auth = creds.to_basic_auth();

        // "admin:password123" をbase64エンコードした結果
        let expected = general_purpose::STANDARD.encode("admin:password123");
        assert_eq!(basic_auth, expected);
    }

    #[test]
    fn test_password_update() {
        let mut creds = SecureCredentials::new("user".to_string(), "old_password".to_string());
        creds.change_password("new_password".to_string());
        assert_eq!(creds.password.expose_secret(), "new_password");
    }

    #[test]
    fn test_display_does_not_leak_password() {
        let creds = SecureCredentials::new("testuser".to_string(), "secret".to_string());
        let display_string = format!("{}", creds);
        assert!(display_string.contains("testuser"));
        assert!(display_string.contains("[REDACTED]"));
        assert!(!display_string.contains("secret"));
    }

    #[test]
    fn test_encryption_produces_different_results() {
        let creds = SecureCredentials::new("user".to_string(), "password".to_string());
        let master_password = "master";

        let encrypted1 = creds.encrypt(master_password).expect("暗号化1に失敗");
        let encrypted2 = creds.encrypt(master_password).expect("暗号化2に失敗");

        // 同じデータでも異なる暗号化結果（ランダムソルト・ノンスのため）
        assert_ne!(encrypted1.encrypted_password, encrypted2.encrypted_password);
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.salt, encrypted2.salt);
    }
}
