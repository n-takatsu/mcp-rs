//! LLMプロバイダー実装

pub mod openai;

use crate::llm::{
    config::LlmConfig,
    error::LlmResult,
    types::{LlmRequest, LlmResponse, StreamChunk},
};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// ストリームの型エイリアス
pub type ChunkStream = Pin<Box<dyn Stream<Item = LlmResult<StreamChunk>> + Send>>;

/// LLMプロバイダートレイト
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 通常の完了リクエスト
    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse>;

    /// ストリーミング完了リクエスト
    async fn complete_stream(&self, request: &LlmRequest) -> LlmResult<ChunkStream>;

    /// プロバイダー名を取得
    fn name(&self) -> &str;

    /// サポートされるモデルのリスト
    fn supported_models(&self) -> Vec<String>;
}

/// プロバイダーファクトリー
pub fn create_provider(config: &LlmConfig) -> LlmResult<Box<dyn LlmProvider>> {
    use crate::llm::config::LlmProvider as ProviderType;

    match config.provider {
        ProviderType::OpenAI => Ok(Box::new(openai::OpenAIProvider::new(config.clone())?)),
        ProviderType::AzureOpenAI => {
            Ok(Box::new(openai::OpenAIProvider::new_azure(config.clone())?))
        }
        ProviderType::Local => Err(crate::llm::error::LlmError::UnsupportedProvider(
            "Local LLM support coming soon".to_string(),
        )),
        ProviderType::Custom => Err(crate::llm::error::LlmError::UnsupportedProvider(
            "Custom provider support coming soon".to_string(),
        )),
    }
}
