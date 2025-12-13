//! LLMクライアント

use crate::llm::{
    config::LlmConfig,
    error::{LlmError, LlmResult},
    providers::{create_provider, ChunkStream, LlmProvider},
    types::{LlmRequest, LlmResponse, Message},
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// LLMクライアント
pub struct LlmClient {
    provider: Arc<RwLock<Box<dyn LlmProvider>>>,
    config: LlmConfig,
}

impl LlmClient {
    /// 新しいクライアントを作成
    pub fn new(config: LlmConfig) -> LlmResult<Self> {
        config.validate()?;
        let provider = create_provider(&config)?;

        Ok(Self {
            provider: Arc::new(RwLock::new(provider)),
            config,
        })
    }

    /// 設定を取得
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    /// プロバイダーを切り替え
    pub async fn switch_provider(&self, new_config: LlmConfig) -> LlmResult<()> {
        new_config.validate()?;
        let new_provider = create_provider(&new_config)?;

        let mut provider = self.provider.write().await;
        *provider = new_provider;

        Ok(())
    }

    /// 完了リクエストを送信
    pub async fn complete(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        let provider = self.provider.read().await;
        provider.complete(&request).await
    }

    /// シンプルなテキスト完了
    pub async fn complete_text(&self, prompt: impl Into<String>) -> LlmResult<String> {
        let request = LlmRequest::new(vec![Message::user(prompt)]);
        let response = self.complete(request).await?;
        Ok(response.content)
    }

    /// ストリーミング完了リクエスト
    pub async fn complete_stream(&self, request: LlmRequest) -> LlmResult<ChunkStream> {
        let provider = self.provider.read().await;
        provider.complete_stream(&request).await
    }

    /// 会話形式の完了
    pub async fn chat(&self, messages: Vec<Message>) -> LlmResult<LlmResponse> {
        let request = LlmRequest::new(messages);
        self.complete(request).await
    }

    /// システムプロンプト付きの完了
    pub async fn complete_with_system(
        &self,
        system_prompt: impl Into<String>,
        user_prompt: impl Into<String>,
    ) -> LlmResult<String> {
        let messages = vec![Message::system(system_prompt), Message::user(user_prompt)];
        let request = LlmRequest::new(messages);
        let response = self.complete(request).await?;
        Ok(response.content)
    }

    /// プロバイダー名を取得
    pub async fn provider_name(&self) -> String {
        let provider = self.provider.read().await;
        provider.name().to_string()
    }

    /// サポートされるモデルのリストを取得
    pub async fn supported_models(&self) -> Vec<String> {
        let provider = self.provider.read().await;
        provider.supported_models()
    }
}

#[cfg(all(test, feature = "llm-integration"))]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        let client = LlmClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation_invalid_config() {
        let mut config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        config.default_temperature = 5.0; // Invalid
        let client = LlmClient::new(config);
        assert!(client.is_err());
    }
}
