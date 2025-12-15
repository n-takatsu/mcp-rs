//! NLP (Natural Language Processing) integration tests

use mcp_rs::ai::nlp::{
    context::{ConversationContext, DefaultContextManager},
    entity::{DefaultEntityExtractor, EntityExtractor, EntityType},
    intent::{Action, DefaultIntentClassifier, IntentClassifier},
    parser::{DefaultQueryParser, QueryParser},
};
use std::collections::HashMap;

#[tokio::test]
async fn test_query_parser_basic() {
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("Create a new blog post").await.unwrap();

    assert_eq!(parsed.text, "Create a new blog post");
    assert!(!parsed.tokens.is_empty());
    assert!(parsed.language.is_some());
}

#[tokio::test]
async fn test_query_parser_japanese() {
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("新しい投稿を作成してください").await.unwrap();

    assert_eq!(parsed.language, Some("ja".to_string()));
    assert!(!parsed.tokens.is_empty());
}

#[tokio::test]
async fn test_query_parser_normalization() {
    let parser = DefaultQueryParser::new();
    let normalized = parser.normalize("  Hello   World  ");
    assert_eq!(normalized, "Hello World");
}

#[tokio::test]
async fn test_query_parser_tokenization() {
    let parser = DefaultQueryParser::new();
    let tokens = parser.tokenize("create new post").unwrap();

    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].text, "create");
    assert_eq!(tokens[1].text, "new");
    assert_eq!(tokens[2].text, "post");
}

#[tokio::test]
async fn test_query_parser_sentiment() {
    let parser = DefaultQueryParser::new().with_sentiment(true);
    let parsed = parser.parse("This is a great post!").await.unwrap();

    assert!(parsed.sentiment.is_some());
    let sentiment = parsed.sentiment.unwrap();
    assert_eq!(sentiment.dominant(), "positive");
}

#[test]
fn test_entity_extractor_datetime() {
    let extractor = DefaultEntityExtractor::new();
    let entities = extractor.extract("昨日の投稿を更新してください").unwrap();

    assert!(!entities.is_empty());
    assert!(entities
        .iter()
        .any(|e| e.entity_type == EntityType::DateTime));
}

#[test]
fn test_entity_extractor_numbers() {
    let extractor = DefaultEntityExtractor::new();
    let entities = extractor.extract("post 123 を削除").unwrap();

    let number_entities: Vec<_> = entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Number)
        .collect();
    assert!(!number_entities.is_empty());
    assert_eq!(number_entities[0].value, "123");
}

#[test]
fn test_entity_extractor_wordpress() {
    let extractor = DefaultEntityExtractor::new();
    let entities = extractor.extract("新しい投稿を作成").unwrap();

    assert!(entities.iter().any(|e| e.entity_type == EntityType::Post));
}

#[test]
fn test_entity_extractor_urls() {
    let extractor = DefaultEntityExtractor::new();
    let entities = extractor
        .extract("Visit https://example.com for more info")
        .unwrap();

    assert!(entities.iter().any(|e| e.entity_type == EntityType::Url));
}

#[test]
fn test_entity_extractor_type_filter() {
    let extractor = DefaultEntityExtractor::new();
    let entities = extractor
        .extract_type("昨日の投稿 123 を更新", EntityType::DateTime)
        .unwrap();

    assert!(entities
        .iter()
        .all(|e| e.entity_type == EntityType::DateTime));
}

#[tokio::test]
async fn test_intent_classifier_create_post() {
    let classifier = DefaultIntentClassifier::new();
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("新しい投稿を作成してください").await.unwrap();

    let intent = classifier.classify(&parsed).await.unwrap();
    assert_eq!(intent.action, Action::CreatePost);
    assert!(intent.confidence > 0.7);
}

#[tokio::test]
async fn test_intent_classifier_update_post() {
    let classifier = DefaultIntentClassifier::new();
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("投稿を更新").await.unwrap();

    let intent = classifier.classify(&parsed).await.unwrap();
    assert_eq!(intent.action, Action::UpdatePost);
}

#[tokio::test]
async fn test_intent_classifier_delete_post() {
    let classifier = DefaultIntentClassifier::new();
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("投稿を削除してください").await.unwrap();

    let intent = classifier.classify(&parsed).await.unwrap();
    assert_eq!(intent.action, Action::DeletePost);
}

#[tokio::test]
async fn test_intent_classifier_search() {
    let classifier = DefaultIntentClassifier::new();
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("投稿を検索").await.unwrap();

    let intent = classifier.classify(&parsed).await.unwrap();
    assert_eq!(intent.action, Action::SearchPosts);
}

#[tokio::test]
async fn test_intent_classifier_list_posts() {
    let classifier = DefaultIntentClassifier::new();
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("すべての投稿を表示").await.unwrap();

    let intent = classifier.classify(&parsed).await.unwrap();
    assert_eq!(intent.action, Action::ListPosts);
}

#[tokio::test]
async fn test_intent_classifier_english() {
    let classifier = DefaultIntentClassifier::new();
    let parser = DefaultQueryParser::new();
    let parsed = parser.parse("Create a new post").await.unwrap();

    let intent = classifier.classify(&parsed).await.unwrap();
    assert_eq!(intent.action, Action::CreatePost);
}

#[tokio::test]
async fn test_intent_classifier_with_entities() {
    let classifier = DefaultIntentClassifier::new();
    let parser = DefaultQueryParser::new();
    let extractor = DefaultEntityExtractor::new();

    let query = "昨日の投稿を更新してください";
    let mut parsed = parser.parse(query).await.unwrap();
    let entities = extractor.extract(query).unwrap();
    parsed = parsed.with_entities(entities);

    let intent = classifier.classify(&parsed).await.unwrap();
    assert_eq!(intent.action, Action::UpdatePost);
    assert!(!intent.entities.is_empty());
}

#[test]
fn test_conversation_context_creation() {
    let context = ConversationContext::new("session-123".to_string());
    assert_eq!(context.session_id, "session-123");
    assert_eq!(context.turns.len(), 0);
}

#[test]
fn test_conversation_context_add_turn() {
    let mut context = ConversationContext::new("session-123".to_string());
    context.add_turn("Hello".to_string(), "Hi there!".to_string());

    assert_eq!(context.turns.len(), 1);
    assert_eq!(context.turns[0].query, "Hello");
    assert_eq!(context.turns[0].response, "Hi there!");
}

#[test]
fn test_conversation_context_last_turns() {
    let mut context = ConversationContext::new("session-123".to_string());
    context.add_turn("Q1".to_string(), "A1".to_string());
    context.add_turn("Q2".to_string(), "A2".to_string());
    context.add_turn("Q3".to_string(), "A3".to_string());

    let last_2 = context.last_turns(2);
    assert_eq!(last_2.len(), 2);
    assert_eq!(last_2[0].query, "Q2");
    assert_eq!(last_2[1].query, "Q3");
}

#[test]
fn test_conversation_context_merge_entities() {
    let mut context = ConversationContext::new("session-123".to_string());
    let mut entities = HashMap::new();
    entities.insert("post_id".to_string(), "123".to_string());
    entities.insert("author".to_string(), "Alice".to_string());

    context.merge_entities(entities);
    assert_eq!(context.entities.get("post_id"), Some(&"123".to_string()));
    assert_eq!(context.entities.get("author"), Some(&"Alice".to_string()));
}

#[test]
fn test_conversation_context_resolve_entity() {
    let mut context = ConversationContext::new("session-123".to_string());
    let mut entities = HashMap::new();
    entities.insert("last_post".to_string(), "post-456".to_string());
    context.merge_entities(entities);

    assert_eq!(
        context.resolve_entity("last_post"),
        Some("post-456".to_string())
    );
}

#[test]
fn test_context_manager_basic() {
    use mcp_rs::ai::nlp::context::ContextManager;

    let mut manager = DefaultContextManager::new("session-123".to_string());
    manager.add_turn("Create a post", "Post created");

    assert_eq!(manager.get_context().turns.len(), 1);
}

#[test]
fn test_context_manager_merge_entities() {
    use mcp_rs::ai::nlp::context::ContextManager;

    let mut manager = DefaultContextManager::new("session-123".to_string());
    let mut entities = HashMap::new();
    entities.insert("test_key".to_string(), "test_value".to_string());

    manager.merge_entities(entities);
    assert_eq!(
        manager.resolve_entity("test_key"),
        Some("test_value".to_string())
    );
}

#[test]
fn test_context_manager_clear() {
    use mcp_rs::ai::nlp::context::ContextManager;

    let mut manager = DefaultContextManager::new("session-123".to_string());
    manager.add_turn("Hello", "Hi");
    manager.clear();

    assert_eq!(manager.get_context().turns.len(), 0);
}

#[tokio::test]
async fn test_end_to_end_query_processing() {
    // Setup components
    let parser = DefaultQueryParser::new();
    let extractor = DefaultEntityExtractor::new();
    let classifier = DefaultIntentClassifier::new();

    // Process query
    let query = "昨日の投稿のタイトルを変更してください";
    let mut parsed = parser.parse(query).await.unwrap();

    // Extract entities
    let entities = extractor.extract(query).unwrap();
    parsed = parsed.with_entities(entities);

    // Classify intent
    let intent = classifier.classify(&parsed).await.unwrap();

    // Verify results
    assert_eq!(intent.action, Action::UpdatePost);
    assert!(intent.confidence > 0.7);
    assert!(!parsed.entities.is_empty());
}

#[tokio::test]
async fn test_multi_turn_conversation() {
    use mcp_rs::ai::nlp::context::ContextManager;

    let parser = DefaultQueryParser::new();
    let classifier = DefaultIntentClassifier::new();
    let mut context_manager = DefaultContextManager::new("session-test".to_string());

    // First turn
    let query1 = "新しい投稿を作成";
    let parsed1 = parser.parse(query1).await.unwrap();
    let intent1 = classifier.classify(&parsed1).await.unwrap();
    context_manager.add_turn(query1, "投稿を作成しました");

    // Add entity to context
    let mut entities = HashMap::new();
    entities.insert("last_post".to_string(), "post-123".to_string());
    context_manager.merge_entities(entities);

    // Second turn - using context
    let query2 = "それを更新";
    let parsed2 = parser.parse(query2).await.unwrap();
    let intent2 = classifier.classify(&parsed2).await.unwrap();
    context_manager.add_turn(query2, "投稿を更新しました");

    // Resolve entity from context
    let resolved = context_manager.resolve_entity("last_post");

    assert_eq!(intent1.action, Action::CreatePost);
    assert_eq!(intent2.action, Action::UpdatePost);
    assert_eq!(resolved, Some("post-123".to_string()));
    assert_eq!(context_manager.get_context().turns.len(), 2);
}
