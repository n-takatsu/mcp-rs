use mcp_rs::config::WordPressConfig;
use mcp_rs::handlers::wordpress::{PostCreateParams, WordPressHandler};
use tracing::{error, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("🎬 WordPress Embed Content Test\n");

    // Direct WordPress configuration for testing
    let wp_config = WordPressConfig {
        url: "https://demo.wp-api.org/wp-json".to_string(),
        username: "demo".to_string(),
        password: "demo".to_string(),
        enabled: Some(true),
        timeout_seconds: Some(30),
        rate_limit: None,
    };

    // Create WordPress handler
    let handler = WordPressHandler::new(wp_config);

    println!("🔗 Testing URL validation and processing...\n");

    // Test YouTube URL validation
    let youtube_urls = vec![
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://youtu.be/dQw4w9WgXcQ",
        "https://www.youtube.com/embed/dQw4w9WgXcQ",
    ];

    for url in &youtube_urls {
        println!("📺 Testing YouTube URL: {}", url);
        println!("   Valid: {}", WordPressHandler::validate_youtube_url(url));
        if let Some(video_id) = WordPressHandler::extract_youtube_id(url) {
            println!("   Video ID: {}", video_id);
            let embed = WordPressHandler::generate_youtube_embed(&video_id, Some(480), Some(360));
            println!("   Embed HTML: {}", embed);
        }
        println!();
    }

    // Test social media URL validation
    let social_urls = vec![
        "https://twitter.com/user/status/123456789",
        "https://x.com/user/status/123456789",
        "https://instagram.com/p/ABC123xyz/",
        "https://facebook.com/user/posts/123456789",
        "https://tiktok.com/@user/video/123456789",
    ];

    for url in &social_urls {
        println!("📱 Testing Social URL: {}", url);
        if let Some(platform) = WordPressHandler::validate_social_url(url) {
            println!("   Platform: {}", platform);
        } else {
            println!("   Platform: unknown");
        }
        println!();
    }

    // Test creating post with embeds
    println!("🎬 Creating post with embedded content...\n");

    let youtube_test_urls = vec!["https://www.youtube.com/watch?v=dQw4w9WgXcQ"];
    let social_test_urls = vec!["https://twitter.com/user/status/123456789"];

    let params = PostCreateParams {
        title: "Test Post with Embedded Content".to_string(),
        content: "<p>This post contains embedded YouTube videos and social media content!</p>"
            .to_string(),
        post_type: "post".to_string(),
        status: "draft".to_string(), // Use draft for testing
        ..Default::default()
    };

    match handler
        .create_post_with_embeds(
            "Test Post with Embeds",
            "<p>Check out this amazing content:</p>",
            youtube_test_urls,
            social_test_urls,
            Some(params),
        )
        .await
    {
        Ok(post) => {
            println!("✅ Post with embeds created successfully:");
            println!("   - ID: {}", post.id.unwrap_or(0));
            println!("   - Title: {}", post.title.rendered);
            println!("   - Status: {}", post.status);
            if post.content.rendered.len() > 200 {
                println!("   - Content: {}...", &post.content.rendered[..200]);
            } else {
                println!("   - Content: {}", post.content.rendered);
            }
        }
        Err(e) => {
            error!("❌ Failed to create post with embeds: {}", e);
            return Err(e.into());
        }
    }

    println!("\n🎯 Embed test completed!");
    Ok(())
}
