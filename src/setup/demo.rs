//! Demo Mode for Interactive Setup
//!
//! Safe demonstration of interactive setup without requiring real user input

use crate::config::{HandlersConfig, McpConfig, RateLimitConfig, ServerConfig, WordPressConfig};
use crate::error::Error;
use crate::setup::validator::ConfigValidator;

pub struct DemoSetup {
    validator: ConfigValidator,
}

impl DemoSetup {
    pub fn new() -> Self {
        Self {
            validator: ConfigValidator::new(),
        }
    }

    /// Run a demonstration of the setup process
    pub async fn run_demo() -> Result<(), Error> {
        println!("🎭 MCP-RS セットアップ デモモード");
        println!("=====================================");
        println!();

        // Demo configuration values
        let demo_config = McpConfig {
            server: ServerConfig {
                bind_addr: Some("127.0.0.1:8080".to_string()),
                stdio: Some(false),
                log_level: Some("info".to_string()),
                log_retention: None, // デモ用はデフォルト設定を使用
                log_module: None,    // デモ用はデフォルト設定を使用
            },
            handlers: HandlersConfig {
                wordpress: Some(WordPressConfig {
                    url: "https://demo.wordpress.com".to_string(),
                    username: "demo_user".to_string(),
                    password: "demo_password_123".to_string(),
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                    rate_limit: Some(RateLimitConfig::default()),
                    encrypted_credentials: None,
                }),
            },
            ..Default::default()
        };

        // Demonstrate each step
        Self::demo_welcome().await?;
        Self::demo_basic_config().await?;
        Self::demo_wordpress_config().await?;
        Self::demo_connection_test().await?;
        Self::demo_save_config(&demo_config).await?;
        Self::demo_completion().await?;

        Ok(())
    }

    async fn demo_welcome() -> Result<(), Error> {
        println!("🔧 MCP-RS 設定セットアップ");
        println!("{}", "═".repeat(60));
        println!();
        println!("📝 基本設定の収集をシミュレート...");
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        println!("✅ サーバー設定: 127.0.0.1:8080");
        println!("✅ STDIO モード: いいえ");
        println!("✅ ログレベル: info");
        println!();
        Ok(())
    }

    async fn demo_basic_config() -> Result<(), Error> {
        println!("📋 基本設定");
        println!("{}", "═".repeat(60));
        println!("📝 サーバーのバインドアドレス [127.0.0.1:8080]: (デモ値使用)");
        println!("❓ STDIO モードを使用しますか？ [y/N]: n (デモ値)");
        println!("📊 ログレベル: 3. info (デモ値)");
        println!();
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        Ok(())
    }

    async fn demo_wordpress_config() -> Result<(), Error> {
        println!("🔗 WordPress 設定");
        println!("{}", "═".repeat(60));
        println!("📝 WordPress サイトのURL: https://demo.wordpress.com (デモ値)");
        println!("📝 WordPressユーザー名: demo_user (デモ値)");
        println!("🔐 Application Password: *** (デモ値)");
        println!();
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        Ok(())
    }

    async fn demo_connection_test() -> Result<(), Error> {
        println!("🧪 設定テスト");
        println!("{}", "═".repeat(60));
        println!("WordPress接続をテスト中...");
        println!("URL: https://demo.wordpress.com");

        // Simulate spinner
        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        print!("接続テスト実行中");

        for _ in 0..10 {
            for &ch in &spinner_chars {
                print!("\r{} 接続テスト実行中", ch);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }

        println!("\r✅ WordPress接続テスト成功! (デモ)");
        println!();
        Ok(())
    }

    async fn demo_save_config(config: &McpConfig) -> Result<(), Error> {
        println!("💾 設定保存");
        println!("{}", "═".repeat(60));
        println!("設定を mcp-config-demo.toml に保存中...");
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Actually save a demo config file
        match config.save_to_file("mcp-config-demo.toml") {
            Ok(_) => println!("✅ デモ設定ファイルが正常に保存されました!"),
            Err(e) => println!("⚠️  デモファイル保存エラー: {} (実害なし)", e),
        }
        println!();
        Ok(())
    }

    async fn demo_completion() -> Result<(), Error> {
        println!("🎉 セットアップ完了 (デモ)");
        println!("{}", "═".repeat(60));
        println!("MCP-RSの設定が完了しました！(デモモード)");
        println!();
        println!("📁 設定ファイル: mcp-config-demo.toml");
        println!();
        println!("🚀 次のステップ:");
        println!("   1. 実際のセットアップ: ./mcp-rs --setup-config");
        println!("   2. サンプル設定確認: ./mcp-rs --generate-config");
        println!("   3. このデモを再実行: ./mcp-rs --demo-setup");
        println!();
        Ok(())
    }
}
