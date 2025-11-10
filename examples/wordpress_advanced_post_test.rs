use mcp_rs::config::WordPressConfig;
use mcp_rs::handlers::wordpress::{PostCreateParams, WordPressHandler};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // 直接WordPress設定を作成（テスト用）
    let wp_config = WordPressConfig {
        url: "https://demo.wp-api.org/wp-json".to_string(),
        username: "demo".to_string(),
        password: "demo".to_string(),
        enabled: Some(true),
        timeout_seconds: Some(30),
        rate_limit: None,            // テスト用なのでレート制限無し
        encrypted_credentials: None, // 平文認証情報を使用
    };

    let handler = WordPressHandler::new(wp_config);

    println!("🚀 WordPress Advanced Post Creation Test\n");

    // 1. 下書き投稿の作成
    println!("📝 Creating a draft post...");
    let draft_post = handler
        .create_advanced_post(PostCreateParams {
            title: "My Draft Post".to_string(),
            content: "This is a draft post content.".to_string(),
            post_type: "post".to_string(),
            status: "draft".to_string(),
            date: None,
            categories: None,
            tags: None,
            featured_media_id: None,
            meta: None,
        })
        .await?;
    println!("✅ Draft post created with ID: {:?}", draft_post.id);

    // 2. SEOメタデータ付きの投稿作成（Yoast SEO用）
    println!("\n🎯 Creating a post with SEO metadata...");
    let mut seo_meta = HashMap::new();
    seo_meta.insert(
        "_yoast_wpseo_title".to_string(),
        "Custom SEO Title".to_string(),
    );
    seo_meta.insert(
        "_yoast_wpseo_metadesc".to_string(),
        "This is a custom meta description for SEO.".to_string(),
    );
    seo_meta.insert(
        "_yoast_wpseo_meta-robots-noindex".to_string(),
        "1".to_string(),
    ); // noindex
    seo_meta.insert(
        "_yoast_wpseo_meta-robots-nofollow".to_string(),
        "1".to_string(),
    ); // nofollow
    seo_meta.insert(
        "_yoast_wpseo_canonical".to_string(),
        "https://example.com/custom-canonical".to_string(),
    );
    seo_meta.insert(
        "_yoast_wpseo_focuskw".to_string(),
        "SEO keyword".to_string(),
    );

    let seo_post = handler
        .create_advanced_post(PostCreateParams {
            title: "SEO Optimized Post".to_string(),
            content: "This post has custom SEO settings applied.".to_string(),
            post_type: "post".to_string(),
            status: "publish".to_string(),
            date: None,
            categories: None,
            tags: None,
            featured_media_id: None,
            meta: Some(seo_meta),
        })
        .await?;
    println!("✅ SEO post created with ID: {:?}", seo_post.id);

    // 3. 非公開の固定ページ作成
    println!("\n📄 Creating a private page...");
    let private_page = handler
        .create_advanced_post(PostCreateParams {
            title: "Private Company Info".to_string(),
            content: "This is private company information.".to_string(),
            post_type: "page".to_string(),
            status: "private".to_string(),
            date: None,
            categories: None, // ページにはカテゴリーなし
            tags: None,       // ページにはタグなし
            featured_media_id: None,
            meta: None,
        })
        .await?;
    println!("✅ Private page created with ID: {:?}", private_page.id);

    // 4. 予約投稿の作成
    println!("\n⏰ Creating a scheduled post...");
    let future_date = "2025-12-25T10:00:00"; // ISO8601形式
    let scheduled_post = handler
        .create_advanced_post(PostCreateParams {
            title: "Christmas Special Post".to_string(),
            content: "This post will be published on Christmas!".to_string(),
            post_type: "post".to_string(),
            status: "future".to_string(),
            date: Some(future_date.to_string()),
            categories: None,
            tags: None,
            featured_media_id: None,
            meta: None,
        })
        .await?;
    println!("✅ Scheduled post created with ID: {:?}", scheduled_post.id);
    println!("   Scheduled for: {}", future_date);

    // 5. 投稿と固定ページの一覧取得
    println!("\n📋 Getting all content...");
    let (posts, pages) = handler.get_all_content().await?;
    println!("✅ Found {} posts and {} pages", posts.len(), pages.len());

    // 投稿ステータスの内訳表示
    let mut status_counts = HashMap::new();
    for post in &posts {
        *status_counts.entry(post.status.clone()).or_insert(0) += 1;
    }
    println!("\n📊 Post status breakdown:");
    for (status, count) in status_counts {
        let status_emoji = match status.as_str() {
            "publish" => "🟢",
            "draft" => "🟡",
            "private" => "🔒",
            "future" => "⏰",
            _ => "❓",
        };
        println!("   {} {}: {} posts", status_emoji, status, count);
    }

    println!("\n🎉 Advanced post creation test completed!");

    Ok(())
}
