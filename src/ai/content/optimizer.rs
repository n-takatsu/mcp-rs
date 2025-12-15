//! Content Optimizer
//!
//! コンテンツの総合的な最適化機能

use super::generator::{ContentGenerator, RefineRequirements};
use super::seo::{SeoAnalyzer, SeoScore};
use super::template::TemplateEngine;
use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 最適化オプション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationOptions {
    /// SEO最適化を有効化
    pub enable_seo: bool,
    /// 読みやすさ最適化を有効化
    pub enable_readability: bool,
    /// キーワード最適化を有効化
    pub enable_keywords: bool,
    /// ターゲットキーワード
    pub target_keywords: Vec<String>,
    /// 目標SEOスコア (0.0-1.0)
    pub target_seo_score: f64,
    /// 最大最適化イテレーション
    pub max_iterations: usize,
}

impl OptimizationOptions {
    /// デフォルトオプションを作成
    pub fn new() -> Self {
        Self {
            enable_seo: true,
            enable_readability: true,
            enable_keywords: true,
            target_keywords: vec![],
            target_seo_score: 0.8,
            max_iterations: 3,
        }
    }

    /// キーワードを設定
    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.target_keywords = keywords;
        self
    }

    /// 目標スコアを設定
    pub fn with_target_score(mut self, score: f64) -> Self {
        self.target_seo_score = score.clamp(0.0, 1.0);
        self
    }
}

impl Default for OptimizationOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// 最適化結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    /// 最適化されたコンテンツ
    pub optimized_content: String,
    /// 初期SEOスコア
    pub initial_score: SeoScore,
    /// 最終SEOスコア
    pub final_score: SeoScore,
    /// 実行したイテレーション数
    pub iterations: usize,
    /// 適用された改善内容
    pub improvements: Vec<String>,
}

/// コンテンツオプティマイザー
#[async_trait]
pub trait ContentOptimizer: Send + Sync {
    /// コンテンツを最適化
    async fn optimize(
        &self,
        content: &str,
        options: &OptimizationOptions,
    ) -> Result<OptimizationResult>;

    /// SEOスコアに基づいて改善
    async fn improve_seo(&self, content: &str, current_score: &SeoScore) -> Result<String>;

    /// キーワード密度を最適化
    async fn optimize_keywords(&self, content: &str, keywords: &[String]) -> Result<String>;
}

/// デフォルトコンテンツオプティマイザー
pub struct DefaultContentOptimizer<G, S>
where
    G: ContentGenerator,
    S: SeoAnalyzer,
{
    generator: G,
    seo_analyzer: S,
}

impl<G, S> DefaultContentOptimizer<G, S>
where
    G: ContentGenerator,
    S: SeoAnalyzer,
{
    /// 新規オプティマイザーを作成
    pub fn new(generator: G, seo_analyzer: S) -> Self {
        Self {
            generator,
            seo_analyzer,
        }
    }

    /// 改善が必要な領域を特定
    fn identify_improvement_areas(&self, score: &SeoScore) -> Vec<String> {
        let mut areas = Vec::new();

        if score.keyword_density < 0.6 {
            areas.push("キーワード密度の改善".to_string());
        }

        if score.readability < 0.6 {
            areas.push("読みやすさの改善".to_string());
        }

        if score.structure_quality < 0.6 {
            areas.push("コンテンツ構造の改善".to_string());
        }

        // 高優先度の提案を追加
        for suggestion in &score.suggestions {
            if suggestion.priority >= 4 {
                areas.push(suggestion.message.clone());
            }
        }

        areas
    }
}

#[async_trait]
impl<G, S> ContentOptimizer for DefaultContentOptimizer<G, S>
where
    G: ContentGenerator + Send + Sync,
    S: SeoAnalyzer + Send + Sync,
{
    async fn optimize(
        &self,
        content: &str,
        options: &OptimizationOptions,
    ) -> Result<OptimizationResult> {
        // 初期スコアを計算
        let initial_score = self.seo_analyzer.analyze(content);
        let mut current_content = content.to_string();
        let mut current_score = initial_score.clone();
        let mut improvements = Vec::new();

        // 最適化イテレーション
        for iteration in 0..options.max_iterations {
            // 目標スコアに達したら終了
            if current_score.overall >= options.target_seo_score {
                break;
            }

            // 改善領域を特定
            let improvement_areas = self.identify_improvement_areas(&current_score);
            if improvement_areas.is_empty() {
                break;
            }

            // SEO改善
            if options.enable_seo && current_score.overall < options.target_seo_score {
                current_content = self.improve_seo(&current_content, &current_score).await?;
                improvements.push(format!("イテレーション {}: SEO改善", iteration + 1));
            }

            // キーワード最適化
            if options.enable_keywords && !options.target_keywords.is_empty() {
                current_content = self
                    .optimize_keywords(&current_content, &options.target_keywords)
                    .await?;
                improvements.push(format!(
                    "イテレーション {}: キーワード最適化",
                    iteration + 1
                ));
            }

            // スコアを再計算
            current_score = self.seo_analyzer.analyze(&current_content);
        }

        Ok(OptimizationResult {
            optimized_content: current_content,
            initial_score,
            final_score: current_score,
            iterations: improvements.len(),
            improvements,
        })
    }

    async fn improve_seo(&self, content: &str, current_score: &SeoScore) -> Result<String> {
        let mut requirements = RefineRequirements {
            adjust_tone: None,
            adjust_length: None,
            target_keyword_density: None,
            add_elements: Some(Vec::new()),
            remove_elements: None,
        };

        // SEO提案に基づいて要件を設定
        for suggestion in &current_score.suggestions {
            if suggestion.priority >= 3 {
                if let Some(add_elements) = &mut requirements.add_elements {
                    add_elements.push(suggestion.message.clone());
                }
            }
        }

        self.generator.refine(content, &requirements).await
    }

    async fn optimize_keywords(&self, content: &str, keywords: &[String]) -> Result<String> {
        let keyword_densities = self
            .seo_analyzer
            .calculate_keyword_density(content, keywords);

        // キーワード密度が低い場合、コンテンツにキーワードを自然に組み込む
        let requirements = RefineRequirements {
            adjust_tone: None,
            adjust_length: None,
            target_keyword_density: Some(0.02), // 2%目標
            add_elements: Some(
                keywords
                    .iter()
                    .filter(|k| keyword_densities.get(*k).unwrap_or(&0.0) < &0.01)
                    .map(|k| format!("「{}」というキーワードを自然に組み込む", k))
                    .collect(),
            ),
            remove_elements: None,
        };

        self.generator.refine(content, &requirements).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::content::generator::{ContentGenerator, DefaultContentGenerator, Tone};
    use crate::ai::content::seo::DefaultSeoAnalyzer;
    use crate::ai::llm::{LlmClient, LlmConfig, LlmProvider};

    #[test]
    fn test_optimization_options() {
        let options = OptimizationOptions::new()
            .with_keywords(vec!["Rust".to_string(), "programming".to_string()])
            .with_target_score(0.85);

        assert_eq!(options.target_keywords.len(), 2);
        assert_eq!(options.target_seo_score, 0.85);
    }

    #[test]
    fn test_optimization_options_score_bounds() {
        let options = OptimizationOptions::new().with_target_score(1.5);
        assert_eq!(options.target_seo_score, 1.0);

        let options = OptimizationOptions::new().with_target_score(-0.5);
        assert_eq!(options.target_seo_score, 0.0);
    }
}
