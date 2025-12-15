//! Intent classification and action detection

use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::parser::ParsedQuery;

/// WordPress action types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Action {
    /// Create a new post
    CreatePost,
    /// Update an existing post
    UpdatePost,
    /// Delete a post
    DeletePost,
    /// Get post details
    GetPost,
    /// List posts
    ListPosts,
    /// Search posts
    SearchPosts,
    /// Create a page
    CreatePage,
    /// Update a page
    UpdatePage,
    /// Delete a page
    DeletePage,
    /// Get user information
    GetUser,
    /// List users
    ListUsers,
    /// Add comment
    AddComment,
    /// Get comments
    GetComments,
    /// Upload media
    UploadMedia,
    /// Get media
    GetMedia,
    /// Unknown action
    Unknown,
}

impl Action {
    /// Returns a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Action::CreatePost => "Create a new post",
            Action::UpdatePost => "Update an existing post",
            Action::DeletePost => "Delete a post",
            Action::GetPost => "Get post details",
            Action::ListPosts => "List posts",
            Action::SearchPosts => "Search posts",
            Action::CreatePage => "Create a new page",
            Action::UpdatePage => "Update a page",
            Action::DeletePage => "Delete a page",
            Action::GetUser => "Get user information",
            Action::ListUsers => "List users",
            Action::AddComment => "Add a comment",
            Action::GetComments => "Get comments",
            Action::UploadMedia => "Upload media",
            Action::GetMedia => "Get media",
            Action::Unknown => "Unknown action",
        }
    }

    /// Checks if this is a write operation
    pub fn is_write_operation(&self) -> bool {
        matches!(
            self,
            Action::CreatePost
                | Action::UpdatePost
                | Action::DeletePost
                | Action::CreatePage
                | Action::UpdatePage
                | Action::DeletePage
                | Action::AddComment
                | Action::UploadMedia
        )
    }

    /// Checks if this is a read operation
    pub fn is_read_operation(&self) -> bool {
        matches!(
            self,
            Action::GetPost
                | Action::ListPosts
                | Action::SearchPosts
                | Action::GetUser
                | Action::ListUsers
                | Action::GetComments
                | Action::GetMedia
        )
    }
}

/// Recognized intent with action and parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// The action to perform
    pub action: Action,
    /// Extracted entities/parameters
    pub entities: HashMap<String, String>,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Alternative intents
    pub alternatives: Vec<(Action, f64)>,
}

impl Intent {
    /// Creates a new intent
    pub fn new(action: Action, confidence: f64) -> Self {
        Self {
            action,
            entities: HashMap::new(),
            confidence,
            alternatives: Vec::new(),
        }
    }

    /// Adds an entity
    pub fn with_entity(mut self, key: String, value: String) -> Self {
        self.entities.insert(key, value);
        self
    }

    /// Adds multiple entities
    pub fn with_entities(mut self, entities: HashMap<String, String>) -> Self {
        self.entities.extend(entities);
        self
    }

    /// Adds alternative intents
    pub fn with_alternatives(mut self, alternatives: Vec<(Action, f64)>) -> Self {
        self.alternatives = alternatives;
        self
    }

    /// Gets an entity value
    pub fn get_entity(&self, key: &str) -> Option<&String> {
        self.entities.get(key)
    }

    /// Checks if the intent is confident enough
    pub fn is_confident(&self, threshold: f64) -> bool {
        self.confidence >= threshold
    }

    /// Returns the best alternative if confidence is low
    pub fn best_alternative(&self) -> Option<(Action, f64)> {
        self.alternatives.first().copied()
    }
}

/// Intent classifier trait
#[async_trait]
pub trait IntentClassifier: Send + Sync {
    /// Classifies the intent from a parsed query
    async fn classify(&self, parsed: &ParsedQuery) -> Result<Intent>;

    /// Classifies with multiple alternatives
    async fn classify_with_alternatives(
        &self,
        parsed: &ParsedQuery,
        top_k: usize,
    ) -> Result<Vec<Intent>>;
}

/// Default intent classifier implementation
#[derive(Debug, Clone)]
pub struct DefaultIntentClassifier {
    /// Confidence threshold
    confidence_threshold: f64,
}

impl DefaultIntentClassifier {
    /// Creates a new default intent classifier
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.7,
        }
    }

    /// Sets confidence threshold
    pub fn with_confidence_threshold(mut self, threshold: f64) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    /// Matches action patterns in text
    fn match_action(&self, text: &str) -> (Action, f64) {
        let text_lower = text.to_lowercase();

        // Create patterns
        let create_patterns = ["create", "作成", "新規", "追加", "add"];
        let update_patterns = ["update", "更新", "編集", "modify", "変更"];
        let delete_patterns = ["delete", "削除", "remove"];
        let get_patterns = ["get", "取得", "show", "表示", "view"];
        let list_patterns = ["list", "一覧", "all", "すべて"];
        let search_patterns = ["search", "検索", "find", "探す"];

        // Check create
        if create_patterns.iter().any(|p| text_lower.contains(p)) {
            if text_lower.contains("post") || text_lower.contains("投稿") {
                return (Action::CreatePost, 0.9);
            }
            if text_lower.contains("page") || text_lower.contains("ページ") {
                return (Action::CreatePage, 0.9);
            }
            return (Action::CreatePost, 0.7);
        }

        // Check update
        if update_patterns.iter().any(|p| text_lower.contains(p)) {
            if text_lower.contains("post") || text_lower.contains("投稿") {
                return (Action::UpdatePost, 0.9);
            }
            if text_lower.contains("page") || text_lower.contains("ページ") {
                return (Action::UpdatePage, 0.9);
            }
            return (Action::UpdatePost, 0.7);
        }

        // Check delete
        if delete_patterns.iter().any(|p| text_lower.contains(p)) {
            if text_lower.contains("post") || text_lower.contains("投稿") {
                return (Action::DeletePost, 0.9);
            }
            if text_lower.contains("page") || text_lower.contains("ページ") {
                return (Action::DeletePage, 0.9);
            }
            return (Action::DeletePost, 0.7);
        }

        // Check search
        if search_patterns.iter().any(|p| text_lower.contains(p)) {
            return (Action::SearchPosts, 0.85);
        }

        // Check list
        if list_patterns.iter().any(|p| text_lower.contains(p)) {
            if text_lower.contains("user") || text_lower.contains("ユーザー") {
                return (Action::ListUsers, 0.85);
            }
            return (Action::ListPosts, 0.85);
        }

        // Check get
        if get_patterns.iter().any(|p| text_lower.contains(p)) {
            if text_lower.contains("user") || text_lower.contains("ユーザー") {
                return (Action::GetUser, 0.85);
            }
            if text_lower.contains("comment") || text_lower.contains("コメント") {
                return (Action::GetComments, 0.85);
            }
            return (Action::GetPost, 0.8);
        }

        // Comment actions
        if text_lower.contains("comment") || text_lower.contains("コメント") {
            if create_patterns.iter().any(|p| text_lower.contains(p)) {
                return (Action::AddComment, 0.85);
            }
            return (Action::GetComments, 0.75);
        }

        // Media actions
        if text_lower.contains("upload") || text_lower.contains("アップロード") {
            return (Action::UploadMedia, 0.85);
        }

        // Default
        (Action::Unknown, 0.3)
    }

    /// Extracts entities from parsed query
    fn extract_entities(&self, parsed: &ParsedQuery) -> HashMap<String, String> {
        let mut entities = HashMap::new();

        // Add entities from parsed query
        for entity in &parsed.entities {
            let key = format!("{:?}", entity.entity_type).to_lowercase();
            entities.insert(key, entity.value.clone());
        }

        entities
    }
}

impl Default for DefaultIntentClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IntentClassifier for DefaultIntentClassifier {
    async fn classify(&self, parsed: &ParsedQuery) -> Result<Intent> {
        let (action, confidence) = self.match_action(&parsed.text);
        let entities = self.extract_entities(parsed);

        Ok(Intent::new(action, confidence).with_entities(entities))
    }

    async fn classify_with_alternatives(
        &self,
        parsed: &ParsedQuery,
        _top_k: usize,
    ) -> Result<Vec<Intent>> {
        let primary = self.classify(parsed).await?;

        // For now, just return the primary intent
        // In a real implementation, we would generate multiple alternatives
        Ok(vec![primary])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_description() {
        assert_eq!(Action::CreatePost.description(), "Create a new post");
        assert_eq!(Action::UpdatePost.description(), "Update an existing post");
    }

    #[test]
    fn test_action_is_write_operation() {
        assert!(Action::CreatePost.is_write_operation());
        assert!(Action::UpdatePost.is_write_operation());
        assert!(!Action::GetPost.is_write_operation());
    }

    #[test]
    fn test_action_is_read_operation() {
        assert!(Action::GetPost.is_read_operation());
        assert!(Action::ListPosts.is_read_operation());
        assert!(!Action::CreatePost.is_read_operation());
    }

    #[test]
    fn test_intent_creation() {
        let intent = Intent::new(Action::CreatePost, 0.9);
        assert_eq!(intent.action, Action::CreatePost);
        assert_eq!(intent.confidence, 0.9);
    }

    #[test]
    fn test_intent_with_entity() {
        let intent = Intent::new(Action::CreatePost, 0.9)
            .with_entity("title".to_string(), "Test Post".to_string());
        assert_eq!(intent.get_entity("title"), Some(&"Test Post".to_string()));
    }

    #[tokio::test]
    async fn test_classify_create_post() {
        let classifier = DefaultIntentClassifier::new();
        let parsed = ParsedQuery::new("新しい投稿を作成してください".to_string());
        let intent = classifier.classify(&parsed).await.unwrap();
        assert_eq!(intent.action, Action::CreatePost);
    }

    #[tokio::test]
    async fn test_classify_update_post() {
        let classifier = DefaultIntentClassifier::new();
        let parsed = ParsedQuery::new("投稿を更新".to_string());
        let intent = classifier.classify(&parsed).await.unwrap();
        assert_eq!(intent.action, Action::UpdatePost);
    }

    #[tokio::test]
    async fn test_classify_search() {
        let classifier = DefaultIntentClassifier::new();
        let parsed = ParsedQuery::new("投稿を検索".to_string());
        let intent = classifier.classify(&parsed).await.unwrap();
        assert_eq!(intent.action, Action::SearchPosts);
    }
}
