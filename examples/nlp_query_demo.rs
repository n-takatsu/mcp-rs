//! Natural Language Query Processing Demo
//!
//! This example demonstrates the NLP (Natural Language Processing) capabilities
//! for understanding and processing natural language queries.

use mcp_rs::ai::nlp::{
    context::{ConversationContext, DefaultContextManager},
    entity::{DefaultEntityExtractor, EntityExtractor},
    intent::{Action, DefaultIntentClassifier, IntentClassifier},
    parser::{DefaultQueryParser, QueryParser},
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Natural Language Query Processing Demo ===\n");

    // Demo 1: Query Parsing
    demo_query_parsing().await?;

    // Demo 2: Entity Extraction
    demo_entity_extraction()?;

    // Demo 3: Intent Classification
    demo_intent_classification().await?;

    // Demo 4: Conversation Context
    demo_conversation_context()?;

    // Demo 5: End-to-End Processing
    demo_end_to_end_processing().await?;

    println!("\n=== Demo Complete ===");
    Ok(())
}

/// Demonstrates query parsing capabilities
async fn demo_query_parsing() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 1: Query Parsing ---");

    let parser = DefaultQueryParser::new();

    let queries = vec![
        "Create a new blog post about Rust programming",
        "新しい投稿を作成してください",
        "Update the post title to 'Hello World'",
        "昨日の投稿を削除してください",
    ];

    for query in queries {
        let parsed = parser.parse(query).await?;
        println!("\nQuery: {}", query);
        println!("  Normalized: {}", parsed.normalized);
        println!("  Language: {:?}", parsed.language);
        println!("  Tokens: {} tokens", parsed.tokens.len());
        if let Some(sentiment) = &parsed.sentiment {
            println!(
                "  Sentiment: {} (score: {:.2})",
                sentiment.dominant(),
                sentiment.score()
            );
        }
    }

    println!();
    Ok(())
}

/// Demonstrates entity extraction
fn demo_entity_extraction() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 2: Entity Extraction ---");

    let extractor = DefaultEntityExtractor::new();

    let test_cases = vec![
        "昨日作成した投稿のタイトルを変更",
        "post 123 を削除してください",
        "Visit https://example.com for more info",
        "今日の投稿を更新",
        "新しいページを作成",
    ];

    for text in test_cases {
        let entities = extractor.extract(text)?;
        println!("\nText: {}", text);
        println!("  Entities found: {}", entities.len());
        for entity in entities {
            println!(
                "    - {:?}: '{}' (confidence: {:.2})",
                entity.entity_type, entity.value, entity.confidence
            );
        }
    }

    println!();
    Ok(())
}

/// Demonstrates intent classification
async fn demo_intent_classification() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 3: Intent Classification ---");

    let parser = DefaultQueryParser::new();
    let classifier = DefaultIntentClassifier::new();

    let queries = vec![
        ("新しい投稿を作成", Action::CreatePost),
        ("投稿を更新してください", Action::UpdatePost),
        ("post を削除", Action::DeletePost),
        ("すべての投稿を表示", Action::ListPosts),
        ("投稿を検索", Action::SearchPosts),
        ("ユーザー情報を取得", Action::GetUser),
    ];

    for (query, expected_action) in queries {
        let parsed = parser.parse(query).await?;
        let intent = classifier.classify(&parsed).await?;

        println!("\nQuery: {}", query);
        println!("  Action: {:?}", intent.action);
        println!("  Description: {}", intent.action.description());
        println!("  Confidence: {:.2}%", intent.confidence * 100.0);
        println!(
            "  Type: {}",
            if intent.action.is_write_operation() {
                "Write Operation"
            } else if intent.action.is_read_operation() {
                "Read Operation"
            } else {
                "Unknown"
            }
        );

        if !intent.entities.is_empty() {
            println!("  Entities:");
            for (key, value) in &intent.entities {
                println!("    - {}: {}", key, value);
            }
        }

        assert_eq!(
            intent.action, expected_action,
            "Expected {:?} but got {:?}",
            expected_action, intent.action
        );
    }

    println!();
    Ok(())
}

/// Demonstrates conversation context management
fn demo_conversation_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 4: Conversation Context ---");

    let mut context = ConversationContext::new("demo-session".to_string());

    // Simulate a conversation
    println!("\nSimulating a multi-turn conversation:");

    // Turn 1
    context.add_turn(
        "Create a new post about Rust".to_string(),
        "Post created with ID 123".to_string(),
    );
    let mut entities = HashMap::new();
    entities.insert("post_id".to_string(), "123".to_string());
    entities.insert("last_action".to_string(), "create_post".to_string());
    context.merge_entities(entities);
    println!("  Turn 1:");
    println!("    User: Create a new post about Rust");
    println!("    System: Post created with ID 123");

    // Turn 2
    context.add_turn(
        "Update its title".to_string(),
        "Title updated successfully".to_string(),
    );
    println!("  Turn 2:");
    println!("    User: Update its title");
    println!("    System: Title updated successfully");

    // Turn 3
    context.add_turn(
        "Delete it".to_string(),
        "Post deleted successfully".to_string(),
    );
    println!("  Turn 3:");
    println!("    User: Delete it");
    println!("    System: Post deleted successfully");

    // Show context information
    println!("\nContext Information:");
    println!("  Session ID: {}", context.session_id);
    println!("  Total turns: {}", context.turns.len());
    println!("  Duration: {} seconds", context.duration());

    // Test entity resolution
    if let Some(post_id) = context.resolve_entity("post_id") {
        println!("  Last post ID: {}", post_id);
    }

    // Show last 2 turns
    println!("\nLast 2 turns:");
    for turn in context.last_turns(2) {
        println!("  Turn {}:", turn.turn_number);
        println!("    Q: {}", turn.query);
        println!("    A: {}", turn.response);
    }

    println!();
    Ok(())
}

/// Demonstrates end-to-end query processing
async fn demo_end_to_end_processing() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Demo 5: End-to-End Processing ---");

    use mcp_rs::ai::nlp::context::ContextManager;

    // Initialize components
    let parser = DefaultQueryParser::new();
    let extractor = DefaultEntityExtractor::new();
    let classifier = DefaultIntentClassifier::new();
    let mut context_manager = DefaultContextManager::new("e2e-demo".to_string());

    // Test queries
    let queries = vec![
        "昨日作成した投稿のタイトルを変更してください",
        "新しいページを作成して、カテゴリを設定",
        "post 456 を削除",
    ];

    for query in queries {
        println!("\n=== Processing Query ===");
        println!("Input: {}", query);

        // Step 1: Parse query
        let mut parsed = parser.parse(query).await?;
        println!("\n1. Parsing:");
        println!("   - Language: {:?}", parsed.language);
        println!("   - Tokens: {}", parsed.tokens.len());

        // Step 2: Extract entities
        let entities = extractor.extract(query)?;
        parsed = parsed.with_entities(entities);
        println!("\n2. Entity Extraction:");
        for entity in &parsed.entities {
            println!(
                "   - {:?}: {} (confidence: {:.2})",
                entity.entity_type, entity.value, entity.confidence
            );
        }

        // Step 3: Classify intent
        let intent = classifier.classify(&parsed).await?;
        println!("\n3. Intent Classification:");
        println!("   - Action: {:?}", intent.action);
        println!("   - Confidence: {:.2}%", intent.confidence * 100.0);
        println!("   - Description: {}", intent.action.description());

        // Step 4: Update context
        context_manager.add_turn(query, "Operation completed successfully");
        let mut ctx_entities = HashMap::new();
        for entity in &parsed.entities {
            let key = format!("{:?}", entity.entity_type).to_lowercase();
            ctx_entities.insert(key, entity.value.clone());
        }
        context_manager.merge_entities(ctx_entities);

        println!("\n4. Context Update:");
        println!(
            "   - Total turns: {}",
            context_manager.get_context().turns.len()
        );
        println!(
            "   - Stored entities: {}",
            context_manager.get_context().entities.len()
        );
    }

    println!();
    Ok(())
}
