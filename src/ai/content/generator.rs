//! Content Generator
//!
//! LLMを活用した動的コンテンツ生成機能

use crate::ai::llm::{LlmClient, LlmProvider};
use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// コンテンツのトーン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tone {
    /// フォーマル
    Formal,
    /// カジュアル
    Casual,
    /// 技術的
    Technical,
    /// 親しみやすい
    Friendly,
    /// 専門的
    Professional,
}

/// コンテンツの長さ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentLength {
    /// 短い（~300語）
    Short,
    /// 中程度（~800語）
    Medium,
    /// 長い（~1500語）
    Long,
    /// カスタム
    Custom(usize),
}

impl ContentLength {
    /// 推定トークン数を取得
    pub fn estimated_tokens(&self) -> usize {
        match self {
            ContentLength::Short => 400,
            ContentLength::Medium => 1000,
            ContentLength::Long => 2000,
            ContentLength::Custom(words) => (words * 4) / 3, // 1語 ≈ 1.33トークン
        }
    }
}

/// コンテンツ生成プロンプト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPrompt {
    /// トピック
    pub topic: String,
    /// トーン
    pub tone: Tone,
    /// 長さ
    pub length: ContentLength,
    /// キーワード
    pub keywords: Vec<String>,
    /// ターゲットオーディエンス
    pub audience: Option<String>,
    /// 追加要件
    pub requirements: Option<Vec<String>>,
}

impl ContentPrompt {
    /// 新規プロンプトを作成
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            tone: Tone::Professional,
            length: ContentLength::Medium,
            keywords: vec![],
            audience: None,
            requirements: None,
        }
    }

    /// トーンを設定
    pub fn with_tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// 長さを設定
    pub fn with_length(mut self, length: ContentLength) -> Self {
        self.length = length;
        self
    }

    /// キーワードを設定
    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// オーディエンスを設定
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    /// LLMプロンプトに変換
    pub fn to_llm_prompt(&self) -> String {
        let mut prompt = "# コンテンツ生成リクエスト\n\n".to_string();
        prompt.push_str(&format!("**トピック**: {}\n\n", self.topic));

        let tone_str = match self.tone {
            Tone::Formal => "フォーマル",
            Tone::Casual => "カジュアル",
            Tone::Technical => "技術的",
            Tone::Friendly => "親しみやすい",
            Tone::Professional => "専門的",
        };
        prompt.push_str(&format!("**トーン**: {}\n\n", tone_str));

        let length_str = match &self.length {
            ContentLength::Short => "短い（約300語）",
            ContentLength::Medium => "中程度（約800語）",
            ContentLength::Long => "長い（約1500語）",
            ContentLength::Custom(n) => &format!("カスタム（約{}語）", n),
        };
        prompt.push_str(&format!("**長さ**: {}\n\n", length_str));

        if !self.keywords.is_empty() {
            prompt.push_str(&format!("**キーワード**: {}\n\n", self.keywords.join(", ")));
        }

        if let Some(audience) = &self.audience {
            prompt.push_str(&format!("**ターゲットオーディエンス**: {}\n\n", audience));
        }

        if let Some(requirements) = &self.requirements {
            prompt.push_str("**追加要件**:\n");
            for req in requirements {
                prompt.push_str(&format!("- {}\n", req));
            }
            prompt.push('\n');
        }

        prompt.push_str("## 要求される出力\n\n");
        prompt.push_str("以下の形式でコンテンツを生成してください:\n\n");
        prompt.push_str("1. **タイトル**: 魅力的で検索エンジンに最適化されたタイトル\n");
        prompt.push_str("2. **本文**: 上記の要件に従った本文\n");
        prompt.push_str("3. **抜粋**: 2-3文の簡潔な要約\n");
        prompt.push_str("4. **メタディスクリプション**: SEO最適化された155文字以内の説明\n");
        prompt.push_str("5. **推奨タグ**: 5-10個の関連タグ\n");
        prompt.push_str("6. **推奨カテゴリ**: 1-3個の適切なカテゴリ\n");

        prompt
    }
}

/// 生成されたコンテンツ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedContent {
    /// タイトル
    pub title: String,
    /// 本文
    pub body: String,
    /// 抜粋
    pub excerpt: String,
    /// メタディスクリプション
    pub meta_description: String,
    /// タグ
    pub tags: Vec<String>,
    /// カテゴリ
    pub categories: Vec<String>,
    /// 生成メタデータ
    pub metadata: HashMap<String, String>,
}

/// コンテンツリファイン要件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineRequirements {
    /// トーン調整
    pub adjust_tone: Option<Tone>,
    /// 長さ調整
    pub adjust_length: Option<ContentLength>,
    /// キーワード密度の目標
    pub target_keyword_density: Option<f64>,
    /// 追加する要素
    pub add_elements: Option<Vec<String>>,
    /// 削除する要素
    pub remove_elements: Option<Vec<String>>,
}

/// コンテンツ生成トレイト
#[async_trait]
pub trait ContentGenerator: Send + Sync {
    /// コンテンツを生成
    async fn generate(&self, prompt: &ContentPrompt) -> Result<GeneratedContent>;

    /// コンテンツを洗練
    async fn refine(&self, content: &str, requirements: &RefineRequirements) -> Result<String>;

    /// タイトルを生成
    async fn generate_title(&self, content: &str, keywords: &[String]) -> Result<String>;

    /// メタディスクリプションを生成
    async fn generate_meta_description(&self, content: &str) -> Result<String>;

    /// タグを提案
    async fn suggest_tags(&self, content: &str, count: usize) -> Result<Vec<String>>;
}

/// デフォルトコンテンツジェネレーター
pub struct DefaultContentGenerator {
    llm_client: Box<dyn LlmClient>,
}

impl DefaultContentGenerator {
    /// 新規ジェネレーターを作成
    pub fn new(llm_client: impl LlmClient + 'static) -> Self {
        Self {
            llm_client: Box::new(llm_client),
        }
    }

    /// LLMレスポンスをパース
    fn parse_generated_content(&self, response: &str) -> Result<GeneratedContent> {
        // 簡易的なパーサー（実際にはより堅牢な実装が必要）
        let mut title = String::new();
        let mut excerpt = String::new();
        let mut meta_description = String::new();
        let mut tags = Vec::new();
        let mut categories = Vec::new();

        let lines: Vec<&str> = response.lines().collect();
        let mut current_section = "";
        let mut body_lines = Vec::new();

        for line in lines {
            if line.starts_with("**タイトル**:") || line.starts_with("1. **タイトル**:") {
                current_section = "title";
                title = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("**本文**:") || line.starts_with("2. **本文**:") {
                current_section = "body";
            } else if line.starts_with("**抜粋**:") || line.starts_with("3. **抜粋**:") {
                current_section = "excerpt";
                excerpt = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("**メタディスクリプション**:")
                || line.starts_with("4. **メタディスクリプション**:")
            {
                current_section = "meta";
                meta_description = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("**推奨タグ**:") || line.starts_with("5. **推奨タグ**:")
            {
                current_section = "tags";
                let tag_str = line.split(':').nth(1).unwrap_or("").trim();
                tags = tag_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if line.starts_with("**推奨カテゴリ**:")
                || line.starts_with("6. **推奨カテゴリ**:")
            {
                current_section = "categories";
                let cat_str = line.split(':').nth(1).unwrap_or("").trim();
                categories = cat_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if current_section == "body" && !line.trim().is_empty() {
                body_lines.push(line);
            }
        }

        let body = body_lines.join("\n");

        if title.is_empty() {
            return Err(Error::Parse(
                "Failed to parse title from generated content".to_string(),
            ));
        }

        Ok(GeneratedContent {
            title,
            body,
            excerpt,
            meta_description,
            tags,
            categories,
            metadata: HashMap::new(),
        })
    }
}

#[async_trait]
impl ContentGenerator for DefaultContentGenerator {
    async fn generate(&self, prompt: &ContentPrompt) -> Result<GeneratedContent> {
        let llm_prompt = prompt.to_llm_prompt();

        let response = self.llm_client.generate(&llm_prompt).await?;

        self.parse_generated_content(&response.content)
    }

    async fn refine(&self, content: &str, requirements: &RefineRequirements) -> Result<String> {
        let mut prompt = "以下のコンテンツを以下の要件に従って改善してください:\n\n".to_string();
        prompt.push_str(&format!("## 元のコンテンツ\n\n{}\n\n", content));
        prompt.push_str("## 改善要件\n\n");

        if let Some(tone) = &requirements.adjust_tone {
            let tone_str = match tone {
                Tone::Formal => "フォーマル",
                Tone::Casual => "カジュアル",
                Tone::Technical => "技術的",
                Tone::Friendly => "親しみやすい",
                Tone::Professional => "専門的",
            };
            prompt.push_str(&format!("- トーンを{}に調整\n", tone_str));
        }

        if let Some(length) = &requirements.adjust_length {
            let length_str = match length {
                ContentLength::Short => "短く（約300語）",
                ContentLength::Medium => "中程度（約800語）",
                ContentLength::Long => "長く（約1500語）",
                ContentLength::Custom(n) => &format!("約{}語", n),
            };
            prompt.push_str(&format!("- 長さを{}に調整\n", length_str));
        }

        if let Some(density) = requirements.target_keyword_density {
            prompt.push_str(&format!(
                "- キーワード密度を{:.1}%に調整\n",
                density * 100.0
            ));
        }

        if let Some(add) = &requirements.add_elements {
            prompt.push_str("- 以下の要素を追加:\n");
            for element in add {
                prompt.push_str(&format!("  * {}\n", element));
            }
        }

        if let Some(remove) = &requirements.remove_elements {
            prompt.push_str("- 以下の要素を削除:\n");
            for element in remove {
                prompt.push_str(&format!("  * {}\n", element));
            }
        }

        let response = self.llm_client.generate(&prompt).await?;
        Ok(response.content)
    }

    async fn generate_title(&self, content: &str, keywords: &[String]) -> Result<String> {
        let prompt = format!(
            "以下のコンテンツに対して、魅力的でSEOに最適化されたタイトルを生成してください。\n\n\
             キーワード: {}\n\n\
             コンテンツ:\n{}\n\n\
             タイトルのみを出力してください。",
            keywords.join(", "),
            content
        );

        let response = self.llm_client.generate(&prompt).await?;
        Ok(response.content.trim().to_string())
    }

    async fn generate_meta_description(&self, content: &str) -> Result<String> {
        let prompt = format!(
            "以下のコンテンツに対して、SEO最適化された155文字以内のメタディスクリプションを生成してください。\n\n\
             コンテンツ:\n{}\n\n\
             メタディスクリプションのみを出力してください。",
            content
        );

        let response = self.llm_client.generate(&prompt).await?;
        Ok(response.content.trim().to_string())
    }

    async fn suggest_tags(&self, content: &str, count: usize) -> Result<Vec<String>> {
        let prompt = format!(
            "以下のコンテンツに対して、{}個の関連タグを提案してください。\n\n\
             コンテンツ:\n{}\n\n\
             タグをカンマ区切りで出力してください。",
            count, content
        );

        let response = self.llm_client.generate(&prompt).await?;
        let tags = response
            .content
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .take(count)
            .collect();

        Ok(tags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_prompt_builder() {
        let prompt = ContentPrompt::new("Rustプログラミング")
            .with_tone(Tone::Technical)
            .with_length(ContentLength::Long)
            .with_keywords(vec!["Rust".to_string(), "所有権".to_string()])
            .with_audience("中級開発者");

        assert_eq!(prompt.topic, "Rustプログラミング");
        assert!(matches!(prompt.tone, Tone::Technical));
        assert!(matches!(prompt.length, ContentLength::Long));
        assert_eq!(prompt.keywords.len(), 2);
        assert_eq!(prompt.audience, Some("中級開発者".to_string()));
    }

    #[test]
    fn test_content_length_tokens() {
        assert_eq!(ContentLength::Short.estimated_tokens(), 400);
        assert_eq!(ContentLength::Medium.estimated_tokens(), 1000);
        assert_eq!(ContentLength::Long.estimated_tokens(), 2000);
        assert_eq!(ContentLength::Custom(1200).estimated_tokens(), 1600);
    }
}
