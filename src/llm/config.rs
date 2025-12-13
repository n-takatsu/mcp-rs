//! LLM統合の設定

use crate::llm::error::{LlmError, LlmResult};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// LLMプロバイダー
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// OpenAI (GPT-3.5, GPT-4など)
    OpenAI,
    /// Azure OpenAI
    AzureOpenAI,
    /// ローカルLLM (llama.cppなど)
    Local,
    /// カスタムエンドポイント
    Custom,
}

/// LLM設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// プロバイダー
    pub provider: LlmProvider,
    /// APIキー（セキュア）
    #[serde(skip_serializing)]
    pub api_key: Option<SecretString>,
    /// APIエンドポイント（カスタム時）
    pub endpoint: Option<String>,
    /// デフォルトモデル
    pub default_model: String,
    /// リクエストタイムアウト（秒）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// 最大リトライ回数
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// デフォルト温度
    #[serde(default = "default_temperature")]
    pub default_temperature: f32,
    /// デフォルト最大トークン数
    #[serde(default = "default_max_tokens")]
    pub default_max_tokens: usize,
    /// 組織ID（OpenAI用、オプション）
    pub organization_id: Option<String>,
}

fn default_timeout() -> u64 {
    60
}

fn default_max_retries() -> u32 {
    3
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> usize {
    2048
}

impl LlmConfig {
    /// OpenAI設定を作成
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LlmProvider::OpenAI,
            api_key: Some(SecretString::new(api_key.into().into_boxed_str())),
            endpoint: None,
            default_model: model.into(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            default_temperature: default_temperature(),
            default_max_tokens: default_max_tokens(),
            organization_id: None,
        }
    }

    /// 環境変数からOpenAI設定を読み込み
    pub fn openai_from_env() -> LlmResult<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| LlmError::ConfigError("OPENAI_API_KEY not set".to_string()))?;

        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-3.5-turbo".to_string());

        Ok(Self::openai(api_key, model))
    }

    /// Azure OpenAI設定を作成
    pub fn azure_openai(
        api_key: impl Into<String>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            provider: LlmProvider::AzureOpenAI,
            api_key: Some(SecretString::new(api_key.into().into_boxed_str())),
            endpoint: Some(endpoint.into()),
            default_model: model.into(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            default_temperature: default_temperature(),
            default_max_tokens: default_max_tokens(),
            organization_id: None,
        }
    }

    /// ローカルLLM設定を作成
    pub fn local(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LlmProvider::Local,
            api_key: None,
            endpoint: Some(endpoint.into()),
            default_model: model.into(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            default_temperature: default_temperature(),
            default_max_tokens: default_max_tokens(),
            organization_id: None,
        }
    }

    /// タイムアウトを取得
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    /// APIキーを取得（露出）
    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_ref().map(|k| k.expose_secret())
    }

    /// 設定を検証
    pub fn validate(&self) -> LlmResult<()> {
        // プロバイダー固有の検証
        match self.provider {
            LlmProvider::OpenAI | LlmProvider::AzureOpenAI => {
                if self.api_key.is_none() {
                    return Err(LlmError::ConfigError(
                        "API key is required for OpenAI providers".to_string(),
                    ));
                }
            }
            LlmProvider::Local | LlmProvider::Custom => {
                if self.endpoint.is_none() {
                    return Err(LlmError::ConfigError(
                        "Endpoint is required for local/custom providers".to_string(),
                    ));
                }
            }
        }

        // 温度の範囲チェック
        if !(0.0..=2.0).contains(&self.default_temperature) {
            return Err(LlmError::ConfigError(
                "Temperature must be between 0.0 and 2.0".to_string(),
            ));
        }

        // max_tokensの妥当性チェック
        if self.default_max_tokens == 0 || self.default_max_tokens > 100_000 {
            return Err(LlmError::ConfigError(
                "max_tokens must be between 1 and 100000".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::OpenAI,
            api_key: None,
            endpoint: None,
            default_model: "gpt-3.5-turbo".to_string(),
            timeout_secs: default_timeout(),
            max_retries: default_max_retries(),
            default_temperature: default_temperature(),
            default_max_tokens: default_max_tokens(),
            organization_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_config() {
        let config = LlmConfig::openai("test-key", "gpt-4");
        assert_eq!(config.provider, LlmProvider::OpenAI);
        assert_eq!(config.default_model, "gpt-4");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_invalid_temperature() {
        let mut config = LlmConfig::openai("test-key", "gpt-4");
        config.default_temperature = 3.0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_invalid_max_tokens() {
        let mut config = LlmConfig::openai("test-key", "gpt-4");
        config.default_max_tokens = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_local_config() {
        let config = LlmConfig::local("http://localhost:8080", "llama-2");
        assert_eq!(config.provider, LlmProvider::Local);
        assert!(config.api_key.is_none());
        assert!(config.validate().is_ok());
    }
}
