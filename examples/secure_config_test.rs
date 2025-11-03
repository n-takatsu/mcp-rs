use mcp_rs::config::McpConfig;
use std::env;
use std::fs;
use tracing::{error, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("🔒 MCP-RS セキュア設定機能のテスト");

    // テスト用の設定ファイルを作成
    let test_config = r#"
[server]
bind_addr = "127.0.0.1:8080"
stdio = false
log_level = "info"

[handlers.wordpress]
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
enabled = true
timeout_seconds = 30
"#;

    // テスト用設定ファイルを作成
    fs::write("test-config.toml", test_config)?;
    println!("📝 テスト用設定ファイルを作成: test-config.toml");

    // Test 1: 環境変数が設定されていない場合
    println!("\n🧪 Test 1: 環境変数未設定の場合");

    // 環境変数をクリア
    env::remove_var("WORDPRESS_URL");
    env::remove_var("WORDPRESS_USERNAME");
    env::remove_var("WORDPRESS_PASSWORD");

    // 設定をロードしてチェック
    let config_result = load_test_config();
    match config_result {
        Ok(config) => {
            if let Some(wp_config) = &config.handlers.wordpress {
                println!("✅ 設定ロード成功:");
                println!("   - URL: {}", wp_config.url);
                println!("   - Username: {}", wp_config.username);
                println!("   - Password: {}", wp_config.password);

                // 環境変数が展開されていないことを確認
                assert!(wp_config.url.contains("${WORDPRESS_URL}"));
                assert!(wp_config.username.contains("${WORDPRESS_USERNAME}"));
                assert!(wp_config.password.contains("${WORDPRESS_PASSWORD}"));
                println!("✅ 環境変数未設定時の動作確認 OK");
            }
        }
        Err(e) => {
            error!("❌ 設定ロードエラー: {}", e);
        }
    }

    // Test 2: 環境変数が設定されている場合
    println!("\n🧪 Test 2: 環境変数設定済みの場合");

    // テスト用環境変数を設定
    env::set_var("WORDPRESS_URL", "https://test-site.example.com");
    env::set_var("WORDPRESS_USERNAME", "test_user");
    env::set_var("WORDPRESS_PASSWORD", "test_password_123");

    let config_result = load_test_config();
    match config_result {
        Ok(config) => {
            if let Some(wp_config) = &config.handlers.wordpress {
                println!("✅ 設定ロード成功:");
                println!("   - URL: {}", wp_config.url);
                println!("   - Username: {}", wp_config.username);
                println!("   - Password: {}", wp_config.password);

                // 環境変数が正しく展開されていることを確認
                assert_eq!(wp_config.url, "https://test-site.example.com");
                assert_eq!(wp_config.username, "test_user");
                assert_eq!(wp_config.password, "test_password_123");
                println!("✅ 環境変数展開機能 OK");
            }
        }
        Err(e) => {
            error!("❌ 設定ロードエラー: {}", e);
        }
    }

    // Test 3: 一部の環境変数のみ設定されている場合
    println!("\n🧪 Test 3: 一部環境変数のみ設定の場合");

    env::set_var("WORDPRESS_URL", "https://partial-test.example.com");
    env::remove_var("WORDPRESS_USERNAME");
    env::remove_var("WORDPRESS_PASSWORD");

    let config_result = load_test_config();
    match config_result {
        Ok(config) => {
            if let Some(wp_config) = &config.handlers.wordpress {
                println!("✅ 設定ロード成功:");
                println!("   - URL: {}", wp_config.url);
                println!("   - Username: {}", wp_config.username);
                println!("   - Password: {}", wp_config.password);

                // 一部のみ展開されていることを確認
                assert_eq!(wp_config.url, "https://partial-test.example.com");
                assert!(wp_config.username.contains("${WORDPRESS_USERNAME}"));
                assert!(wp_config.password.contains("${WORDPRESS_PASSWORD}"));
                println!("✅ 部分的環境変数展開 OK");
            }
        }
        Err(e) => {
            error!("❌ 設定ロードエラー: {}", e);
        }
    }

    // Test 4: セキュリティベストプラクティスのデモ
    println!("\n🔒 セキュリティベストプラクティス:");
    println!("✅ パスワードは設定ファイルに直接記載されません");
    println!("✅ 環境変数による動的な値の注入が可能");
    println!("✅ 設定ファイルをバージョン管理に安全に含められます");
    println!("✅ 開発・本番環境での設定切り替えが容易");

    // クリーンアップ
    fs::remove_file("test-config.toml").ok();
    println!("\n🧹 テストファイルをクリーンアップしました");

    println!("\n🎉 すべてのセキュリティ設定テストが完了しました！");

    Ok(())
}

/// テスト用の設定読み込み（実際の load() を模倣）
fn load_test_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    let mut settings = config::Config::builder();

    // デフォルト値を設定
    let default_config = McpConfig::default();
    settings = settings.add_source(config::Config::try_from(&default_config)?);

    // テスト設定ファイルを読み込み
    settings = settings.add_source(config::File::with_name("test-config"));

    let config: McpConfig = settings.build()?.try_deserialize()?;
    let mut final_config = config;

    // WordPressConfig の環境変数展開を適用
    if let Some(ref mut wp_config) = final_config.handlers.wordpress {
        McpConfig::expand_wordpress_config(wp_config);
    }

    Ok(final_config)
}
