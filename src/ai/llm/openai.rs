//! OpenAI API Integration
//!
//! GPT-4、GPT-3.5統合とストリーミングサポート

use super::{ChatMessage, LlmClient, LlmResponse, ModelInfo};
use crate::error::{Error, Result};
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::time::{Duration, Instant};

/// OpenAI APIクライアント
#[derive(Debug, Clone)]
pub struct OpenAiClient {
    /// APIキー
    api_key: String,
    /// モデル名
    model: String,
    /// 最大トークン数
    max_tokens: usize,
    /// 温度パラメータ
    temperature: f32,
    /// Top-pサンプリング
    top_p: f32,
    /// HTTPクライアント
    client: Client,
    /// APIベースURL
    base_url: String,
}

impl OpenAiClient {
    /// 新しいOpenAIクライアントを作成
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 1.0,
            client: Client::new(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    /// ビルダーパターン: 最大トークン数設定
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// ビルダーパターン: 温度設定
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// ビルダーパターン: Top-p設定
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p;
        self
    }

    /// ビルダーパターン: カスタムベースURL
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// チャット完了APIリクエスト
    async fn create_chat_completion(
        &self,
        messages: Vec<ChatMessage>,
        stream: bool,
    ) -> Result<reqwest::Response> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            max_tokens: Some(self.max_tokens),
            temperature: Some(self.temperature),
            top_p: Some(self.top_p),
            stream: Some(stream),
            n: Some(1),
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(Error::Internal(format!(
                "OpenAI API error ({}): {}",
                status, error_text
            )));
        }

        Ok(response)
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn generate(&self, prompt: &str) -> Result<LlmResponse> {
        let messages = vec![ChatMessage::user(prompt)];
        self.chat(&messages).await
    }

    async fn chat(&self, messages: &[ChatMessage]) -> Result<LlmResponse> {
        let start_time = Instant::now();
        let response = self
            .create_chat_completion(messages.to_vec(), false)
            .await?;
        let api_response: ChatCompletionResponse = response.json().await?;

        let choice = api_response
            .choices
            .first()
            .ok_or_else(|| Error::Internal("No choices in OpenAI response".to_string()))?;

        let content = choice.message.content.clone();
        let tokens_used = api_response.usage.map(|u| u.total_tokens).unwrap_or(0);
        let response_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(LlmResponse {
            content,
            tokens_used,
            response_time_ms,
            model: api_response.model,
            finish_reason: choice.finish_reason.clone(),
        })
    }

    async fn generate_stream(
        &self,
        prompt: &str,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        let messages = vec![ChatMessage::user(prompt)];
        let _response = self.create_chat_completion(messages, true).await?;

        // Note: ストリーミング実装はrequwestのstream featureが必要
        // 現時点ではプレースホルダー実装
        let stream = futures::stream::once(async {
            Err(Error::NotImplemented("Streaming not yet implemented".to_string()))
        });

        Ok(Box::new(Box::pin(stream)))
    }

    fn model_info(&self) -> ModelInfo {
        let (context_window, max_output_tokens, cost) = match self.model.as_str() {
            "gpt-4" => (8192, 4096, Some(0.03)),
            "gpt-4-32k" => (32768, 4096, Some(0.06)),
            "gpt-3.5-turbo" => (4096, 4096, Some(0.002)),
            "gpt-3.5-turbo-16k" => (16384, 4096, Some(0.004)),
            _ => (4096, 2048, None),
        };

        ModelInfo {
            name: self.model.clone(),
            provider: "OpenAI".to_string(),
            context_window,
            max_output_tokens,
            cost_per_1k_tokens: cost,
        }
    }

    async fn health_check(&self) -> Result<()> {
        let response = self
            .client
            .get(format!("{}/models", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::Internal(format!(
                "OpenAI health check failed: {}",
                response.status()
            )))
        }
    }
}

/// チャット完了リクエスト
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

/// チャット完了レスポンス
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

/// 選択肢
#[derive(Debug, Deserialize)]
struct Choice {
    index: u32,
    message: ChatMessage,
    finish_reason: Option<String>,
}

/// トークン使用量
#[derive(Debug, Deserialize)]
struct Usage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_client_creation() {
        let client = OpenAiClient::new("test-key", "gpt-4")
            .with_max_tokens(1000)
            .with_temperature(0.5)
            .with_top_p(0.9);

        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model, "gpt-4");
        assert_eq!(client.max_tokens, 1000);
        assert_eq!(client.temperature, 0.5);
        assert_eq!(client.top_p, 0.9);
    }

    #[test]
    fn test_model_info() {
        let client = OpenAiClient::new("test-key", "gpt-4");
        let info = client.model_info();

        assert_eq!(info.name, "gpt-4");
        assert_eq!(info.provider, "OpenAI");
        assert_eq!(info.context_window, 8192);
        assert_eq!(info.max_output_tokens, 4096);
        assert_eq!(info.cost_per_1k_tokens, Some(0.03));
    }

    #[test]
    fn test_gpt35_model_info() {
        let client = OpenAiClient::new("test-key", "gpt-3.5-turbo");
        let info = client.model_info();

        assert_eq!(info.context_window, 4096);
        assert_eq!(info.cost_per_1k_tokens, Some(0.002));
    }
}
