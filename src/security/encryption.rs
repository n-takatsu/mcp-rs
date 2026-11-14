//! 認証情報の暗号化・復号化機能
//! AES-GCM-256による安全な暗号化と、PBKDF2によるキー導出を実装

#![allow(deprecated)] // generic-array v1.x移行中の一時的対応

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt;

/// 暗号化された認証情報を管理する構造体
#[derive(Clone)]
pub struct SecureCredentials {
    pub username: String,
    password: SecretString,
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
    /// 鍵の導出の方式（#543）。今は`pbkdf2-sha256`だけ。省略時（v0.23.0
    /// までに作ったもの）は`pbkdf2-sha256`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kdf: Option<String>,
    /// 鍵の導出の回数（#543）。省略時（v0.23.0までに作ったもの）は100000
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kdf_iterations: Option<u32>,
}

/// 鍵の導出の方式の名前
pub const KDF_PBKDF2_SHA256: &str = "pbkdf2-sha256";

/// 新しく暗号化するときの、PBKDF2-HMAC-SHA256の回数（#543、OWASPの推奨値）。
/// テストでは、速さのために少なくする（回数は記録するので、読むときに困らない）
#[cfg(not(test))]
pub const DEFAULT_KDF_ITERATIONS: u32 = 600_000;
#[cfg(test)]
pub const DEFAULT_KDF_ITERATIONS: u32 = 1_000;

/// 回数を記録していないもの（v0.23.0までに作ったもの）の回数
pub const LEGACY_KDF_ITERATIONS: u32 = 100_000;

/// 読むときに受け付ける回数の上限。書き換えられた設定ファイルで、起動が
/// 終わらなくならないように
const MAX_KDF_ITERATIONS: u32 = 10_000_000;

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
            password: SecretString::new(password.into()),
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

        // 記録した方式と回数で鍵を導出する。記録が無いものは、v0.23.0までの
        // 回数で読む（#543）
        match encrypted.kdf.as_deref() {
            None | Some(KDF_PBKDF2_SHA256) => {}
            Some(other) => {
                return Err(EncryptionError::InvalidInput(format!(
                    "unsupported kdf '{other}' in encrypted_credentials (supported: {KDF_PBKDF2_SHA256})"
                )))
            }
        }
        let iterations = encrypted.kdf_iterations.unwrap_or(LEGACY_KDF_ITERATIONS);
        if iterations == 0 || iterations > MAX_KDF_ITERATIONS {
            return Err(EncryptionError::InvalidInput(format!(
                "kdf_iterations in encrypted_credentials must be between 1 and {MAX_KDF_ITERATIONS}"
            )));
        }
        let cipher = cipher_for(master_password, &salt, iterations);

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
        let decrypted = cipher
            .decrypt(nonce, encrypted_password.as_ref())
            .map_err(|e| EncryptionError::DecryptionFailed(e.to_string()))?;

        let password = String::from_utf8(decrypted)
            .map_err(|e| EncryptionError::DecryptionFailed(format!("UTF-8変換エラー: {}", e)))?;

        Ok(Self {
            username: encrypted.username.clone(),
            password: SecretString::new(password.into()),
        })
    }

    /// 認証情報を暗号化してEncryptedCredentialsを作成
    pub fn encrypt(&self, master_password: &str) -> Result<EncryptedCredentials, EncryptionError> {
        // ランダムソルトを生成（32バイト）
        let mut salt_bytes = [0u8; 32];
        use aes_gcm::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut salt_bytes);

        // キーを派生（方式と回数は、結果に記録する、#543）
        let cipher = cipher_for(master_password, &salt_bytes, DEFAULT_KDF_ITERATIONS);

        // AES-GCMで暗号化
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        let encrypted_password = cipher
            .encrypt(&nonce, self.password.expose_secret().as_bytes())
            .map_err(|e| EncryptionError::EncryptionFailed(e.to_string()))?;

        Ok(EncryptedCredentials {
            username: self.username.clone(),
            encrypted_password: general_purpose::STANDARD.encode(&encrypted_password),
            nonce: general_purpose::STANDARD.encode(nonce),
            salt: general_purpose::STANDARD.encode(salt_bytes),
            kdf: Some(KDF_PBKDF2_SHA256.to_string()),
            kdf_iterations: Some(DEFAULT_KDF_ITERATIONS),
        })
    }

    /// パスワードを取得（Secretのまま返す）
    pub fn get_password(&self) -> &SecretString {
        &self.password
    }

    /// Basic認証用のヘッダー値を生成
    pub fn to_basic_auth(&self) -> String {
        let credentials = format!("{}:{}", self.username, self.password.expose_secret());
        general_purpose::STANDARD.encode(credentials.as_bytes())
    }

    /// パスワードを変更
    pub fn change_password(&mut self, new_password: String) {
        self.password = SecretString::new(new_password.into());
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

/// PBKDF2-HMAC-SHA256で、パスワードから鍵を導出し、AES-GCMの暗号を作る。
/// 導出した鍵のバイト列は、使い終わったら0で上書きする（`Zeroizing`、#543）
fn cipher_for(password: &str, salt: &[u8], iterations: u32) -> Aes256Gcm {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;

    let mut key_bytes = zeroize::Zeroizing::new([0u8; 32]);
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, key_bytes.as_mut());
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key_bytes.as_ref()))
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

    /// 方式と回数を記録し、記録した回数で読む（#543）
    #[test]
    fn test_kdf_parameters_are_recorded_and_used() {
        let original = SecureCredentials::new("u".to_string(), "app pw".to_string());
        let encrypted = original.encrypt("master-password-123").unwrap();
        assert_eq!(encrypted.kdf.as_deref(), Some(KDF_PBKDF2_SHA256));
        assert_eq!(encrypted.kdf_iterations, Some(DEFAULT_KDF_ITERATIONS));
        // 設定ファイルに書き出すと、両方が入る
        let toml = toml::to_string(&encrypted).unwrap();
        assert!(toml.contains("kdf = \"pbkdf2-sha256\""), "{toml}");
        assert!(toml.contains("kdf_iterations = "), "{toml}");

        // 回数を書き換えると、別の鍵になって読めない
        let mut changed = encrypted.clone();
        changed.kdf_iterations = Some(DEFAULT_KDF_ITERATIONS + 1);
        assert!(SecureCredentials::from_encrypted(&changed, "master-password-123").is_err());

        // 知らない方式・範囲外の回数は、理由を示して止める
        let mut unknown = encrypted.clone();
        unknown.kdf = Some("argon2id".to_string());
        let err = SecureCredentials::from_encrypted(&unknown, "master-password-123")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unsupported kdf 'argon2id'"), "{err}");
        let mut zero = encrypted.clone();
        zero.kdf_iterations = Some(0);
        assert!(SecureCredentials::from_encrypted(&zero, "master-password-123").is_err());
        let mut huge = encrypted;
        huge.kdf_iterations = Some(u32::MAX);
        let err = SecureCredentials::from_encrypted(&huge, "master-password-123")
            .unwrap_err()
            .to_string();
        assert!(err.contains("kdf_iterations"), "{err}");
    }

    /// 記録の無いもの（v0.23.0までに作ったもの）は、10万回で読む（#543）
    #[test]
    fn test_legacy_credentials_use_100000_iterations() {
        let original = SecureCredentials::new("u".to_string(), "legacy pw".to_string());
        // v0.23.0までと同じく、10万回で作り、記録を消す
        let mut salt = [0u8; 32];
        use aes_gcm::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut salt);
        let cipher = cipher_for("master-password-123", &salt, LEGACY_KDF_ITERATIONS);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, original.get_password().expose_secret().as_bytes())
            .unwrap();
        let toml = format!(
            "username = \"u\"\nencrypted_password = \"{}\"\nnonce = \"{}\"\nsalt = \"{}\"\n",
            general_purpose::STANDARD.encode(&ciphertext),
            general_purpose::STANDARD.encode(nonce),
            general_purpose::STANDARD.encode(salt)
        );
        let legacy: EncryptedCredentials = toml::from_str(&toml).unwrap();
        assert_eq!(legacy.kdf, None);
        assert_eq!(legacy.kdf_iterations, None);
        let decrypted = SecureCredentials::from_encrypted(&legacy, "master-password-123").unwrap();
        assert_eq!(decrypted.get_password().expose_secret(), "legacy pw");
    }
}
