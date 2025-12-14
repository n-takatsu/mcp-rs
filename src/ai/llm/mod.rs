//! Large Language Model Integration
//!
//! OpenAI、ローカルLLM（llama.cpp、candle）の統合

pub mod openai;

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// LLMプロバイダー種別
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LlmProvider {
    /// OpenAI (GPT-4, GPT-3.5)
    OpenAI {
        model: String,
        api_key: String,
    },
    /// ローカルLLM (llama.cpp)
    Local {
        model_path: String,
        context_size: usize,
    },
    /// Candle統合
    Candle {
        model_id: String,
        device: String,
    },
}

/// LLM設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// プロバイダー
    pub provider: LlmProvider,
    /// 最大トークン数
    pub max_tokens: usize,
    /// 温度パラメータ（0.0-2.0）
    pub temperature: f32,
    /// Top-pサンプリング
    pub top_p: f32,
    /// ストリーミング有効化
    pub streaming: bool,
    /// タイムアウト（秒）
    pub timeout_seconds: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::OpenAI {
                model: "gpt-4".to_string(),
                api_key: String::new(),
            },
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 1.0,
            streaming: false,
            timeout_seconds: 30,
        }
    }
}

/// LLMレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// 生成されたテキスト
    pub content: String,
    /// 使用トークン数
    pub tokens_used: usize,
    /// レスポンス時間（ミリ秒）
    pub response_time_ms: u64,
    /// モデル名
    pub model: String,
    /// 完了理由
    pub finish_reason: Option<String>,
}

/// LLMクライアントトレイト
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// テキスト生成
    async fn generate(&self, prompt: &str) -> Result<LlmResponse>;

    /// チャット形式でのテキスト生成
    async fn chat(&self, messages: &[ChatMessage]) -> Result<LlmResponse>;

    /// ストリーミング生成
    async fn generate_stream(
        &self,
        prompt: &str,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Unpin + Send>>;

    /// モデル情報取得
    fn model_info(&self) -> ModelInfo;

    /// ヘルスチェック
    async fn health_check(&self) -> Result<()>;
}

/// チャットメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// 役割（system, user, assistant）
    pub role: String,
    /// メッセージ内容
    pub content: String,
}

impl ChatMessage {
    /// システムメッセージ作成
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    /// ユーザーメッセージ作成
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    /// アシスタントメッセージ作成
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// モデル情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// モデル名
    pub name: String,
    /// プロバイダー
    pub provider: String,
    /// コンテキストウィンドウサイズ
    pub context_window: usize,
    /// 最大出力トークン数
    pub max_output_tokens: usize,
    /// コストパラメータ（$/1Kトークン）
    pub cost_per_1k_tokens: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_creation() {
        let system_msg = ChatMessage::system("You are a helpful assistant");
        assert_eq!(system_msg.role, "system");

        let user_msg = ChatMessage::user("Hello");
        assert_eq!(user_msg.role, "user");
        assert_eq!(user_msg.content, "Hello");

        let assistant_msg = ChatMessage::assistant("Hi there!");
        assert_eq!(assistant_msg.role, "assistant");
    }

    #[test]
    fn test_llm_config_default() {
        let config = LlmConfig::default();
        assert_eq!(config.max_tokens, 2048);
        assert_eq!(config.temperature, 0.7);
        assert!(!config.streaming);
    }

    #[test]
    fn test_llm_provider_serialization() {
        let provider = LlmProvider::OpenAI {
            model: "gpt-4".to_string(),
            api_key: "test-key".to_string(),
        };
        let json = serde_json::to_string(&provider).unwrap();
        assert!(json.contains("OpenAI"));
        assert!(json.contains("gpt-4"));
    }
}
