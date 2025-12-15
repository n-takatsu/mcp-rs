//! Template Engine
//!
//! コンテンツテンプレートの作成と管理機能

use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// テンプレートタイプ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TemplateType {
    /// ブログ記事
    BlogPost,
    /// 商品説明
    ProductDescription,
    /// ランディングページ
    LandingPage,
    /// ニュースレター
    Newsletter,
    /// プレスリリース
    PressRelease,
    /// ソーシャルメディア投稿
    SocialMediaPost,
    /// FAQエントリ
    FaqEntry,
    /// カスタムテンプレート
    Custom(String),
}

impl TemplateType {
    /// テンプレート名を取得
    pub fn name(&self) -> &str {
        match self {
            TemplateType::BlogPost => "blog_post",
            TemplateType::ProductDescription => "product_description",
            TemplateType::LandingPage => "landing_page",
            TemplateType::Newsletter => "newsletter",
            TemplateType::PressRelease => "press_release",
            TemplateType::SocialMediaPost => "social_media_post",
            TemplateType::FaqEntry => "faq_entry",
            TemplateType::Custom(name) => name,
        }
    }
}

/// コンテンツテンプレート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentTemplate {
    /// テンプレートタイプ
    pub template_type: TemplateType,
    /// テンプレート名
    pub name: String,
    /// テンプレート説明
    pub description: String,
    /// テンプレート本文
    pub template: String,
    /// 必須フィールド
    pub required_fields: Vec<String>,
    /// オプションフィールド
    pub optional_fields: Vec<String>,
    /// デフォルト値
    pub defaults: HashMap<String, String>,
}

impl ContentTemplate {
    /// 新規テンプレートを作成
    pub fn new(
        template_type: TemplateType,
        name: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        Self {
            template_type,
            name: name.into(),
            description: String::new(),
            template: template.into(),
            required_fields: vec![],
            optional_fields: vec![],
            defaults: HashMap::new(),
        }
    }

    /// 必須フィールドを検証
    pub fn validate_fields(&self, data: &HashMap<String, String>) -> Result<()> {
        for field in &self.required_fields {
            if !data.contains_key(field) {
                return Err(Error::Validation(format!(
                    "Required field '{}' is missing",
                    field
                )));
            }
        }
        Ok(())
    }

    /// デフォルト値を適用
    pub fn apply_defaults(&self, data: &mut HashMap<String, String>) {
        for (key, value) in &self.defaults {
            data.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
}

/// テンプレートエンジントレイト
#[async_trait]
pub trait TemplateEngine: Send + Sync {
    /// テンプレートを作成
    async fn create_template(&self, template_type: TemplateType) -> Result<ContentTemplate>;

    /// テンプレートを適用
    fn apply_template(
        &self,
        template: &ContentTemplate,
        data: &HashMap<String, String>,
    ) -> Result<String>;

    /// テンプレートをカスタマイズ
    async fn customize_template(
        &self,
        template: &ContentTemplate,
        customizations: &HashMap<String, String>,
    ) -> Result<ContentTemplate>;

    /// 使用可能なテンプレートを一覧表示
    fn list_templates(&self) -> Vec<TemplateType>;
}

/// デフォルトテンプレートエンジン
pub struct DefaultTemplateEngine {
    templates: HashMap<TemplateType, ContentTemplate>,
}

impl DefaultTemplateEngine {
    /// 新規テンプレートエンジンを作成
    pub fn new() -> Self {
        let mut templates = HashMap::new();

        // ブログ記事テンプレート
        let blog_template = ContentTemplate {
            template_type: TemplateType::BlogPost,
            name: "Standard Blog Post".to_string(),
            description: "標準的なブログ記事テンプレート".to_string(),
            template: r#"# {{title}}

{{#if author}}
*By {{author}}*
{{/if}}

{{#if date}}
*Published on {{date}}*
{{/if}}

## Introduction

{{introduction}}

## Main Content

{{content}}

{{#if conclusion}}
## Conclusion

{{conclusion}}
{{/if}}

{{#if tags}}
**Tags**: {{tags}}
{{/if}}

{{#if categories}}
**Categories**: {{categories}}
{{/if}}"#
                .to_string(),
            required_fields: vec!["title".to_string(), "content".to_string()],
            optional_fields: vec![
                "author".to_string(),
                "date".to_string(),
                "introduction".to_string(),
                "conclusion".to_string(),
                "tags".to_string(),
                "categories".to_string(),
            ],
            defaults: HashMap::new(),
        };
        templates.insert(TemplateType::BlogPost, blog_template);

        // 商品説明テンプレート
        let product_template = ContentTemplate {
            template_type: TemplateType::ProductDescription,
            name: "Product Description".to_string(),
            description: "商品説明テンプレート".to_string(),
            template: r#"# {{product_name}}

{{#if tagline}}
*{{tagline}}*
{{/if}}

## Description

{{description}}

{{#if features}}
## Key Features

{{features}}
{{/if}}

{{#if specifications}}
## Specifications

{{specifications}}
{{/if}}

{{#if price}}
**Price**: {{price}}
{{/if}}

{{#if availability}}
**Availability**: {{availability}}
{{/if}}"#
                .to_string(),
            required_fields: vec!["product_name".to_string(), "description".to_string()],
            optional_fields: vec![
                "tagline".to_string(),
                "features".to_string(),
                "specifications".to_string(),
                "price".to_string(),
                "availability".to_string(),
            ],
            defaults: HashMap::new(),
        };
        templates.insert(TemplateType::ProductDescription, product_template);

        // ランディングページテンプレート
        let landing_template = ContentTemplate {
            template_type: TemplateType::LandingPage,
            name: "Landing Page".to_string(),
            description: "ランディングページテンプレート".to_string(),
            template: r#"# {{headline}}

{{#if subheadline}}
## {{subheadline}}
{{/if}}

{{hero_content}}

{{#if benefits}}
## Why Choose Us

{{benefits}}
{{/if}}

{{#if features}}
## Features

{{features}}
{{/if}}

{{#if testimonials}}
## What Our Customers Say

{{testimonials}}
{{/if}}

{{#if cta}}
## {{cta}}
{{/if}}"#
                .to_string(),
            required_fields: vec!["headline".to_string(), "hero_content".to_string()],
            optional_fields: vec![
                "subheadline".to_string(),
                "benefits".to_string(),
                "features".to_string(),
                "testimonials".to_string(),
                "cta".to_string(),
            ],
            defaults: HashMap::new(),
        };
        templates.insert(TemplateType::LandingPage, landing_template);

        Self { templates }
    }

    /// テンプレートを登録
    pub fn register_template(&mut self, template: ContentTemplate) {
        self.templates
            .insert(template.template_type.clone(), template);
    }

    /// シンプルなテンプレート置換（Handlebarsの簡易版）
    fn simple_replace(&self, template: &str, data: &HashMap<String, String>) -> String {
        let mut result = template.to_string();

        // {{variable}} 形式の置換
        for (key, value) in data {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }

        // {{#if variable}}...{{/if}} の処理（簡易版）
        let if_pattern = regex::Regex::new(r"\{\{#if (\w+)\}\}(.*?)\{\{/if\}\}")
            .unwrap_or_else(|_| panic!("Failed to create regex"));

        while let Some(captures) = if_pattern.captures(&result.clone()) {
            let full_match = captures.get(0).map(|m| m.as_str()).unwrap_or("");
            let var_name = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            let content = captures.get(2).map(|m| m.as_str()).unwrap_or("");

            if data.contains_key(var_name) && !data[var_name].is_empty() {
                // 変数が存在し、空でない場合はコンテンツを保持
                result = result.replace(full_match, content);
            } else {
                // 変数が存在しないか空の場合はブロック全体を削除
                result = result.replace(full_match, "");
            }
        }

        // 余分な空行を削除
        result
            .lines()
            .filter(|line| !line.trim().is_empty() || line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for DefaultTemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TemplateEngine for DefaultTemplateEngine {
    async fn create_template(&self, template_type: TemplateType) -> Result<ContentTemplate> {
        self.templates
            .get(&template_type)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("Template type {:?} not found", template_type)))
    }

    fn apply_template(
        &self,
        template: &ContentTemplate,
        data: &HashMap<String, String>,
    ) -> Result<String> {
        // フィールド検証
        template.validate_fields(data)?;

        // デフォルト値を適用
        let mut full_data = data.clone();
        template.apply_defaults(&mut full_data);

        // テンプレートを適用
        let result = self.simple_replace(&template.template, &full_data);

        Ok(result)
    }

    async fn customize_template(
        &self,
        template: &ContentTemplate,
        customizations: &HashMap<String, String>,
    ) -> Result<ContentTemplate> {
        let mut customized = template.clone();

        if let Some(name) = customizations.get("name") {
            customized.name = name.clone();
        }

        if let Some(description) = customizations.get("description") {
            customized.description = description.clone();
        }

        if let Some(template_content) = customizations.get("template") {
            customized.template = template_content.clone();
        }

        Ok(customized)
    }

    fn list_templates(&self) -> Vec<TemplateType> {
        self.templates.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_type_name() {
        assert_eq!(TemplateType::BlogPost.name(), "blog_post");
        assert_eq!(
            TemplateType::ProductDescription.name(),
            "product_description"
        );
        assert_eq!(
            TemplateType::Custom("my_template".to_string()).name(),
            "my_template"
        );
    }

    #[test]
    fn test_content_template_validation() {
        let template = ContentTemplate {
            template_type: TemplateType::BlogPost,
            name: "Test".to_string(),
            description: "Test template".to_string(),
            template: "{{title}}".to_string(),
            required_fields: vec!["title".to_string()],
            optional_fields: vec![],
            defaults: HashMap::new(),
        };

        let mut data = HashMap::new();
        assert!(template.validate_fields(&data).is_err());

        data.insert("title".to_string(), "Test Title".to_string());
        assert!(template.validate_fields(&data).is_ok());
    }

    #[tokio::test]
    async fn test_template_engine() {
        let engine = DefaultTemplateEngine::new();
        let template = engine
            .create_template(TemplateType::BlogPost)
            .await
            .unwrap();

        let mut data = HashMap::new();
        data.insert("title".to_string(), "Test Post".to_string());
        data.insert("content".to_string(), "This is a test.".to_string());

        let result = engine.apply_template(&template, &data).unwrap();
        assert!(result.contains("Test Post"));
        assert!(result.contains("This is a test."));
    }
}
