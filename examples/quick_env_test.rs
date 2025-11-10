use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;
use std::env;
use tracing::{error, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("🔒 WordPress環境変数展開テスト");

    // 現在の設定を直接使用して接続テスト
    println!("\n🧪 Test 1: 既存設定での接続確認");
    let config = McpConfig::load()?;

    if let Some(wp_config) = &config.handlers.wordpress {
        println!("✅ WordPress設定読み込み成功:");
        println!("   - URL: {}", wp_config.url);
        println!("   - Username: {}", wp_config.username);
        println!("   - Password: {}***", &wp_config.password[..4]);

        // WordPress接続テスト
        let handler = WordPressHandler::new(wp_config.clone());

        println!("\n🔗 WordPress設定取得テスト中...");
        match handler.get_settings().await {
            Ok(settings) => {
                println!("✅ WordPress接続成功！");
                if let Some(title) = settings.title {
                    println!("   サイトタイトル: {}", title);
                }
                if let Some(description) = settings.description {
                    println!("   サイト説明: {}", description);
                }
                if let Some(language) = settings.language {
                    println!("   言語: {}", language);
                }
            }
            Err(e) => {
                error!("❌ WordPress接続エラー: {}", e);
            }
        }
    } else {
        println!("❌ WordPress設定が見つかりません");
    }

    // 環境変数設定のデモ
    println!("\n🧪 Test 2: 環境変数展開のデモ");

    // テスト用環境変数を設定
    env::set_var("TEST_WP_URL", "https://demo.wordpress.com");
    env::set_var("TEST_WP_USER", "demo_user");
    env::set_var("TEST_WP_PASS", "demo_password");

    // 環境変数参照の文字列をテスト
    let test_strings = vec![
        "${TEST_WP_URL}",
        "${TEST_WP_USER}",
        "${TEST_WP_PASS}",
        "URL: ${TEST_WP_URL}, User: ${TEST_WP_USER}",
        "${NONEXISTENT_VAR}",
    ];

    for test_str in test_strings {
        let expanded = McpConfig::expand_env_vars(test_str);
        println!("   '{}' → '{}'", test_str, expanded);
    }

    println!("\n🎯 環境変数を使った安全な設定例:");
    println!("```toml");
    println!("[handlers.wordpress]");
    println!("url = \"${{WORDPRESS_URL}}\"");
    println!("username = \"${{WORDPRESS_USERNAME}}\"");
    println!("password = \"${{WORDPRESS_PASSWORD}}\"");
    println!("```");

    println!("\n📝 環境変数設定コマンド例:");
    println!("set WORDPRESS_URL=https://your-wordpress-site.com");
    println!("set WORDPRESS_USERNAME=your_username");
    println!("set WORDPRESS_PASSWORD=your_password");

    Ok(())
}
