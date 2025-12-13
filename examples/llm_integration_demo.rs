//! LLM統合システムのデモプログラム
//!
//! このデモでは、LLM統合システムの基本的な使用方法を示します。

use mcp_rs::llm::{
    client::LlmClient,
    config::LlmConfig,
    types::{LlmRequest, Message},
};
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロギング設定
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== LLM Integration System Demo ===\n");

    // 環境変数からAPIキーを取得
    let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| {
        info!("Note: OPENAI_API_KEY not set. Using mock configuration for demonstration.");
        "demo-api-key".to_string()
    });

    // 1. クライアント初期化デモ
    demo_client_initialization(&api_key).await?;

    // 2. シンプルなテキスト完了デモ
    if std::env::var("OPENAI_API_KEY").is_ok() {
        demo_simple_completion(&api_key).await?;
    } else {
        info!("Skipping API calls (no OPENAI_API_KEY set)");
    }

    // 3. 会話形式のデモ
    if std::env::var("OPENAI_API_KEY").is_ok() {
        demo_chat_conversation(&api_key).await?;
    }

    // 4. ストリーミングデモ（デモのみ）
    demo_streaming_concept();

    info!("\n=== Demo Completed ===");
    Ok(())
}

/// クライアント初期化デモ
async fn demo_client_initialization(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== 1. Client Initialization Demo ===");

    // OpenAI設定で初期化
    let config = LlmConfig::openai(api_key, "gpt-3.5-turbo");
    let client = LlmClient::new(config)?;

    info!("✓ LLM Client initialized successfully");
    info!("  Provider: {}", client.provider_name().await);
    info!("  Model: {}", client.config().default_model);
    info!("  Supported models: {:?}", client.supported_models().await);

    Ok(())
}

/// シンプルなテキスト完了デモ
async fn demo_simple_completion(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== 2. Simple Text Completion Demo ===");

    let config = LlmConfig::openai(api_key, "gpt-3.5-turbo");
    let client = LlmClient::new(config)?;

    info!("Sending request: 'Hello, how are you?'");

    match client.complete_text("Hello, how are you?").await {
        Ok(response) => {
            info!("✓ Response received:");
            info!("  {}", response);
        }
        Err(e) => {
            info!("⚠ Request failed: {}", e);
        }
    }

    Ok(())
}

/// 会話形式のデモ
async fn demo_chat_conversation(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== 3. Chat Conversation Demo ===");

    let config = LlmConfig::openai(api_key, "gpt-3.5-turbo");
    let client = LlmClient::new(config)?;

    let messages = vec![
        Message::system("You are a helpful assistant specialized in Rust programming."),
        Message::user("What is the difference between String and &str in Rust?"),
    ];

    info!("Sending chat request with system prompt...");

    match client.chat(messages).await {
        Ok(response) => {
            info!("✓ Response received:");
            info!("  Model: {}", response.model);
            info!(
                "  Tokens: {} prompt + {} completion = {} total",
                response.usage.prompt_tokens,
                response.usage.completion_tokens,
                response.usage.total_tokens
            );
            info!("  Content: {}", response.content);
        }
        Err(e) => {
            info!("⚠ Request failed: {}", e);
        }
    }

    Ok(())
}

/// ストリーミングの概念デモ
fn demo_streaming_concept() {
    info!("\n=== 4. Streaming Concept Demo ===");
    info!("Streaming allows real-time token-by-token responses.");
    info!("\nExample usage:");
    info!("```rust");
    info!("let request = LlmRequest::new(messages).with_streaming(true);");
    info!("let stream = client.complete_stream(request).await?;");
    info!("let content = StreamHelper::process_stream(stream, |chunk| {{");
    info!("    print!(\"{{}}\", chunk); // Print each token as it arrives");
    info!("}}).await?;");
    info!("```");
}

/// 高度な機能デモ（実際のAPI呼び出しなし）
#[allow(dead_code)]
async fn demo_advanced_features(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== Advanced Features Demo ===");

    let config = LlmConfig::openai(api_key, "gpt-4");
    let _client = LlmClient::new(config)?;

    // カスタムパラメータ付きリクエスト
    let request = LlmRequest::new(vec![Message::user("Explain Rust ownership")])
        .with_temperature(0.7)
        .with_max_tokens(500);

    info!("Request configuration:");
    info!("  Temperature: {:?}", request.temperature);
    info!("  Max tokens: {:?}", request.max_tokens);
    info!("  Model: {:?}", request.model);

    Ok(())
}

/// プロバイダー切り替えデモ
#[allow(dead_code)]
async fn demo_provider_switching(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== Provider Switching Demo ===");

    let config = LlmConfig::openai(api_key, "gpt-3.5-turbo");
    let client = LlmClient::new(config)?;

    info!("Initial provider: {}", client.provider_name().await);

    // Azure OpenAIに切り替え（例）
    let azure_config = LlmConfig::azure_openai(
        "azure-key",
        "https://your-resource.openai.azure.com",
        "gpt-4",
    );

    if let Ok(()) = client.switch_provider(azure_config).await {
        info!("✓ Switched to: {}", client.provider_name().await);
    }

    Ok(())
}
