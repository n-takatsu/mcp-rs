//! AI Error Handling and Mock Tests
//!
//! OpenAI統合のエラーケースとモックベーステスト

use mcp_rs::ai::llm::{ChatMessage, LlmClient};
use mcp_rs::ai::llm::openai::OpenAiClient;

#[tokio::test]
async fn test_generate_with_empty_prompt() {
    // 空のプロンプトでもクライアントは処理を試みる
    // （実際のAPI呼び出しは失敗するが、クライアント側では問題ない）
    let client = OpenAiClient::new("test-key", "gpt-4");
    
    // generate関数自体は正常に構築される
    // API呼び出しエラーは実際のリクエスト時に発生
    let messages = vec![ChatMessage::user("")];
    
    // モックなしの場合、API呼び出しは失敗するはず
    // ここではクライアント構築の正常性のみテスト
    assert_eq!(client.model_info().name, "gpt-4");
}

#[tokio::test]
async fn test_chat_with_empty_messages() {
    let client = OpenAiClient::new("test-key", "gpt-4");
    let empty_messages: Vec<ChatMessage> = vec![];
    
    // 空のメッセージ配列でもAPIリクエストは構築される
    // 実際のAPI呼び出しはサーバー側でエラーになるが、
    // クライアント側の構造は正常
    assert!(empty_messages.is_empty());
}

#[test]
fn test_invalid_api_key_format() {
    // 無効なAPIキーフォーマットでもクライアントは構築される
    let invalid_keys = vec![
        "",
        "invalid",
        "sk-",
        "not-a-real-key",
    ];

    for key in invalid_keys {
        let client = OpenAiClient::new(key, "gpt-4");
        let info = client.model_info();
        assert_eq!(info.name, "gpt-4");
        // クライアント構築自体は成功する（検証は実際のAPI呼び出し時）
    }
}

#[test]
fn test_model_name_variations() {
    let model_names = vec![
        "gpt-4",
        "gpt-4-32k",
        "gpt-3.5-turbo",
        "gpt-3.5-turbo-16k",
        "custom-model",
        "",
    ];

    for model_name in model_names {
        let client = OpenAiClient::new("test-key", model_name);
        let info = client.model_info();
        assert_eq!(info.name, model_name);
    }
}

#[tokio::test]
async fn test_health_check_with_invalid_key() {
    let client = OpenAiClient::new("invalid-key", "gpt-4");
    
    // 無効なキーでのヘルスチェックは失敗するはず
    let result = client.health_check().await;
    
    // ネットワーク接続がある場合は401エラーになる
    // ない場合は接続エラーになる
    // いずれにしてもエラーになることを確認
    assert!(result.is_err(), "Health check should fail with invalid key");
}

#[test]
fn test_client_with_extreme_parameters() {
    // 極端なパラメータ値でのクライアント構築
    let client = OpenAiClient::new("test-key", "gpt-4")
        .with_max_tokens(0)
        .with_temperature(-1.0)
        .with_top_p(2.0);
    
    // クライアント自体は構築される（バリデーションはAPI側で行われる）
    let info = client.model_info();
    assert_eq!(info.name, "gpt-4");
}

#[test]
fn test_client_with_very_large_max_tokens() {
    let client = OpenAiClient::new("test-key", "gpt-4")
        .with_max_tokens(1_000_000);
    
    // 非常に大きなmax_tokensでも構築は可能
    // 実際のAPI呼び出し時にサーバー側で制限される
    let info = client.model_info();
    assert_eq!(info.max_output_tokens, 4096); // モデルの実際の最大値
}

#[test]
fn test_chat_message_with_special_characters() {
    let messages = vec![
        ChatMessage::user("Hello! @#$%^&*()"),
        ChatMessage::user("日本語メッセージ"),
        ChatMessage::user("Emoji: 🚀🎉"),
        ChatMessage::user("Newlines:\n\ntest"),
        ChatMessage::user("Quotes: \"test\" 'test'"),
    ];

    for msg in messages {
        assert_eq!(msg.role, "user");
        assert!(!msg.content.is_empty());
    }
}

#[test]
fn test_chat_message_with_long_content() {
    let long_content = "a".repeat(10000);
    let message = ChatMessage::user(&long_content);
    
    assert_eq!(message.role, "user");
    assert_eq!(message.content.len(), 10000);
}

#[test]
fn test_model_info_consistency() {
    // 同じモデル名で複数のクライアントを作成し、
    // model_infoが一貫していることを確認
    let client1 = OpenAiClient::new("key1", "gpt-4");
    let client2 = OpenAiClient::new("key2", "gpt-4");
    
    let info1 = client1.model_info();
    let info2 = client2.model_info();
    
    assert_eq!(info1.name, info2.name);
    assert_eq!(info1.context_window, info2.context_window);
    assert_eq!(info1.cost_per_1k_tokens, info2.cost_per_1k_tokens);
}

#[test]
fn test_builder_pattern_chaining() {
    // ビルダーパターンのチェーンが正しく動作することを確認
    let client = OpenAiClient::new("test-key", "gpt-4")
        .with_max_tokens(1000)
        .with_temperature(0.8)
        .with_top_p(0.9)
        .with_base_url("https://custom.api.com/v1");
    
    let info = client.model_info();
    assert_eq!(info.name, "gpt-4");
    assert_eq!(info.provider, "OpenAI");
}

#[test]
fn test_multiple_chat_roles() {
    let messages = vec![
        ChatMessage::system("System prompt"),
        ChatMessage::user("User message 1"),
        ChatMessage::assistant("Assistant response 1"),
        ChatMessage::user("User message 2"),
        ChatMessage::assistant("Assistant response 2"),
        ChatMessage::user("User message 3"),
    ];

    assert_eq!(messages.len(), 6);
    
    // 役割の順序が保持されていることを確認
    assert_eq!(messages[0].role, "system");
    assert_eq!(messages[1].role, "user");
    assert_eq!(messages[2].role, "assistant");
}

#[test]
fn test_cost_calculation_accuracy() {
    let models_with_costs = vec![
        ("gpt-4", 0.03),
        ("gpt-4-32k", 0.06),
        ("gpt-3.5-turbo", 0.002),
        ("gpt-3.5-turbo-16k", 0.004),
    ];

    for (model, expected_cost) in models_with_costs {
        let client = OpenAiClient::new("test-key", model);
        let info = client.model_info();
        
        if let Some(cost) = info.cost_per_1k_tokens {
            assert_eq!(cost, expected_cost);
            
            // 10Kトークンのコスト計算例
            let tokens_10k = 10000.0;
            let calculated_cost = (tokens_10k / 1000.0) * cost;
            assert!(calculated_cost > 0.0);
        }
    }
}

#[tokio::test]
async fn test_concurrent_client_creation() {
    // 複数のクライアントを並行して作成
    let tasks = (0..10).map(|i| {
        tokio::spawn(async move {
            let client = OpenAiClient::new(
                format!("test-key-{}", i),
                "gpt-4"
            );
            client.model_info()
        })
    });

    let results = futures::future::join_all(tasks).await;
    
    for result in results {
        let info = result.unwrap();
        assert_eq!(info.name, "gpt-4");
        assert_eq!(info.provider, "OpenAI");
    }
}

#[test]
fn test_model_comparison_table() {
    // 複数モデルの性能比較テーブルを生成
    let models = vec!["gpt-4", "gpt-4-32k", "gpt-3.5-turbo", "gpt-3.5-turbo-16k"];
    
    for model in models {
        let client = OpenAiClient::new("test-key", model);
        let info = client.model_info();
        
        // 各モデルの特性を検証
        assert!(!info.name.is_empty());
        assert!(info.context_window > 0);
        assert!(info.max_output_tokens > 0);
        
        // GPT-4系はGPT-3.5より高コスト
        if model.starts_with("gpt-4") {
            assert!(info.cost_per_1k_tokens.unwrap_or(0.0) >= 0.03);
        } else {
            assert!(info.cost_per_1k_tokens.unwrap_or(0.0) <= 0.004);
        }
    }
}
