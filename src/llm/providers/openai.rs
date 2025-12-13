//! OpenAIプロバイダー実装

#[cfg(feature = "llm-integration")]
use crate::llm::{
    config::LlmConfig,
    error::{LlmError, LlmResult},
    providers::{ChunkStream, LlmProvider as LlmProviderTrait},
    types::{LlmRequest, LlmResponse, Message, Role, StreamChunk, TokenUsage},
};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
    Client,
};
use async_trait::async_trait;
use futures::StreamExt;

/// OpenAIプロバイダー
pub struct OpenAIProvider {
    client: Client<OpenAIConfig>,
    config: LlmConfig,
}

impl OpenAIProvider {
    /// 新しいOpenAIプロバイダーを作成
    pub fn new(config: LlmConfig) -> LlmResult<Self> {
        config.validate()?;

        let api_key = config
            .get_api_key()
            .ok_or_else(|| LlmError::ConfigError("API key is required".to_string()))?;

        let mut openai_config = OpenAIConfig::new().with_api_key(api_key);

        if let Some(org_id) = &config.organization_id {
            openai_config = openai_config.with_org_id(org_id);
        }

        let client = Client::with_config(openai_config);

        Ok(Self { client, config })
    }

    /// Azure OpenAI用のプロバイダーを作成
    pub fn new_azure(config: LlmConfig) -> LlmResult<Self> {
        config.validate()?;

        let api_key = config
            .get_api_key()
            .ok_or_else(|| LlmError::ConfigError("API key is required".to_string()))?;

        let endpoint = config
            .endpoint
            .as_ref()
            .ok_or_else(|| LlmError::ConfigError("Endpoint is required for Azure".to_string()))?;

        let openai_config = OpenAIConfig::new()
            .with_api_key(api_key)
            .with_api_base(endpoint);

        let client = Client::with_config(openai_config);

        Ok(Self { client, config })
    }

    /// メッセージを変換
    fn convert_messages(&self, messages: &[Message]) -> Vec<ChatCompletionRequestMessage> {
        messages
            .iter()
            .filter_map(|msg| match msg.role {
                Role::System => ChatCompletionRequestSystemMessageArgs::default()
                    .content(msg.content.clone())
                    .build()
                    .ok()
                    .map(Into::into),
                Role::User => ChatCompletionRequestUserMessageArgs::default()
                    .content(msg.content.clone())
                    .build()
                    .ok()
                    .map(Into::into),
                Role::Assistant => {
                    // アシスタントメッセージも変換できるように拡張可能
                    None
                }
            })
            .collect()
    }
}

#[async_trait]
impl LlmProviderTrait for OpenAIProvider {
    async fn complete(&self, request: &LlmRequest) -> LlmResult<LlmResponse> {
        let messages = self.convert_messages(&request.messages);

        let model = request
            .model
            .as_ref()
            .unwrap_or(&self.config.default_model)
            .clone();

        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(&model).messages(messages);

        if let Some(temp) = request.temperature {
            req_builder.temperature(temp);
        } else {
            req_builder.temperature(self.config.default_temperature);
        }

        if let Some(max_tokens) = request.max_tokens {
            req_builder.max_tokens(max_tokens as u32);
        }

        let chat_request = req_builder
            .build()
            .map_err(|e| LlmError::InvalidRequest(e.to_string()))?;

        let response = self
            .client
            .chat()
            .create(chat_request)
            .await
            .map_err(|e| LlmError::ApiError(e.to_string()))?;

        let choice = response
            .choices
            .first()
            .ok_or_else(|| LlmError::ApiError("No choices in response".to_string()))?;

        let content = choice.message.content.clone().unwrap_or_default();

        let usage = if let Some(u) = response.usage {
            TokenUsage::new(u.prompt_tokens as usize, u.completion_tokens as usize)
        } else {
            TokenUsage::default()
        };

        Ok(LlmResponse {
            content,
            model: response.model,
            usage,
            id: Some(response.id),
            finish_reason: choice.finish_reason.as_ref().map(|r| format!("{:?}", r)),
        })
    }

    async fn complete_stream(&self, request: &LlmRequest) -> LlmResult<ChunkStream> {
        let messages = self.convert_messages(&request.messages);

        let model = request
            .model
            .as_ref()
            .unwrap_or(&self.config.default_model)
            .clone();

        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(&model).messages(messages).stream(true);

        if let Some(temp) = request.temperature {
            req_builder.temperature(temp);
        }

        let chat_request = req_builder
            .build()
            .map_err(|e| LlmError::InvalidRequest(e.to_string()))?;

        let mut stream = self
            .client
            .chat()
            .create_stream(chat_request)
            .await
            .map_err(|e| LlmError::StreamingError(e.to_string()))?;

        let chunk_stream = async_stream::stream! {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(response) => {
                        if let Some(choice) = response.choices.first() {
                            let content = choice.delta.content.clone().unwrap_or_default();
                            let done = choice.finish_reason.is_some();
                            let finish_reason = choice.finish_reason.as_ref().map(|r| format!("{:?}", r));

                            yield Ok(StreamChunk {
                                content,
                                done,
                                finish_reason,
                            });
                        }
                    }
                    Err(e) => {
                        yield Err(LlmError::StreamingError(e.to_string()));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(chunk_stream))
    }

    fn name(&self) -> &str {
        match self.config.provider {
            crate::llm::config::LlmProvider::OpenAI => "OpenAI",
            crate::llm::config::LlmProvider::AzureOpenAI => "Azure OpenAI",
            _ => "Unknown",
        }
    }

    fn supported_models(&self) -> Vec<String> {
        vec![
            "gpt-3.5-turbo".to_string(),
            "gpt-3.5-turbo-16k".to_string(),
            "gpt-4".to_string(),
            "gpt-4-32k".to_string(),
            "gpt-4-turbo-preview".to_string(),
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        let provider = OpenAIProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_message_conversion() {
        let config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        let provider = OpenAIProvider::new(config).unwrap();

        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("Hello!"),
        ];

        let converted = provider.convert_messages(&messages);
        assert_eq!(converted.len(), 2);
    }
}
