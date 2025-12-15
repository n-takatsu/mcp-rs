//! Entity extraction and recognition

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Entity type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EntityType {
    /// Post ID or reference
    Post,
    /// Page ID or reference
    Page,
    /// User ID or reference
    User,
    /// Category name
    Category,
    /// Tag name
    Tag,
    /// Date/time reference
    DateTime,
    /// Number
    Number,
    /// Text content
    Content,
    /// URL
    Url,
    /// Email address
    Email,
    /// Custom type
    Custom,
}

/// Represents an extracted entity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entity {
    /// Entity type
    pub entity_type: EntityType,
    /// Entity value
    pub value: String,
    /// Original text in query
    pub text: String,
    /// Start position in query
    pub start: usize,
    /// End position in query
    pub end: usize,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl Entity {
    /// Creates a new entity
    pub fn new(
        entity_type: EntityType,
        value: String,
        text: String,
        start: usize,
        end: usize,
    ) -> Self {
        Self {
            entity_type,
            value,
            text,
            start,
            end,
            confidence: 1.0,
            metadata: HashMap::new(),
        }
    }

    /// Sets confidence score
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence;
        self
    }

    /// Adds metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Checks if this is a date/time entity
    pub fn is_temporal(&self) -> bool {
        self.entity_type == EntityType::DateTime
    }

    /// Checks if this is a numeric entity
    pub fn is_numeric(&self) -> bool {
        self.entity_type == EntityType::Number
    }
}

/// Entity extractor trait
pub trait EntityExtractor: Send + Sync {
    /// Extracts entities from text
    fn extract(&self, text: &str) -> Result<Vec<Entity>>;

    /// Extracts entities of a specific type
    fn extract_type(&self, text: &str, entity_type: EntityType) -> Result<Vec<Entity>>;
}

/// Default entity extractor implementation
#[derive(Debug, Clone)]
pub struct DefaultEntityExtractor {
    /// Enable date/time extraction
    enable_datetime: bool,
    /// Enable number extraction
    enable_numbers: bool,
    /// Enable URL extraction
    enable_urls: bool,
}

impl DefaultEntityExtractor {
    /// Creates a new default entity extractor
    pub fn new() -> Self {
        Self {
            enable_datetime: true,
            enable_numbers: true,
            enable_urls: true,
        }
    }

    /// Extracts date/time entities
    fn extract_datetime(&self, text: &str) -> Vec<Entity> {
        let mut entities = Vec::new();
        let text_lower = text.to_lowercase();

        // Common temporal expressions
        let temporal_patterns = [
            ("今日", "today"),
            ("昨日", "yesterday"),
            ("明日", "tomorrow"),
            ("先週", "last week"),
            ("来週", "next week"),
            ("今月", "this month"),
            ("先月", "last month"),
            ("今年", "this year"),
            ("yesterday", "yesterday"),
            ("today", "today"),
            ("tomorrow", "tomorrow"),
        ];

        for (pattern, value) in &temporal_patterns {
            if let Some(pos) = text_lower.find(pattern) {
                entities.push(
                    Entity::new(
                        EntityType::DateTime,
                        value.to_string(),
                        pattern.to_string(),
                        pos,
                        pos + pattern.len(),
                    )
                    .with_confidence(0.9),
                );
            }
        }

        entities
    }

    /// Extracts numeric entities
    fn extract_numbers(&self, text: &str) -> Vec<Entity> {
        let mut entities = Vec::new();

        for (i, word) in text.split_whitespace().enumerate() {
            if let Ok(num) = word.parse::<i64>() {
                let pos = text
                    .match_indices(word)
                    .nth(i)
                    .map(|(pos, _)| pos)
                    .unwrap_or(0);
                entities.push(Entity::new(
                    EntityType::Number,
                    num.to_string(),
                    word.to_string(),
                    pos,
                    pos + word.len(),
                ));
            }
        }

        entities
    }

    /// Extracts URL entities
    fn extract_urls(&self, text: &str) -> Vec<Entity> {
        let mut entities = Vec::new();

        for word in text.split_whitespace() {
            if word.starts_with("http://") || word.starts_with("https://") {
                if let Some(pos) = text.find(word) {
                    entities.push(Entity::new(
                        EntityType::Url,
                        word.to_string(),
                        word.to_string(),
                        pos,
                        pos + word.len(),
                    ));
                }
            }
        }

        entities
    }

    /// Extracts WordPress-specific entities
    fn extract_wordpress_entities(&self, text: &str) -> Vec<Entity> {
        let mut entities = Vec::new();
        let text_lower = text.to_lowercase();

        // Post references
        let post_patterns = ["post", "投稿", "記事"];
        for pattern in &post_patterns {
            if text_lower.contains(pattern) {
                if let Some(pos) = text_lower.find(pattern) {
                    entities.push(
                        Entity::new(
                            EntityType::Post,
                            pattern.to_string(),
                            pattern.to_string(),
                            pos,
                            pos + pattern.len(),
                        )
                        .with_confidence(0.8),
                    );
                }
            }
        }

        // Page references
        let page_patterns = ["page", "ページ"];
        for pattern in &page_patterns {
            if text_lower.contains(pattern) {
                if let Some(pos) = text_lower.find(pattern) {
                    entities.push(
                        Entity::new(
                            EntityType::Page,
                            pattern.to_string(),
                            pattern.to_string(),
                            pos,
                            pos + pattern.len(),
                        )
                        .with_confidence(0.8),
                    );
                }
            }
        }

        // User references
        let user_patterns = ["user", "ユーザー", "著者"];
        for pattern in &user_patterns {
            if text_lower.contains(pattern) {
                if let Some(pos) = text_lower.find(pattern) {
                    entities.push(
                        Entity::new(
                            EntityType::User,
                            pattern.to_string(),
                            pattern.to_string(),
                            pos,
                            pos + pattern.len(),
                        )
                        .with_confidence(0.8),
                    );
                }
            }
        }

        entities
    }
}

impl Default for DefaultEntityExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityExtractor for DefaultEntityExtractor {
    fn extract(&self, text: &str) -> Result<Vec<Entity>> {
        let mut all_entities = Vec::new();

        // Extract different entity types
        if self.enable_datetime {
            all_entities.extend(self.extract_datetime(text));
        }

        if self.enable_numbers {
            all_entities.extend(self.extract_numbers(text));
        }

        if self.enable_urls {
            all_entities.extend(self.extract_urls(text));
        }

        // Extract WordPress-specific entities
        all_entities.extend(self.extract_wordpress_entities(text));

        // Sort by position
        all_entities.sort_by_key(|e| e.start);

        Ok(all_entities)
    }

    fn extract_type(&self, text: &str, entity_type: EntityType) -> Result<Vec<Entity>> {
        let all_entities = self.extract(text)?;
        Ok(all_entities
            .into_iter()
            .filter(|e| e.entity_type == entity_type)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_creation() {
        let entity = Entity::new(
            EntityType::Post,
            "123".to_string(),
            "post 123".to_string(),
            0,
            8,
        );
        assert_eq!(entity.entity_type, EntityType::Post);
        assert_eq!(entity.value, "123");
    }

    #[test]
    fn test_entity_confidence() {
        let entity = Entity::new(
            EntityType::Post,
            "123".to_string(),
            "post".to_string(),
            0,
            4,
        )
        .with_confidence(0.95);
        assert_eq!(entity.confidence, 0.95);
    }

    #[test]
    fn test_extract_datetime() {
        let extractor = DefaultEntityExtractor::new();
        let entities = extractor.extract("昨日の投稿を更新してください").unwrap();
        assert!(!entities.is_empty());
        assert!(entities
            .iter()
            .any(|e| e.entity_type == EntityType::DateTime));
    }

    #[test]
    fn test_extract_numbers() {
        let extractor = DefaultEntityExtractor::new();
        let entities = extractor.extract("post 123 を削除").unwrap();
        assert!(entities.iter().any(|e| e.entity_type == EntityType::Number));
    }

    #[test]
    fn test_extract_wordpress_entities() {
        let extractor = DefaultEntityExtractor::new();
        let entities = extractor.extract("新しい投稿を作成").unwrap();
        assert!(entities.iter().any(|e| e.entity_type == EntityType::Post));
    }
}
