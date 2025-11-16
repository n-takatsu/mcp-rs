//! Configuration Setup UI
//!
//! Interactive user interface for creating MCP-RS configuration

use crate::config::{HandlersConfig, McpConfig, RateLimitConfig, ServerConfig, WordPressConfig};
use crate::error::Error;
use crate::setup::validator::ConfigValidator;
use crossterm::{
    cursor, execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::io::{self, Write};
use tokio::time::{sleep, Duration};

pub struct ConfigSetupUI {
    validator: ConfigValidator,
}

impl ConfigSetupUI {
    pub fn new() -> Self {
        Self {
            validator: ConfigValidator::new(),
        }
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        self.show_welcome().await?;

        let config = self.collect_configuration().await?;

        self.test_configuration(&config).await?;

        self.save_configuration(&config).await?;

        self.show_completion().await?;

        Ok(())
    }

    async fn show_welcome(&self) -> Result<(), Error> {
        self.clear_screen()?;
        self.print_header("🔧 MCP-RS 設定セットアップ")?;

        println!("このセットアップウィザードでは、MCP-RSの設定を対話的に作成できます。");
        println!();
        println!("📝 WordPress サイトの情報を入力してください:");
        println!("   - サイトのURL");
        println!("   - ユーザー名");
        println!("   - Application Password");
        println!();
        println!("🔍 各設定項目で接続テストを行い、正常性を確認します。");
        println!();

        self.wait_for_enter("続行するには Enter を押してください...")?;
        Ok(())
    }

    async fn collect_configuration(&mut self) -> Result<McpConfig, Error> {
        self.clear_screen()?;
        self.print_header("📋 基本設定")?;

        // サーバー設定
        let bind_addr = self.input_with_default("サーバーのバインドアドレス", "127.0.0.1:8080")?;

        let stdio_mode =
            self.input_yes_no("STDIO モードを使用しますか？ (MCP クライアント用)", false)?;

        let log_level = self.select_log_level()?;

        // WordPress設定
        self.clear_screen()?;
        self.print_header("🔗 WordPress 設定")?;

        let wp_url = self.input_wordpress_url().await?;
        let wp_username = self.input_required("WordPressユーザー名")?;
        let wp_password = self.input_password("Application Password")?;

        // 設定オブジェクトを構築
        let config = McpConfig {
            server: ServerConfig {
                bind_addr: Some(bind_addr),
                stdio: Some(stdio_mode),
                log_level: Some(log_level),
                log_retention: None, // UI設定時はデフォルト設定を使用
                log_module: None,    // UI設定時はデフォルト設定を使用
            },
            handlers: HandlersConfig {
                wordpress: Some(WordPressConfig {
                    url: wp_url,
                    username: wp_username,
                    password: wp_password,
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                    rate_limit: Some(RateLimitConfig::default()),
                    encrypted_credentials: None,
                }),
            },
            ..Default::default()
        };

        Ok(config)
    }

    async fn test_configuration(&self, config: &McpConfig) -> Result<(), Error> {
        self.clear_screen()?;
        self.print_header("🧪 設定テスト")?;

        if let Some(wp_config) = &config.handlers.wordpress {
            println!("WordPress接続をテスト中...");
            println!("URL: {}", wp_config.url);

            let _ = self.show_spinner("接続テスト実行中").await;

            match self.validator.test_wordpress_connection(wp_config).await {
                Ok(_) => {
                    self.print_success("✅ WordPress接続テスト成功!")?;
                }
                Err(e) => {
                    self.print_error(&format!("❌ WordPress接続テスト失敗: {}", e))?;

                    if !self.input_yes_no("設定を続行しますか？", false)? {
                        return Err(Error::Config(
                            "ユーザーによってキャンセルされました".to_string(),
                        ));
                    }
                }
            }
        }

        self.wait_for_enter("続行するには Enter を押してください...")?;
        Ok(())
    }

    async fn save_configuration(&self, config: &McpConfig) -> Result<(), Error> {
        self.clear_screen()?;
        self.print_header("💾 設定保存")?;

        let config_path = "mcp-config.toml";

        println!("設定を {} に保存中...", config_path);

        match config.save_to_file(config_path) {
            Ok(_) => {
                self.print_success("✅ 設定ファイルが正常に保存されました!")?;
            }
            Err(e) => {
                self.print_error(&format!("❌ 設定ファイル保存エラー: {}", e))?;
                return Err(Error::Config(format!("設定ファイル保存失敗: {}", e)));
            }
        }

        Ok(())
    }

    async fn show_completion(&self) -> Result<(), Error> {
        self.clear_screen()?;
        self.print_header("🎉 セットアップ完了")?;

        println!("MCP-RSの設定が完了しました！");
        println!();
        println!("📁 設定ファイル: mcp-config.toml");
        println!();
        println!("🚀 次のステップ:");
        println!("   1. MCP-RS サーバーを起動: ./mcp-rs");
        println!("   2. 設定を変更する場合: ./mcp-rs --setup-config");
        println!("   3. サンプル設定を確認: ./mcp-rs --generate-config");
        println!();

        Ok(())
    }

    // Helper methods
    fn clear_screen(&self) -> Result<(), Error> {
        execute!(io::stdout(), Clear(ClearType::All), cursor::MoveTo(0, 0)).map_err(Error::Io)?;
        Ok(())
    }

    fn print_header(&self, title: &str) -> Result<(), Error> {
        queue!(
            io::stdout(),
            SetForegroundColor(Color::Cyan),
            Print("═".repeat(60)),
            Print("\n"),
            Print(format!(" {} ", title)),
            Print("\n"),
            Print("═".repeat(60)),
            Print("\n\n"),
            ResetColor
        )
        .map_err(Error::Io)?;
        io::stdout().flush().map_err(Error::Io)?;
        Ok(())
    }

    fn print_success(&self, message: &str) -> Result<(), Error> {
        queue!(
            io::stdout(),
            SetForegroundColor(Color::Green),
            Print(message),
            Print("\n"),
            ResetColor
        )
        .map_err(Error::Io)?;
        io::stdout().flush().map_err(Error::Io)?;
        Ok(())
    }

    fn print_error(&self, message: &str) -> Result<(), Error> {
        queue!(
            io::stdout(),
            SetForegroundColor(Color::Red),
            Print(message),
            Print("\n"),
            ResetColor
        )
        .map_err(Error::Io)?;
        io::stdout().flush().map_err(Error::Io)?;
        Ok(())
    }

    fn input_required(&self, prompt: &str) -> Result<String, Error> {
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 5;

        loop {
            print!("📝 {}: ", prompt);
            io::stdout().flush().map_err(Error::Io)?;

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(0) => {
                    // EOF reached, no more input available
                    return Err(Error::Config("入力ストリームが終了しました".to_string()));
                }
                Ok(_) => {
                    let input = input.trim().to_string();
                    if !input.is_empty() {
                        return Ok(input);
                    }
                }
                Err(e) => return Err(Error::Io(e)),
            }

            retry_count += 1;
            if retry_count >= MAX_RETRIES {
                return Err(Error::Config("最大試行回数に達しました".to_string()));
            }

            println!("⚠️  この項目は必須です。値を入力してください。");
        }
    }

    fn input_with_default(&self, prompt: &str, default: &str) -> Result<String, Error> {
        print!("📝 {} [{}]: ", prompt, default);
        io::stdout().flush().map_err(Error::Io)?;

        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(Error::Io)?;
        let input = input.trim();

        if input.is_empty() {
            Ok(default.to_string())
        } else {
            Ok(input.to_string())
        }
    }

    fn input_password(&self, prompt: &str) -> Result<String, Error> {
        print!("🔐 {}: ", prompt);
        io::stdout().flush().map_err(Error::Io)?;

        // Note: For production, use a proper password input library
        // For now, use regular input (password will be visible)
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(Error::Io)?;
        Ok(input.trim().to_string())
    }

    fn input_yes_no(&self, prompt: &str, default: bool) -> Result<bool, Error> {
        let default_str = if default { "Y/n" } else { "y/N" };
        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 5;

        loop {
            print!("❓ {} [{}]: ", prompt, default_str);
            io::stdout().flush().map_err(Error::Io)?;

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(0) => {
                    // EOF reached, return default
                    return Ok(default);
                }
                Ok(_) => {
                    let input = input.trim().to_lowercase();

                    match input.as_str() {
                        "" => return Ok(default),
                        "y" | "yes" => return Ok(true),
                        "n" | "no" => return Ok(false),
                        _ => {
                            retry_count += 1;
                            if retry_count >= MAX_RETRIES {
                                println!(
                                    "⚠️  最大試行回数に達しました。デフォルト値を使用します。"
                                );
                                return Ok(default);
                            }
                            println!("⚠️  'y' または 'n' で答えてください。");
                        }
                    }
                }
                Err(e) => return Err(Error::Io(e)),
            }
        }
    }

    fn select_log_level(&self) -> Result<String, Error> {
        println!("📊 ログレベルを選択してください:");
        println!("  1. error  (エラーのみ)");
        println!("  2. warn   (警告以上)");
        println!("  3. info   (情報以上) [推奨]");
        println!("  4. debug  (デバッグ情報)");
        println!("  5. trace  (詳細トレース)");

        loop {
            print!("選択 [3]: ");
            io::stdout().flush().map_err(Error::Io)?;

            let mut input = String::new();
            io::stdin().read_line(&mut input).map_err(Error::Io)?;
            let input = input.trim();

            let level = match input {
                "" | "3" => "info",
                "1" => "error",
                "2" => "warn",
                "4" => "debug",
                "5" => "trace",
                _ => {
                    println!("⚠️  1-5の数字を入力してください。");
                    continue;
                }
            };

            return Ok(level.to_string());
        }
    }

    async fn input_wordpress_url(&self) -> Result<String, Error> {
        loop {
            let url = self.input_required("WordPress サイトのURL (例: https://example.com)")?;

            // URL形式の基本チェック
            if !url.starts_with("https://") && !url.starts_with("http://") {
                println!("⚠️  URLは http:// または https:// で始まる必要があります。");
                continue;
            }

            // HTTPS推奨の警告
            if url.starts_with("http://") {
                println!("⚠️  セキュリティのため HTTPS の使用を推奨します。");
                if !self.input_yes_no("HTTP での接続を続行しますか？", false)? {
                    continue;
                }
            }

            return Ok(url);
        }
    }

    fn wait_for_enter(&self, message: &str) -> Result<(), Error> {
        print!("{}", message);
        io::stdout().flush().map_err(Error::Io)?;

        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(Error::Io)?;
        Ok(())
    }

    async fn show_spinner(&self, message: &str) -> Result<(), Error> {
        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

        print!("{}... ", message);

        for _ in 0..20 {
            // 2秒間表示
            for &ch in &spinner_chars {
                print!("\r{} {}", ch, message);
                io::stdout().flush().map_err(Error::Io)?;
                sleep(Duration::from_millis(100)).await;
            }
        }

        print!("\r");
        io::stdout().flush().map_err(Error::Io)?;
        Ok(())
    }
}
