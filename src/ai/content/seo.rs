//! SEO Analyzer
//!
//! コンテンツのSEO最適化分析機能

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SEOスコア
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeoScore {
    /// 総合スコア (0.0-1.0)
    pub overall: f64,
    /// キーワード密度スコア
    pub keyword_density: f64,
    /// 読みやすさスコア
    pub readability: f64,
    /// 構造品質スコア
    pub structure_quality: f64,
    /// 改善提案
    pub suggestions: Vec<SeoSuggestion>,
}

impl SeoScore {
    /// スコアを作成
    pub fn new() -> Self {
        Self {
            overall: 0.0,
            keyword_density: 0.0,
            readability: 0.0,
            structure_quality: 0.0,
            suggestions: vec![],
        }
    }

    /// 総合スコアを計算
    pub fn calculate_overall(&mut self) {
        self.overall = (self.keyword_density + self.readability + self.structure_quality) / 3.0;
    }

    /// スコアグレードを取得
    pub fn grade(&self) -> &str {
        match (self.overall * 100.0) as u8 {
            90..=100 => "Excellent",
            80..=89 => "Good",
            70..=79 => "Fair",
            60..=69 => "Poor",
            _ => "Very Poor",
        }
    }
}

impl Default for SeoScore {
    fn default() -> Self {
        Self::new()
    }
}

/// SEO改善提案
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeoSuggestion {
    /// カテゴリ
    pub category: SuggestionCategory,
    /// 重要度 (1-5)
    pub priority: u8,
    /// メッセージ
    pub message: String,
    /// 詳細説明
    pub details: Option<String>,
}

/// 提案カテゴリ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SuggestionCategory {
    /// キーワード
    Keywords,
    /// 見出し
    Headings,
    /// 内部リンク
    InternalLinks,
    /// 読みやすさ
    Readability,
    /// メタデータ
    Metadata,
    /// コンテンツ長
    ContentLength,
}

/// SEOアナライザートレイト
pub trait SeoAnalyzer: Send + Sync {
    /// コンテンツを分析
    fn analyze(&self, content: &str) -> SeoScore;

    /// 改善提案を生成
    fn suggest_improvements(&self, content: &str) -> Vec<SeoSuggestion>;

    /// キーワード密度を計算
    fn calculate_keyword_density(&self, content: &str, keywords: &[String])
        -> HashMap<String, f64>;

    /// 読みやすさスコアを計算
    fn calculate_readability(&self, content: &str) -> f64;
}

/// デフォルトSEOアナライザー
pub struct DefaultSeoAnalyzer {
    target_keyword_density: f64,
    min_word_count: usize,
    max_word_count: usize,
}

impl DefaultSeoAnalyzer {
    /// 新規アナライザーを作成
    pub fn new() -> Self {
        Self {
            target_keyword_density: 0.02, // 2%
            min_word_count: 300,
            max_word_count: 2000,
        }
    }

    /// 単語数をカウント
    fn count_words(&self, content: &str) -> usize {
        content.split_whitespace().count()
    }

    /// 文の数をカウント
    fn count_sentences(&self, content: &str) -> usize {
        content
            .chars()
            .filter(|c| matches!(c, '.' | '!' | '?' | '。' | '！' | '？'))
            .count()
            .max(1)
    }

    /// 段落数をカウント
    fn count_paragraphs(&self, content: &str) -> usize {
        content
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count()
            .max(1)
    }

    /// 見出しを抽出
    fn extract_headings(&self, content: &str) -> Vec<String> {
        content
            .lines()
            .filter(|line| line.starts_with('#'))
            .map(|line| line.to_string())
            .collect()
    }

    /// Flesch Reading Easeスコアを計算（簡易版）
    fn flesch_reading_ease(&self, content: &str) -> f64 {
        let word_count = self.count_words(content);
        let sentence_count = self.count_sentences(content);

        if word_count == 0 || sentence_count == 0 {
            return 0.0;
        }

        let avg_sentence_length = word_count as f64 / sentence_count as f64;

        // 簡易版（英語向けの計算式を日本語用に調整）
        // 実際にはより洗練された日本語読みやすさ指標が必要
        let score = 206.835 - (1.015 * avg_sentence_length);

        score.clamp(0.0, 100.0)
    }

    /// 構造品質を評価
    fn evaluate_structure(&self, content: &str) -> (f64, Vec<SeoSuggestion>) {
        let mut score: f64 = 1.0;
        let mut suggestions = Vec::new();

        let headings = self.extract_headings(content);
        let word_count = self.count_words(content);
        let paragraph_count = self.count_paragraphs(content);

        // 見出しチェック
        if headings.is_empty() {
            score -= 0.3;
            suggestions.push(SeoSuggestion {
                category: SuggestionCategory::Headings,
                priority: 5,
                message: "見出し（H1、H2など）を追加してください".to_string(),
                details: Some("見出しはコンテンツの構造を明確にし、SEOに重要です".to_string()),
            });
        } else if headings.len() < 3 {
            score -= 0.1;
            suggestions.push(SeoSuggestion {
                category: SuggestionCategory::Headings,
                priority: 3,
                message: "より多くの見出しでコンテンツを構造化してください".to_string(),
                details: None,
            });
        }

        // 段落の長さチェック
        let avg_paragraph_length = word_count as f64 / paragraph_count as f64;
        if avg_paragraph_length > 150.0 {
            score -= 0.2;
            suggestions.push(SeoSuggestion {
                category: SuggestionCategory::Readability,
                priority: 3,
                message: "段落が長すぎます。より短い段落に分割してください".to_string(),
                details: Some("1段落あたり100-150語が理想的です".to_string()),
            });
        }

        // コンテンツ長チェック
        if word_count < self.min_word_count {
            score -= 0.2;
            suggestions.push(SeoSuggestion {
                category: SuggestionCategory::ContentLength,
                priority: 4,
                message: format!(
                    "コンテンツが短すぎます（{}語）。最低{}語を推奨",
                    word_count, self.min_word_count
                ),
                details: None,
            });
        } else if word_count > self.max_word_count {
            suggestions.push(SeoSuggestion {
                category: SuggestionCategory::ContentLength,
                priority: 2,
                message: format!("コンテンツが長すぎる可能性があります（{}語）", word_count),
                details: Some("複数の記事に分割することを検討してください".to_string()),
            });
        }

        (score.max(0.0), suggestions)
    }
}

impl Default for DefaultSeoAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SeoAnalyzer for DefaultSeoAnalyzer {
    fn analyze(&self, content: &str) -> SeoScore {
        let mut score = SeoScore::new();

        // 読みやすさスコア
        let readability_raw = self.flesch_reading_ease(content);
        score.readability = (readability_raw / 100.0).clamp(0.0, 1.0);

        // 構造品質スコア
        let (structure_score, structure_suggestions) = self.evaluate_structure(content);
        score.structure_quality = structure_score;
        score.suggestions.extend(structure_suggestions);

        // キーワード密度スコア（基本的なチェック）
        let word_count = self.count_words(content);
        if word_count >= self.min_word_count {
            score.keyword_density = 0.7; // 基本スコア
        } else {
            score.keyword_density = 0.3;
        }

        // 総合スコア計算
        score.calculate_overall();

        // 読みやすさの提案
        if score.readability < 0.5 {
            score.suggestions.push(SeoSuggestion {
                category: SuggestionCategory::Readability,
                priority: 4,
                message: "文章の読みやすさを改善してください".to_string(),
                details: Some("短い文と明確な表現を使用してください".to_string()),
            });
        }

        score
    }

    fn suggest_improvements(&self, content: &str) -> Vec<SeoSuggestion> {
        let score = self.analyze(content);
        score.suggestions
    }

    fn calculate_keyword_density(
        &self,
        content: &str,
        keywords: &[String],
    ) -> HashMap<String, f64> {
        let content_lower = content.to_lowercase();
        let total_words = self.count_words(content) as f64;

        keywords
            .iter()
            .map(|keyword| {
                let keyword_lower = keyword.to_lowercase();
                let count = content_lower.matches(&keyword_lower).count() as f64;
                let density = if total_words > 0.0 {
                    count / total_words
                } else {
                    0.0
                };
                (keyword.clone(), density)
            })
            .collect()
    }

    fn calculate_readability(&self, content: &str) -> f64 {
        self.flesch_reading_ease(content) / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seo_score_grade() {
        let mut score = SeoScore::new();

        score.overall = 0.95;
        assert_eq!(score.grade(), "Excellent");

        score.overall = 0.85;
        assert_eq!(score.grade(), "Good");

        score.overall = 0.75;
        assert_eq!(score.grade(), "Fair");

        score.overall = 0.65;
        assert_eq!(score.grade(), "Poor");

        score.overall = 0.50;
        assert_eq!(score.grade(), "Very Poor");
    }

    #[test]
    fn test_keyword_density() {
        let analyzer = DefaultSeoAnalyzer::new();
        let content = "Rust is great. Rust programming is fun. I love Rust.";
        let keywords = vec!["Rust".to_string(), "programming".to_string()];

        let densities = analyzer.calculate_keyword_density(content, &keywords);

        assert!(densities.contains_key("Rust"));
        assert!(densities.contains_key("programming"));
        assert!(densities["Rust"] > densities["programming"]);
    }

    #[test]
    fn test_seo_analyzer() {
        let analyzer = DefaultSeoAnalyzer::new();
        let content = r#"
# Test Article

This is a test article with multiple paragraphs.

## Section 1

Lorem ipsum dolor sit amet, consectetur adipiscing elit.
Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

## Section 2

Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.
"#;

        let score = analyzer.analyze(content);
        assert!(score.overall > 0.0);
        assert!(score.overall <= 1.0);
    }
}
