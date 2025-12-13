//! LLM統合システムのテスト

#[cfg(feature = "llm-integration")]
#[cfg(test)]
mod tests {
    use mcp_rs::llm::{
        client::LlmClient,
        config::{LlmConfig, LlmProvider},
        types::{LlmRequest, Message, Role},
    };

    #[test]
    fn test_llm_config_creation() {
        let config = LlmConfig::openai("test-api-key", "gpt-3.5-turbo");
        assert_eq!(config.provider, LlmProvider::OpenAI);
        assert_eq!(config.default_model, "gpt-3.5-turbo");
        assert!(config.get_api_key().is_some());
    }

    #[test]
    fn test_llm_config_validation() {
        let config = LlmConfig::openai("test-api-key", "gpt-4");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_llm_config_invalid_temperature() {
        let mut config = LlmConfig::openai("test-api-key", "gpt-4");
        config.default_temperature = 3.0; // Invalid range
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_llm_config_invalid_max_tokens() {
        let mut config = LlmConfig::openai("test-api-key", "gpt-4");
        config.default_max_tokens = 0; // Invalid
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_local_llm_config() {
        let config = LlmConfig::local("http://localhost:8080", "llama-2");
        assert_eq!(config.provider, LlmProvider::Local);
        assert!(config.api_key.is_none());
        assert!(config.endpoint.is_some());
    }

    #[test]
    fn test_azure_openai_config() {
        let config = LlmConfig::azure_openai("test-key", "https://test.openai.azure.com", "gpt-4");
        assert_eq!(config.provider, LlmProvider::AzureOpenAI);
        assert!(config.endpoint.is_some());
    }

    #[test]
    fn test_message_creation() {
        let system_msg = Message::system("You are a helpful assistant");
        assert_eq!(system_msg.role, Role::System);
        assert_eq!(system_msg.content, "You are a helpful assistant");

        let user_msg = Message::user("Hello!");
        assert_eq!(user_msg.role, Role::User);
        assert_eq!(user_msg.content, "Hello!");

        let assistant_msg = Message::assistant("Hi there!");
        assert_eq!(assistant_msg.role, Role::Assistant);
        assert_eq!(assistant_msg.content, "Hi there!");
    }

    #[test]
    fn test_llm_request_builder() {
        let messages = vec![Message::system("You are helpful"), Message::user("Test")];

        let request = LlmRequest::new(messages)
            .with_model("gpt-4")
            .with_temperature(0.8)
            .with_max_tokens(1000)
            .with_streaming(true);

        assert_eq!(request.model, Some("gpt-4".to_string()));
        assert_eq!(request.temperature, Some(0.8));
        assert_eq!(request.max_tokens, Some(1000));
        assert!(request.stream);
    }

    #[test]
    fn test_client_creation() {
        let config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        let client = LlmClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_creation_with_invalid_config() {
        let mut config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        config.default_temperature = 5.0; // Invalid
        let client = LlmClient::new(config);
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn test_client_provider_name() {
        let config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        let client = LlmClient::new(config).unwrap();
        let name = client.provider_name().await;
        assert_eq!(name, "OpenAI");
    }

    #[tokio::test]
    async fn test_client_supported_models() {
        let config = LlmConfig::openai("test-key", "gpt-3.5-turbo");
        let client = LlmClient::new(config).unwrap();
        let models = client.supported_models().await;
        assert!(!models.is_empty());
        assert!(models.contains(&"gpt-3.5-turbo".to_string()));
        assert!(models.contains(&"gpt-4".to_string()));
    }
}
