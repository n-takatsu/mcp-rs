//! Content Generation Demo
//!
//! インテリジェントコンテンツ生成機能のデモンストレーション

use mcp_rs::ai::content::generator::{ContentLength, ContentPrompt, Tone};
use mcp_rs::ai::content::optimizer::OptimizationOptions;
use mcp_rs::ai::content::seo::{DefaultSeoAnalyzer, SeoAnalyzer};
use mcp_rs::ai::content::template::{DefaultTemplateEngine, TemplateEngine, TemplateType};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== インテリジェントコンテンツ生成デモ ===\n");

    // 1. テンプレートエンジンのデモ
    demo_template_engine().await?;

    // 2. SEOアナライザーのデモ
    demo_seo_analyzer()?;

    // 3. コンテンツ生成プロンプトのデモ
    demo_content_prompt();

    // 4. 最適化オプションのデモ
    demo_optimization_options();

    println!("\n=== デモ完了 ===");
    Ok(())
}

/// テンプレートエンジンのデモ
async fn demo_template_engine() -> Result<(), Box<dyn std::error::Error>> {
    println!("## 1. テンプレートエンジンのデモ\n");

    let engine = DefaultTemplateEngine::new();

    // 使用可能なテンプレートを表示
    let templates = engine.list_templates();
    println!("使用可能なテンプレート: {} 種類", templates.len());
    for template_type in &templates {
        println!("  - {:?}", template_type);
    }
    println!();

    // ブログ記事テンプレートの使用例
    println!("### ブログ記事テンプレートの使用例\n");
    let blog_template = engine.create_template(TemplateType::BlogPost).await?;

    let mut blog_data = HashMap::new();
    blog_data.insert(
        "title".to_string(),
        "Rustでのエラーハンドリング".to_string(),
    );
    blog_data.insert("author".to_string(), "技術ブログ編集部".to_string());
    blog_data.insert("date".to_string(), "2025-12-15".to_string());
    blog_data.insert(
        "introduction".to_string(),
        "Rustのエラーハンドリングは、型安全性を維持しながら堅牢なコードを書くための重要な機能です。".to_string(),
    );
    blog_data.insert(
        "content".to_string(),
        "RustではResult型とOption型を使用してエラーを処理します。\n\
         Result<T, E>型は成功時にOk(T)、エラー時にErr(E)を返します。\n\
         ?演算子を使用することで、エラーハンドリングを簡潔に記述できます。"
            .to_string(),
    );
    blog_data.insert(
        "conclusion".to_string(),
        "適切なエラーハンドリングにより、予期しない状況でも安全に動作するアプリケーションを構築できます。".to_string(),
    );
    blog_data.insert(
        "tags".to_string(),
        "Rust, エラーハンドリング, プログラミング".to_string(),
    );
    blog_data.insert("categories".to_string(), "技術ブログ, Rust入門".to_string());

    let blog_content = engine.apply_template(&blog_template, &blog_data)?;
    println!("{}\n", blog_content);

    // 商品説明テンプレートの使用例
    println!("### 商品説明テンプレートの使用例\n");
    let product_template = engine
        .create_template(TemplateType::ProductDescription)
        .await?;

    let mut product_data = HashMap::new();
    product_data.insert(
        "product_name".to_string(),
        "高性能開発用ノートPC XPro 15".to_string(),
    );
    product_data.insert(
        "tagline".to_string(),
        "プロフェッショナルのための究極のモバイルワークステーション".to_string(),
    );
    product_data.insert(
        "description".to_string(),
        "最新のプロセッサと大容量メモリを搭載し、\
         複雑な開発タスクもスムーズにこなせる高性能ノートパソコンです。"
            .to_string(),
    );
    product_data.insert(
        "features".to_string(),
        "- Intel Core i9プロセッサ\n\
         - 32GB RAM\n\
         - 1TB NVMe SSD\n\
         - 15.6インチ 4K ディスプレイ\n\
         - バックライト付きキーボード"
            .to_string(),
    );
    product_data.insert("price".to_string(), "¥250,000（税込）".to_string());
    product_data.insert(
        "availability".to_string(),
        "在庫あり - 2営業日以内に発送".to_string(),
    );

    let product_content = engine.apply_template(&product_template, &product_data)?;
    println!("{}\n", product_content);

    Ok(())
}

/// SEOアナライザーのデモ
fn demo_seo_analyzer() -> Result<(), Box<dyn std::error::Error>> {
    println!("## 2. SEOアナライザーのデモ\n");

    let analyzer = DefaultSeoAnalyzer::new();

    let sample_content = r#"
# Rustプログラミング言語入門

Rustは、パフォーマンス、信頼性、生産性を重視したシステムプログラミング言語です。

## メモリ安全性

Rustの最大の特徴は、所有権システムによるメモリ安全性の保証です。
ガベージコレクタを使用せずに、メモリリークやヌルポインタ参照を防ぎます。

## ゼロコスト抽象化

Rustは高レベルの抽象化を提供しながら、実行時のオーバーヘッドを最小限に抑えます。
これにより、C/C++に匹敵するパフォーマンスを実現できます。

## 強力な型システム

Rustの型システムは、多くのバグをコンパイル時に検出します。
パターンマッチングとトレイトシステムにより、安全で表現力豊かなコードを書けます。

## まとめ

Rustは、安全性とパフォーマンスを両立させた現代的なプログラミング言語です。
学習曲線は急ですが、習得する価値のある言語と言えます。
"#;

    // コンテンツを分析
    println!("### コンテンツ分析結果\n");
    let score = analyzer.analyze(sample_content);

    println!(
        "**総合スコア**: {:.1}% ({})",
        score.overall * 100.0,
        score.grade()
    );
    println!("**キーワード密度**: {:.1}%", score.keyword_density * 100.0);
    println!("**読みやすさ**: {:.1}%", score.readability * 100.0);
    println!("**構造品質**: {:.1}%\n", score.structure_quality * 100.0);

    // 改善提案を表示
    if !score.suggestions.is_empty() {
        println!("### 改善提案\n");
        for suggestion in &score.suggestions {
            println!(
                "**[優先度 {}] {}**",
                suggestion.priority, suggestion.message
            );
            if let Some(details) = &suggestion.details {
                println!("  → {}", details);
            }
            println!();
        }
    } else {
        println!("改善提案なし - コンテンツは良好です！\n");
    }

    // キーワード密度を計算
    println!("### キーワード密度分析\n");
    let keywords = vec![
        "Rust".to_string(),
        "プログラミング".to_string(),
        "安全性".to_string(),
        "パフォーマンス".to_string(),
    ];

    let densities = analyzer.calculate_keyword_density(sample_content, &keywords);
    for (keyword, density) in &densities {
        println!("  - \"{}\": {:.2}%", keyword, density * 100.0);
    }
    println!();

    Ok(())
}

/// コンテンツ生成プロンプトのデモ
fn demo_content_prompt() {
    println!("## 3. コンテンツ生成プロンプトのデモ\n");

    let prompt = ContentPrompt::new("RustでのWebアプリケーション開発")
        .with_tone(Tone::Technical)
        .with_length(ContentLength::Long)
        .with_keywords(vec![
            "Rust".to_string(),
            "Web開発".to_string(),
            "Actix-web".to_string(),
            "非同期処理".to_string(),
        ])
        .with_audience("中級以上のRust開発者");

    println!("### プロンプト設定\n");
    println!("**トピック**: {}", prompt.topic);
    println!("**トーン**: {:?}", prompt.tone);
    println!("**長さ**: {:?}", prompt.length);
    println!("**キーワード**: {}", prompt.keywords.join(", "));
    if let Some(audience) = &prompt.audience {
        println!("**ターゲットオーディエンス**: {}", audience);
    }
    println!();

    println!("### LLMプロンプト（一部）\n");
    let llm_prompt = prompt.to_llm_prompt();
    let lines: Vec<&str> = llm_prompt.lines().take(15).collect();
    println!("{}", lines.join("\n"));
    println!("...\n");
}

/// 最適化オプションのデモ
fn demo_optimization_options() {
    println!("## 4. 最適化オプションのデモ\n");

    let options = OptimizationOptions::new()
        .with_keywords(vec![
            "Rust".to_string(),
            "パフォーマンス".to_string(),
            "メモリ安全性".to_string(),
        ])
        .with_target_score(0.85);

    println!("### 最適化設定\n");
    println!(
        "**SEO最適化**: {}",
        if options.enable_seo {
            "有効"
        } else {
            "無効"
        }
    );
    println!(
        "**読みやすさ最適化**: {}",
        if options.enable_readability {
            "有効"
        } else {
            "無効"
        }
    );
    println!(
        "**キーワード最適化**: {}",
        if options.enable_keywords {
            "有効"
        } else {
            "無効"
        }
    );
    println!(
        "**ターゲットキーワード**: {}",
        options.target_keywords.join(", ")
    );
    println!(
        "**目標SEOスコア**: {:.0}%",
        options.target_seo_score * 100.0
    );
    println!("**最大イテレーション**: {}", options.max_iterations);
    println!();

    println!("この設定で、コンテンツは以下のように最適化されます:");
    println!("1. SEOスコアが85%に達するまで改善を繰り返します");
    println!("2. 指定されたキーワードが適切な密度で含まれるよう調整します");
    println!("3. 文章の読みやすさを向上させます");
    println!("4. 最大3回のイテレーションで最適化を完了します");
    println!();
}
