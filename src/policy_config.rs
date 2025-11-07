use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

use crate::error::McpError;

/// ポリシー設定の統一表現
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyConfig {
    /// ポリシーの一意識別子
    pub id: String,
    /// ポリシー名
    pub name: String,
    /// ポリシーバージョン
    pub version: String,
    /// ポリシーの説明
    pub description: Option<String>,
    /// 作成日時
    pub created_at: DateTime<Utc>,
    /// 最終更新日時
    pub updated_at: DateTime<Utc>,
    /// セキュリティ設定
    pub security: SecurityPolicyConfig,
    /// 監視設定
    pub monitoring: MonitoringPolicyConfig,
    /// 認証設定
    pub authentication: AuthenticationPolicyConfig,
    /// カスタム設定
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// セキュリティポリシー設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityPolicyConfig {
    /// セキュリティ機能の有効/無効
    pub enabled: bool,
    /// 暗号化アルゴリズム
    pub encryption: EncryptionConfig,
    /// TLS設定
    pub tls: TlsConfig,
    /// 入力検証設定
    pub input_validation: InputValidationConfig,
    /// レート制限設定
    pub rate_limiting: RateLimitingConfig,
}

/// 暗号化設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptionConfig {
    /// 暗号化アルゴリズム
    pub algorithm: String,
    /// キーサイズ（ビット）
    pub key_size: u32,
    /// PBKDF2反復回数
    #[serde(default = "default_pbkdf2_iterations")]
    pub pbkdf2_iterations: u32,
}

/// 監視ポリシー設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitoringPolicyConfig {
    /// 監視間隔（秒）
    pub interval_seconds: u64,
    /// アラート有効/無効
    pub alerts_enabled: bool,
    /// ログレベル
    pub log_level: String,
    /// メトリクス収集設定
    pub metrics: MetricsConfig,
}

/// メトリクス設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsConfig {
    /// メトリクス収集の有効/無効
    pub enabled: bool,
    /// サンプリングレート（0.0-1.0）
    pub sampling_rate: f64,
    /// バッファサイズ
    pub buffer_size: usize,
}

/// TLS設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TlsConfig {
    /// TLS強制の有効/無効
    pub enforce: bool,
    /// 最小TLSバージョン
    pub min_version: String,
    /// 許可する暗号スイート
    pub cipher_suites: Vec<String>,
}

/// 入力検証設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputValidationConfig {
    /// 入力検証の有効/無効
    pub enabled: bool,
    /// 最大入力長
    pub max_input_length: usize,
    /// SQLインジェクション保護
    pub sql_injection_protection: bool,
    /// XSS保護
    pub xss_protection: bool,
}

/// レート制限設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimitingConfig {
    /// レート制限の有効/無効
    pub enabled: bool,
    /// 1分あたりのリクエスト制限
    pub requests_per_minute: u32,
    /// バーストサイズ
    pub burst_size: u32,
}

/// 認証ポリシー設定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthenticationPolicyConfig {
    /// 認証の有効/無効
    pub enabled: bool,
    /// 認証方式
    pub method: String,
    /// セッションタイムアウト（秒）
    pub session_timeout_seconds: u64,
    /// MFA（多要素認証）の要求
    pub require_mfa: bool,
}

/// ポリシーローダー - 複数形式のファイルを統一的に読み込み
pub struct PolicyLoader;

impl PolicyLoader {
    /// ファイルからポリシー設定を読み込み
    pub async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<PolicyConfig, McpError> {
        let path = path.as_ref();
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| McpError::Config(format!("ポリシーファイルの読み込みに失敗: {}", e)))?;

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "toml" => Self::parse_toml(&content),
            "yaml" | "yml" => Self::parse_yaml(&content),
            "json" => Self::parse_json(&content),
            _ => Err(McpError::Config(format!(
                "サポートされていないファイル形式: {}",
                extension
            ))),
        }
    }

    /// TOML形式をパース
    fn parse_toml(content: &str) -> Result<PolicyConfig, McpError> {
        toml::from_str(content).map_err(|e| McpError::Config(format!("TOMLパースエラー: {}", e)))
    }

    /// YAML形式をパース
    fn parse_yaml(content: &str) -> Result<PolicyConfig, McpError> {
        serde_yaml_ng::from_str(content)
            .map_err(|e| McpError::Config(format!("YAMLパースエラー: {}", e)))
    }

    /// JSON形式をパース
    fn parse_json(content: &str) -> Result<PolicyConfig, McpError> {
        serde_json::from_str(content)
            .map_err(|e| McpError::Config(format!("JSONパースエラー: {}", e)))
    }

    /// ポリシー設定をファイルに保存
    pub async fn save_to_file<P: AsRef<Path>>(
        policy: &PolicyConfig,
        path: P,
    ) -> Result<(), McpError> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_lowercase();

        let content = match extension.as_str() {
            "toml" => toml::to_string_pretty(policy)
                .map_err(|e| McpError::Config(format!("TOMLシリアライズエラー: {}", e)))?,
            "yaml" | "yml" => serde_yaml_ng::to_string(policy)
                .map_err(|e| McpError::Config(format!("YAMLシリアライズエラー: {}", e)))?,
            "json" => serde_json::to_string_pretty(policy)
                .map_err(|e| McpError::Config(format!("JSONシリアライズエラー: {}", e)))?,
            _ => {
                return Err(McpError::Config(format!(
                    "サポートされていないファイル形式: {}",
                    extension
                )));
            }
        };

        tokio::fs::write(path, content)
            .await
            .map_err(|e| McpError::Config(format!("ポリシーファイルの保存に失敗: {}", e)))?;

        Ok(())
    }
}

impl Default for PolicyConfig {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Default Policy".to_string(),
            version: "1.0.0".to_string(),
            description: Some("デフォルトポリシー設定".to_string()),
            created_at: now,
            updated_at: now,
            security: SecurityPolicyConfig::default(),
            monitoring: MonitoringPolicyConfig::default(),
            authentication: AuthenticationPolicyConfig::default(),
            custom: HashMap::new(),
        }
    }
}

impl Default for SecurityPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            encryption: EncryptionConfig::default(),
            tls: TlsConfig::default(),
            input_validation: InputValidationConfig::default(),
            rate_limiting: RateLimitingConfig::default(),
        }
    }
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            algorithm: "AES-256-GCM".to_string(),
            key_size: 256,
            pbkdf2_iterations: default_pbkdf2_iterations(),
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enforce: true,
            min_version: "TLSv1.2".to_string(),
            cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
                "TLS_AES_128_GCM_SHA256".to_string(),
            ],
        }
    }
}

impl Default for InputValidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_input_length: 1024 * 1024, // 1MB
            sql_injection_protection: true,
            xss_protection: true,
        }
    }
}

impl Default for RateLimitingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 60,
            burst_size: 10,
        }
    }
}

impl Default for MonitoringPolicyConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 60,
            alerts_enabled: true,
            log_level: "info".to_string(),
            metrics: MetricsConfig::default(),
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_rate: 1.0,
            buffer_size: 1000,
        }
    }
}

impl Default for AuthenticationPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            method: "basic".to_string(),
            session_timeout_seconds: 3600, // 1時間
            require_mfa: false,
        }
    }
}

fn default_pbkdf2_iterations() -> u32 {
    #[cfg(test)]
    return 1_000; // テスト環境では高速化

    #[cfg(not(test))]
    100_000 // 本番環境では安全性重視
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_policy_config_default() {
        let policy = PolicyConfig::default();
        assert_eq!(policy.name, "Default Policy");
        assert_eq!(policy.version, "1.0.0");
        assert!(policy.security.enabled);
        assert!(policy.monitoring.alerts_enabled);
    }

    #[tokio::test]
    async fn test_policy_loader_json() {
        let temp_dir = TempDir::new().unwrap();
        let policy_file = temp_dir.path().join("test_policy.json");

        let policy = PolicyConfig::default();
        PolicyLoader::save_to_file(&policy, &policy_file)
            .await
            .unwrap();

        let loaded = PolicyLoader::load_from_file(&policy_file).await.unwrap();
        assert_eq!(policy.name, loaded.name);
        assert_eq!(
            policy.security.encryption.algorithm,
            loaded.security.encryption.algorithm
        );
    }

    #[tokio::test]
    async fn test_policy_loader_toml() {
        let temp_dir = TempDir::new().unwrap();
        let policy_file = temp_dir.path().join("test_policy.toml");

        let policy = PolicyConfig::default();
        PolicyLoader::save_to_file(&policy, &policy_file)
            .await
            .unwrap();

        let loaded = PolicyLoader::load_from_file(&policy_file).await.unwrap();
        assert_eq!(policy.name, loaded.name);
    }

    #[tokio::test]
    async fn test_policy_loader_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let policy_file = temp_dir.path().join("test_policy.yaml");

        let policy = PolicyConfig::default();
        PolicyLoader::save_to_file(&policy, &policy_file)
            .await
            .unwrap();

        let loaded = PolicyLoader::load_from_file(&policy_file).await.unwrap();
        assert_eq!(policy.name, loaded.name);
    }

    #[test]
    fn test_encryption_config_pbkdf2_iterations() {
        let config = EncryptionConfig::default();
        // テスト環境では1,000回
        assert_eq!(config.pbkdf2_iterations, 1_000);
    }
}
