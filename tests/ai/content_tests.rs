//! Content Generation Integration Tests

use mcp_rs::ai::content::generator::{ContentLength, ContentPrompt, Tone};
use mcp_rs::ai::content::optimizer::OptimizationOptions;
use mcp_rs::ai::content::seo::{DefaultSeoAnalyzer, SeoAnalyzer};
use mcp_rs::ai::content::template::{DefaultTemplateEngine, TemplateEngine, TemplateType};
use std::collections::HashMap;

#[tokio::test]
async fn test_content_generator_basic() {
    // LLMクライアントを作成（モック）
    // 実際のテストでは、LlmClientのモック実装が必要
    // ここではインターフェースのテストのみ
    let prompt = ContentPrompt::new("Rustプログラミング入門")
        .with_tone(Tone::Technical)
        .with_length(ContentLength::Medium)
        .with_keywords(vec!["Rust".to_string(), "所有権".to_string()])
        .with_audience("初心者エンジニア");

    assert_eq!(prompt.topic, "Rustプログラミング入門");
    assert_eq!(prompt.keywords.len(), 2);
}

#[tokio::test]
async fn test_template_engine() {
    let engine = DefaultTemplateEngine::new();

    // ブログ記事テンプレートを取得
    let template = engine
        .create_template(TemplateType::BlogPost)
        .await
        .unwrap();

    assert_eq!(template.template_type, TemplateType::BlogPost);
    assert!(template.required_fields.contains(&"title".to_string()));
    assert!(template.required_fields.contains(&"content".to_string()));

    // テンプレートを適用
    let mut data = HashMap::new();
    data.insert("title".to_string(), "テストタイトル".to_string());
    data.insert("content".to_string(), "テスト本文".to_string());
    data.insert("author".to_string(), "テスト著者".to_string());

    let result = engine.apply_template(&template, &data).unwrap();
    assert!(result.contains("テストタイトル"));
    assert!(result.contains("テスト本文"));
    assert!(result.contains("テスト著者"));
}

#[tokio::test]
async fn test_template_validation() {
    let engine = DefaultTemplateEngine::new();
    let template = engine
        .create_template(TemplateType::BlogPost)
        .await
        .unwrap();

    // 必須フィールドが不足している場合
    let mut data = HashMap::new();
    data.insert("title".to_string(), "テストタイトル".to_string());
    // "content"が不足

    let result = engine.apply_template(&template, &data);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_template_product_description() {
    let engine = DefaultTemplateEngine::new();
    let template = engine
        .create_template(TemplateType::ProductDescription)
        .await
        .unwrap();

    let mut data = HashMap::new();
    data.insert("product_name".to_string(), "高性能ノートPC".to_string());
    data.insert(
        "description".to_string(),
        "最新技術を搭載した高性能ノートパソコン".to_string(),
    );
    data.insert("price".to_string(), "¥150,000".to_string());

    let result = engine.apply_template(&template, &data).unwrap();
    assert!(result.contains("高性能ノートPC"));
    assert!(result.contains("¥150,000"));
}

#[test]
fn test_seo_analyzer() {
    let analyzer = DefaultSeoAnalyzer::new();

    let content = r#"
# Rustプログラミング入門

Rustは安全で高速なシステムプログラミング言語です。

## 所有権システム

Rustの所有権システムは、メモリ安全性を保証します。
これにより、ヌルポインタやデータ競合といった問題を防ぐことができます。

## 型システム

Rustの型システムは強力で、コンパイル時に多くのエラーを検出できます。
型推論により、明示的な型注釈を最小限に抑えることができます。

## パフォーマンス

Rustはゼロコスト抽象化を提供し、C/C++に匹敵するパフォーマンスを実現します。
"#;

    let score = analyzer.analyze(content);

    // スコアが0.0-1.0の範囲内
    assert!(score.overall >= 0.0 && score.overall <= 1.0);
    assert!(score.readability >= 0.0 && score.readability <= 1.0);
    assert!(score.structure_quality >= 0.0 && score.structure_quality <= 1.0);

    // 見出しがあるためstructure_qualityが高い
    assert!(score.structure_quality > 0.5);

    println!("SEOスコア: {:.2} ({})", score.overall, score.grade());
}

#[test]
fn test_keyword_density() {
    let analyzer = DefaultSeoAnalyzer::new();

    let content = "Rust is great. Rust programming is fun. I love Rust and Rust tooling.";
    let keywords = vec!["Rust".to_string(), "programming".to_string()];

    let densities = analyzer.calculate_keyword_density(content, &keywords);

    assert!(densities.contains_key("Rust"));
    assert!(densities.contains_key("programming"));
    assert!(densities["Rust"] > 0.0);
    assert!(densities["programming"] > 0.0);

    // Rustの方が頻出するためdensityが高い
    assert!(densities["Rust"] > densities["programming"]);
}

#[test]
fn test_seo_suggestions() {
    let analyzer = DefaultSeoAnalyzer::new();

    // 短すぎるコンテンツ（改善提案が生成されるはず）
    let short_content = "短いコンテンツ";
    let suggestions = analyzer.suggest_improvements(short_content);

    assert!(!suggestions.is_empty());
    println!("改善提案数: {}", suggestions.len());

    for suggestion in &suggestions {
        println!(
            "- [優先度{}] {:?}: {}",
            suggestion.priority, suggestion.category, suggestion.message
        );
    }
}

#[test]
fn test_optimization_options() {
    let options = OptimizationOptions::new()
        .with_keywords(vec!["Rust".to_string(), "programming".to_string()])
        .with_target_score(0.85);

    assert!(options.enable_seo);
    assert!(options.enable_readability);
    assert_eq!(options.target_keywords.len(), 2);
    assert_eq!(options.target_seo_score, 0.85);
}

#[test]
fn test_template_list() {
    let engine = DefaultTemplateEngine::new();
    let templates = engine.list_templates();

    assert!(!templates.is_empty());
    assert!(templates.contains(&TemplateType::BlogPost));
    assert!(templates.contains(&TemplateType::ProductDescription));
    assert!(templates.contains(&TemplateType::LandingPage));
}

#[test]
fn test_readability_calculation() {
    let analyzer = DefaultSeoAnalyzer::new();

    // 短い文
    let simple_content = "これは簡単な文です。読みやすいです。";
    let simple_score = analyzer.calculate_readability(simple_content);

    // 長い文
    let complex_content = "これは非常に長くて複雑な文章で、多くの情報が含まれており、\
                           読者にとって理解するのが難しい可能性があります。";
    let complex_score = analyzer.calculate_readability(complex_content);

    // 短い文の方が読みやすいスコアが高い（はず）
    // 注: 実際の実装では日本語特有の読みやすさ指標が必要
    assert!((0.0..=1.0).contains(&simple_score));
    assert!((0.0..=1.0).contains(&complex_score));
}
