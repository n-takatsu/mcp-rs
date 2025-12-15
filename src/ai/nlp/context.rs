//! Conversation context management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single turn in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    /// Turn number (1-indexed)
    pub turn_number: usize,
    /// User query
    pub query: String,
    /// System response
    pub response: String,
    /// Timestamp
    pub timestamp: i64,
    /// Extracted entities from this turn
    pub entities: HashMap<String, String>,
}

impl ConversationTurn {
    /// Creates a new conversation turn
    pub fn new(turn_number: usize, query: String, response: String) -> Self {
        Self {
            turn_number,
            query,
            response,
            timestamp: chrono::Utc::now().timestamp(),
            entities: HashMap::new(),
        }
    }

    /// Adds entities to the turn
    pub fn with_entities(mut self, entities: HashMap<String, String>) -> Self {
        self.entities = entities;
        self
    }
}

/// Conversation context for maintaining state across turns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContext {
    /// Unique session ID
    pub session_id: String,
    /// Conversation turns
    pub turns: Vec<ConversationTurn>,
    /// Global entities across all turns
    pub entities: HashMap<String, String>,
    /// Current topic/focus
    pub topic: Option<String>,
    /// User preferences
    pub preferences: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: i64,
    /// Last updated timestamp
    pub updated_at: i64,
}

impl ConversationContext {
    /// Creates a new conversation context
    pub fn new(session_id: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            session_id,
            turns: Vec::new(),
            entities: HashMap::new(),
            topic: None,
            preferences: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Adds a conversation turn
    pub fn add_turn(&mut self, query: String, response: String) {
        let turn_number = self.turns.len() + 1;
        let turn = ConversationTurn::new(turn_number, query, response);
        self.turns.push(turn);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Gets the last N turns
    pub fn last_turns(&self, n: usize) -> &[ConversationTurn] {
        let start = self.turns.len().saturating_sub(n);
        &self.turns[start..]
    }

    /// Gets the most recent turn
    pub fn last_turn(&self) -> Option<&ConversationTurn> {
        self.turns.last()
    }

    /// Merges entities into global context
    pub fn merge_entities(&mut self, entities: HashMap<String, String>) {
        self.entities.extend(entities);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Resolves an entity reference (e.g., "that post", "it")
    pub fn resolve_entity(&self, reference: &str) -> Option<String> {
        // Direct lookup
        if let Some(value) = self.entities.get(reference) {
            return Some(value.clone());
        }

        // Check recent turns for common references
        let reference_lower = reference.to_lowercase();
        
        // Check for temporal references
        if reference_lower.contains("that") || reference_lower.contains("it") {
            // Look for the most recent post/page/user reference
            for turn in self.turns.iter().rev() {
                for (key, value) in &turn.entities {
                    if key == "post" || key == "page" || key == "user" {
                        return Some(value.clone());
                    }
                }
            }
        }

        // Check last turn for specific entity types
        if let Some(last_turn) = self.last_turn() {
            for (key, value) in &last_turn.entities {
                if reference_lower.contains(key) {
                    return Some(value.clone());
                }
            }
        }

        None
    }

    /// Sets the current topic
    pub fn set_topic(&mut self, topic: String) {
        self.topic = Some(topic);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Clears the context
    pub fn clear(&mut self) {
        self.turns.clear();
        self.entities.clear();
        self.topic = None;
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Gets conversation duration in seconds
    pub fn duration(&self) -> i64 {
        self.updated_at - self.created_at
    }

    /// Checks if context is expired (older than TTL)
    pub fn is_expired(&self, ttl_seconds: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        (now - self.updated_at) > ttl_seconds
    }
}

/// Context manager trait
pub trait ContextManager: Send + Sync {
    /// Adds a conversation turn
    fn add_turn(&mut self, query: &str, response: &str);

    /// Gets the current context
    fn get_context(&self) -> &ConversationContext;

    /// Gets mutable context
    fn get_context_mut(&mut self) -> &mut ConversationContext;

    /// Resolves an entity reference
    fn resolve_entity(&self, entity: &str) -> Option<String>;

    /// Merges entities into context
    fn merge_entities(&mut self, entities: HashMap<String, String>);

    /// Clears the context
    fn clear(&mut self);
}

/// Default context manager implementation
#[derive(Debug, Clone)]
pub struct DefaultContextManager {
    /// The conversation context
    context: ConversationContext,
}

impl DefaultContextManager {
    /// Creates a new default context manager
    pub fn new(session_id: String) -> Self {
        Self {
            context: ConversationContext::new(session_id),
        }
    }

    /// Creates a context manager with existing context
    pub fn with_context(context: ConversationContext) -> Self {
        Self { context }
    }
}

impl ContextManager for DefaultContextManager {
    fn add_turn(&mut self, query: &str, response: &str) {
        self.context.add_turn(query.to_string(), response.to_string());
    }

    fn get_context(&self) -> &ConversationContext {
        &self.context
    }

    fn get_context_mut(&mut self) -> &mut ConversationContext {
        &mut self.context
    }

    fn resolve_entity(&self, entity: &str) -> Option<String> {
        self.context.resolve_entity(entity)
    }

    fn merge_entities(&mut self, entities: HashMap<String, String>) {
        self.context.merge_entities(entities);
    }

    fn clear(&mut self) {
        self.context.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_turn_creation() {
        let turn = ConversationTurn::new(1, "Hello".to_string(), "Hi there".to_string());
        assert_eq!(turn.turn_number, 1);
        assert_eq!(turn.query, "Hello");
        assert_eq!(turn.response, "Hi there");
    }

    #[test]
    fn test_context_creation() {
        let context = ConversationContext::new("session-123".to_string());
        assert_eq!(context.session_id, "session-123");
        assert_eq!(context.turns.len(), 0);
    }

    #[test]
    fn test_add_turn() {
        let mut context = ConversationContext::new("session-123".to_string());
        context.add_turn("Hello".to_string(), "Hi".to_string());
        assert_eq!(context.turns.len(), 1);
        assert_eq!(context.turns[0].query, "Hello");
    }

    #[test]
    fn test_last_turns() {
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
    fn test_merge_entities() {
        let mut context = ConversationContext::new("session-123".to_string());
        let mut entities = HashMap::new();
        entities.insert("post_id".to_string(), "123".to_string());
        context.merge_entities(entities);
        assert_eq!(context.entities.get("post_id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_resolve_entity() {
        let mut context = ConversationContext::new("session-123".to_string());
        let mut entities = HashMap::new();
        entities.insert("last_post".to_string(), "post-123".to_string());
        context.merge_entities(entities);
        
        assert_eq!(
            context.resolve_entity("last_post"),
            Some("post-123".to_string())
        );
    }

    #[test]
    fn test_context_manager() {
        let mut manager = DefaultContextManager::new("session-123".to_string());
        manager.add_turn("Hello", "Hi");
        
        assert_eq!(manager.get_context().turns.len(), 1);
        
        let mut entities = HashMap::new();
        entities.insert("test".to_string(), "value".to_string());
        manager.merge_entities(entities);
        
        assert_eq!(manager.resolve_entity("test"), Some("value".to_string()));
    }

    #[test]
    fn test_context_clear() {
        let mut manager = DefaultContextManager::new("session-123".to_string());
        manager.add_turn("Hello", "Hi");
        manager.clear();
        
        assert_eq!(manager.get_context().turns.len(), 0);
    }
}
