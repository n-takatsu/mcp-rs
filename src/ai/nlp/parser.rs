//! Query parsing and text analysis

use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Represents a token in the parsed query
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Token {
    /// The token text
    pub text: String,
    /// Part of speech tag
    pub pos: PartOfSpeech,
    /// Start position in original text
    pub start: usize,
    /// End position in original text
    pub end: usize,
    /// Lemmatized form
    pub lemma: Option<String>,
}

impl Token {
    /// Creates a new token
    pub fn new(text: String, pos: PartOfSpeech, start: usize, end: usize) -> Self {
        Self {
            text,
            pos,
            start,
            end,
            lemma: None,
        }
    }

    /// Sets the lemma
    pub fn with_lemma(mut self, lemma: String) -> Self {
        self.lemma = Some(lemma);
        self
    }
}

/// Part of speech tags
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PartOfSpeech {
    /// Noun
    Noun,
    /// Verb
    Verb,
    /// Adjective
    Adjective,
    /// Adverb
    Adverb,
    /// Pronoun
    Pronoun,
    /// Preposition
    Preposition,
    /// Conjunction
    Conjunction,
    /// Determiner
    Determiner,
    /// Number
    Number,
    /// Punctuation
    Punctuation,
    /// Unknown
    Unknown,
}

/// Sentiment analysis result
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Sentiment {
    /// Positive score (0.0 to 1.0)
    pub positive: f64,
    /// Negative score (0.0 to 1.0)
    pub negative: f64,
    /// Neutral score (0.0 to 1.0)
    pub neutral: f64,
}

impl Sentiment {
    /// Creates a new sentiment
    pub fn new(positive: f64, negative: f64, neutral: f64) -> Self {
        Self {
            positive,
            negative,
            neutral,
        }
    }

    /// Returns the dominant sentiment
    pub fn dominant(&self) -> &'static str {
        if self.positive > self.negative && self.positive > self.neutral {
            "positive"
        } else if self.negative > self.positive && self.negative > self.neutral {
            "negative"
        } else {
            "neutral"
        }
    }

    /// Returns the overall score (-1.0 to 1.0)
    pub fn score(&self) -> f64 {
        self.positive - self.negative
    }
}

/// Parsed query structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedQuery {
    /// Original query text
    pub text: String,
    /// Normalized text
    pub normalized: String,
    /// Extracted tokens
    pub tokens: Vec<Token>,
    /// Extracted entities
    pub entities: Vec<super::entity::Entity>,
    /// Sentiment analysis result
    pub sentiment: Option<Sentiment>,
    /// Detected language
    pub language: Option<String>,
}

impl ParsedQuery {
    /// Creates a new parsed query
    pub fn new(text: String) -> Self {
        Self {
            normalized: text.clone(),
            text,
            tokens: Vec::new(),
            entities: Vec::new(),
            sentiment: None,
            language: None,
        }
    }

    /// Sets the normalized text
    pub fn with_normalized(mut self, normalized: String) -> Self {
        self.normalized = normalized;
        self
    }

    /// Adds tokens
    pub fn with_tokens(mut self, tokens: Vec<Token>) -> Self {
        self.tokens = tokens;
        self
    }

    /// Adds entities
    pub fn with_entities(mut self, entities: Vec<super::entity::Entity>) -> Self {
        self.entities = entities;
        self
    }

    /// Sets sentiment
    pub fn with_sentiment(mut self, sentiment: Sentiment) -> Self {
        self.sentiment = Some(sentiment);
        self
    }

    /// Sets language
    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    /// Returns verbs in the query
    pub fn verbs(&self) -> Vec<&Token> {
        self.tokens
            .iter()
            .filter(|t| t.pos == PartOfSpeech::Verb)
            .collect()
    }

    /// Returns nouns in the query
    pub fn nouns(&self) -> Vec<&Token> {
        self.tokens
            .iter()
            .filter(|t| t.pos == PartOfSpeech::Noun)
            .collect()
    }
}

/// Query parser trait
#[async_trait]
pub trait QueryParser: Send + Sync {
    /// Parses a natural language query
    async fn parse(&self, query: &str) -> Result<ParsedQuery>;

    /// Normalizes text (lowercasing, removing punctuation, etc.)
    fn normalize(&self, text: &str) -> String;

    /// Tokenizes text
    fn tokenize(&self, text: &str) -> Result<Vec<Token>>;

    /// Detects language
    fn detect_language(&self, text: &str) -> Option<String>;
}

/// Default implementation of QueryParser
#[derive(Debug, Clone)]
pub struct DefaultQueryParser {
    /// Enable sentiment analysis
    enable_sentiment: bool,
    /// Enable language detection
    enable_language_detection: bool,
}

impl DefaultQueryParser {
    /// Creates a new default query parser
    pub fn new() -> Self {
        Self {
            enable_sentiment: true,
            enable_language_detection: true,
        }
    }

    /// Sets sentiment analysis
    pub fn with_sentiment(mut self, enable: bool) -> Self {
        self.enable_sentiment = enable;
        self
    }

    /// Sets language detection
    pub fn with_language_detection(mut self, enable: bool) -> Self {
        self.enable_language_detection = enable;
        self
    }

    /// Simple sentiment analysis
    fn analyze_sentiment(&self, text: &str) -> Sentiment {
        let text_lower = text.to_lowercase();

        // Simple keyword-based sentiment analysis
        let positive_words = [
            "good",
            "great",
            "excellent",
            "wonderful",
            "amazing",
            "良い",
            "素晴らしい",
            "最高",
        ];
        let negative_words = [
            "bad",
            "terrible",
            "awful",
            "poor",
            "悪い",
            "ひどい",
            "最悪",
        ];

        let mut positive_count = 0;
        let mut negative_count = 0;

        for word in &positive_words {
            if text_lower.contains(word) {
                positive_count += 1;
            }
        }

        for word in &negative_words {
            if text_lower.contains(word) {
                negative_count += 1;
            }
        }

        let total = (positive_count + negative_count) as f64;
        if total == 0.0 {
            Sentiment::new(0.0, 0.0, 1.0)
        } else {
            let pos = positive_count as f64 / total;
            let neg = negative_count as f64 / total;
            let neu = 1.0 - pos - neg;
            Sentiment::new(pos, neg, neu.max(0.0))
        }
    }

    /// Simple language detection
    fn simple_detect_language(&self, text: &str) -> Option<String> {
        // Check for Japanese characters
        let has_hiragana = text.chars().any(|c| ('\u{3040}'..='\u{309F}').contains(&c));
        let has_katakana = text.chars().any(|c| ('\u{30A0}'..='\u{30FF}').contains(&c));
        let has_kanji = text
            .chars()
            .any(|c| ('\u{4E00}'..='\u{9FAF}').contains(&c));

        if has_hiragana || has_katakana || has_kanji {
            Some("ja".to_string())
        } else {
            // Assume English for Latin characters
            if text.chars().any(|c| c.is_ascii_alphabetic()) {
                Some("en".to_string())
            } else {
                None
            }
        }
    }

    /// Simple part-of-speech tagging
    fn tag_pos(&self, text: &str, _lang: Option<&str>) -> PartOfSpeech {
        let text_lower = text.to_lowercase();

        // Common verbs
        let verbs = [
            "create",
            "update",
            "delete",
            "get",
            "list",
            "search",
            "作成",
            "更新",
            "削除",
            "取得",
            "検索",
        ];
        if verbs.iter().any(|v| text_lower.contains(v)) {
            return PartOfSpeech::Verb;
        }

        // Common nouns
        let nouns = [
            "post",
            "page",
            "user",
            "comment",
            "投稿",
            "ページ",
            "ユーザー",
            "コメント",
        ];
        if nouns.iter().any(|n| text_lower.contains(n)) {
            return PartOfSpeech::Noun;
        }

        // Numbers
        if text.chars().all(|c| c.is_numeric()) {
            return PartOfSpeech::Number;
        }

        // Punctuation
        if text.chars().all(|c| c.is_ascii_punctuation()) {
            return PartOfSpeech::Punctuation;
        }

        // Default to unknown
        PartOfSpeech::Unknown
    }
}

impl Default for DefaultQueryParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QueryParser for DefaultQueryParser {
    async fn parse(&self, query: &str) -> Result<ParsedQuery> {
        let normalized = self.normalize(query);
        let tokens = self.tokenize(&normalized)?;

        let mut parsed = ParsedQuery::new(query.to_string()).with_normalized(normalized);

        // Add tokens
        parsed = parsed.with_tokens(tokens);

        // Detect language
        if self.enable_language_detection {
            if let Some(lang) = self.detect_language(query) {
                parsed = parsed.with_language(lang);
            }
        }

        // Analyze sentiment
        if self.enable_sentiment {
            let sentiment = self.analyze_sentiment(query);
            parsed = parsed.with_sentiment(sentiment);
        }

        Ok(parsed)
    }

    fn normalize(&self, text: &str) -> String {
        text.trim()
            .chars()
            .map(|c| {
                if c.is_whitespace() {
                    ' '
                } else {
                    c
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn tokenize(&self, text: &str) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        let mut start = 0;

        // Simple whitespace tokenization
        for word in text.split_whitespace() {
            let end = start + word.len();
            let pos = self.tag_pos(word, None);
            tokens.push(Token::new(word.to_string(), pos, start, end));
            start = end + 1;
        }

        Ok(tokens)
    }

    fn detect_language(&self, text: &str) -> Option<String> {
        self.simple_detect_language(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let token = Token::new("test".to_string(), PartOfSpeech::Noun, 0, 4);
        assert_eq!(token.text, "test");
        assert_eq!(token.pos, PartOfSpeech::Noun);
    }

    #[test]
    fn test_sentiment_dominant() {
        let sentiment = Sentiment::new(0.8, 0.1, 0.1);
        assert_eq!(sentiment.dominant(), "positive");

        let sentiment = Sentiment::new(0.1, 0.8, 0.1);
        assert_eq!(sentiment.dominant(), "negative");

        let sentiment = Sentiment::new(0.1, 0.1, 0.8);
        assert_eq!(sentiment.dominant(), "neutral");
    }

    #[tokio::test]
    async fn test_parser_normalize() {
        let parser = DefaultQueryParser::new();
        let normalized = parser.normalize("  hello   world  ");
        assert_eq!(normalized, "hello world");
    }

    #[tokio::test]
    async fn test_parser_tokenize() {
        let parser = DefaultQueryParser::new();
        let tokens = parser.tokenize("create new post").unwrap();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text, "create");
    }

    #[tokio::test]
    async fn test_language_detection() {
        let parser = DefaultQueryParser::new();
        assert_eq!(parser.detect_language("こんにちは"), Some("ja".to_string()));
        assert_eq!(parser.detect_language("hello"), Some("en".to_string()));
    }
}
