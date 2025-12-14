//! AI Integration Tests
//!
//! OpenAI統合とLLMクライアントの統合テスト

use mcp_rs::ai::llm::openai::OpenAiClient;
use mcp_rs::ai::llm::{ChatMessage, LlmClient, LlmConfig, LlmProvider};

#[test]
fn test_openai_client_builder() {
    let client = OpenAiClient::new("test-api-key", "gpt-4")
        .with_max_tokens(1000)
        .with_temperature(0.8)
        .with_top_p(0.95);

    let info = client.model_info();
    assert_eq!(info.name, "gpt-4");
    assert_eq!(info.provider, "OpenAI");
    assert_eq!(info.context_window, 8192);
    assert_eq!(info.max_output_tokens, 4096);
}

#[test]
fn test_gpt4_model_info() {
    let client = OpenAiClient::new("test-key", "gpt-4");
    let info = client.model_info();

    assert_eq!(info.name, "gpt-4");
    assert_eq!(info.context_window, 8192);
    assert_eq!(info.max_output_tokens, 4096);
    assert_eq!(info.cost_per_1k_tokens, Some(0.03));
}

#[test]
fn test_gpt4_32k_model_info() {
    let client = OpenAiClient::new("test-key", "gpt-4-32k");
    let info = client.model_info();

    assert_eq!(info.context_window, 32768);
    assert_eq!(info.cost_per_1k_tokens, Some(0.06));
}

#[test]
fn test_gpt35_turbo_model_info() {
    let client = OpenAiClient::new("test-key", "gpt-3.5-turbo");
    let info = client.model_info();

    assert_eq!(info.context_window, 4096);
    assert_eq!(info.cost_per_1k_tokens, Some(0.002));
}

#[test]
fn test_gpt35_turbo_16k_model_info() {
    let client = OpenAiClient::new("test-key", "gpt-3.5-turbo-16k");
    let info = client.model_info();

    assert_eq!(info.context_window, 16384);
    assert_eq!(info.cost_per_1k_tokens, Some(0.004));
}

#[test]
fn test_unknown_model_info() {
    let client = OpenAiClient::new("test-key", "unknown-model");
    let info = client.model_info();

    // デフォルト値が返される
    assert_eq!(info.context_window, 4096);
    assert_eq!(info.max_output_tokens, 2048);
    assert_eq!(info.cost_per_1k_tokens, None);
}

#[test]
fn test_chat_message_builders() {
    let system = ChatMessage::system("You are a helpful assistant");
    assert_eq!(system.role, "system");
    assert_eq!(system.content, "You are a helpful assistant");

    let user = ChatMessage::user("Hello, AI!");
    assert_eq!(user.role, "user");
    assert_eq!(user.content, "Hello, AI!");

    let assistant = ChatMessage::assistant("Hello! How can I help?");
    assert_eq!(assistant.role, "assistant");
    assert_eq!(assistant.content, "Hello! How can I help?");
}

#[test]
fn test_chat_conversation_flow() {
    let messages = [
        ChatMessage::system("You are a math tutor"),
        ChatMessage::user("What is 2+2?"),
        ChatMessage::assistant("2+2 equals 4"),
        ChatMessage::user("And 3+3?"),
    ];

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].role, "system");
    assert_eq!(messages[1].role, "user");
    assert_eq!(messages[2].role, "assistant");
    assert_eq!(messages[3].role, "user");
}

#[test]
fn test_llm_config_default() {
    let config = LlmConfig::default();

    assert_eq!(config.max_tokens, 2048);
    assert_eq!(config.temperature, 0.7);
    assert_eq!(config.top_p, 1.0);
    assert!(!config.streaming);
    assert_eq!(config.timeout_seconds, 30);

    match config.provider {
        LlmProvider::OpenAI { model, .. } => {
            assert_eq!(model, "gpt-4");
        }
        _ => panic!("Expected OpenAI provider"),
    }
}

#[test]
fn test_llm_provider_variants() {
    let openai = LlmProvider::OpenAI {
        model: "gpt-4".to_string(),
        api_key: "test-key".to_string(),
    };
    assert!(matches!(openai, LlmProvider::OpenAI { .. }));

    let local = LlmProvider::Local {
        model_path: "/path/to/model".to_string(),
        context_size: 4096,
    };
    assert!(matches!(local, LlmProvider::Local { .. }));

    let candle = LlmProvider::Candle {
        model_id: "mistral-7b".to_string(),
        device: "cuda".to_string(),
    };
    assert!(matches!(candle, LlmProvider::Candle { .. }));
}

#[test]
fn test_llm_config_custom() {
    let config = LlmConfig {
        provider: LlmProvider::OpenAI {
            model: "gpt-3.5-turbo".to_string(),
            api_key: "custom-key".to_string(),
        },
        max_tokens: 1000,
        temperature: 0.5,
        top_p: 0.9,
        streaming: true,
        timeout_seconds: 60,
    };

    assert_eq!(config.max_tokens, 1000);
    assert_eq!(config.temperature, 0.5);
    assert!(config.streaming);
}

#[test]
fn test_custom_base_url() {
    let client =
        OpenAiClient::new("test-key", "gpt-4").with_base_url("https://custom-api.example.com/v1");

    // カスタムベースURLが設定されていることを確認
    // (内部実装の詳細を直接テストできないため、clientの作成が成功することを確認)
    let info = client.model_info();
    assert_eq!(info.name, "gpt-4");
}

#[test]
fn test_temperature_bounds() {
    // 温度パラメータの境界値テスト
    let cold = OpenAiClient::new("test-key", "gpt-4").with_temperature(0.0);
    let info = cold.model_info();
    assert_eq!(info.name, "gpt-4");

    let hot = OpenAiClient::new("test-key", "gpt-4").with_temperature(2.0);
    let info = hot.model_info();
    assert_eq!(info.name, "gpt-4");
}

#[test]
fn test_top_p_bounds() {
    // Top-pパラメータの境界値テスト
    let min = OpenAiClient::new("test-key", "gpt-4").with_top_p(0.0);
    let info = min.model_info();
    assert_eq!(info.name, "gpt-4");

    let max = OpenAiClient::new("test-key", "gpt-4").with_top_p(1.0);
    let info = max.model_info();
    assert_eq!(info.name, "gpt-4");
}

#[test]
fn test_max_tokens_configuration() {
    let small = OpenAiClient::new("test-key", "gpt-4").with_max_tokens(100);
    let info = small.model_info();
    assert_eq!(info.max_output_tokens, 4096); // モデルの最大値

    let large = OpenAiClient::new("test-key", "gpt-4").with_max_tokens(4096);
    let info = large.model_info();
    assert_eq!(info.max_output_tokens, 4096);
}

// Note: 実際のAPI呼び出しテストは環境変数OPENAI_API_KEYが設定されている場合のみ実行
#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_openai_health_check_with_valid_key() {
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let client = OpenAiClient::new(api_key, "gpt-4");
        let result = client.health_check().await;
        assert!(
            result.is_ok(),
            "Health check should succeed with valid API key"
        );
    }
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_openai_generate_with_valid_key() {
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let client = OpenAiClient::new(api_key, "gpt-3.5-turbo")
            .with_max_tokens(100)
            .with_temperature(0.7);

        let result = client.generate("Say 'test successful' in one word").await;
        assert!(result.is_ok(), "Generate should succeed with valid API key");

        if let Ok(response) = result {
            assert!(!response.content.is_empty());
            assert!(response.tokens_used > 0);
            assert!(response.response_time_ms > 0);
        }
    }
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_openai_chat_with_valid_key() {
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let client = OpenAiClient::new(api_key, "gpt-3.5-turbo");

        let messages = vec![
            ChatMessage::system("You are a helpful assistant"),
            ChatMessage::user("Respond with exactly one word: yes"),
        ];

        let result = client.chat(&messages).await;
        assert!(result.is_ok(), "Chat should succeed with valid API key");

        if let Ok(response) = result {
            assert!(!response.content.is_empty());
            assert_eq!(response.model, "gpt-3.5-turbo");
        }
    }
}

#[test]
fn test_multiple_model_comparison() {
    let models = vec![
        ("gpt-4", 8192, 0.03),
        ("gpt-4-32k", 32768, 0.06),
        ("gpt-3.5-turbo", 4096, 0.002),
        ("gpt-3.5-turbo-16k", 16384, 0.004),
    ];

    for (model_name, expected_context, expected_cost) in models {
        let client = OpenAiClient::new("test-key", model_name);
        let info = client.model_info();

        assert_eq!(info.name, model_name);
        assert_eq!(info.context_window, expected_context);
        assert_eq!(info.cost_per_1k_tokens, Some(expected_cost));
    }
}
