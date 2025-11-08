//! Dynamic Configuration Management
//!
//! Provides runtime configuration switching and reloading capabilities

use crate::config::McpConfig;
use crate::error::Error;
use config::Config;
use std::path::Path;
use std::sync::{Arc, RwLock};
use tokio::sync::watch;
use tracing::{error, info, warn};

/// Dynamic configuration manager
pub struct DynamicConfigManager {
    /// Current configuration
    current_config: Arc<RwLock<McpConfig>>,
    /// Configuration file path
    config_path: Arc<RwLock<Option<String>>>,
    /// Configuration change notifier
    change_sender: watch::Sender<u64>,
    /// Configuration change receiver
    change_receiver: watch::Receiver<u64>,
    /// Configuration version counter
    version: Arc<RwLock<u64>>,
}

impl DynamicConfigManager {
    /// Create a new dynamic configuration manager
    pub fn new(initial_config: McpConfig, config_path: Option<String>) -> Self {
        let (sender, receiver) = watch::channel(0);

        Self {
            current_config: Arc::new(RwLock::new(initial_config)),
            config_path: Arc::new(RwLock::new(config_path)),
            change_sender: sender,
            change_receiver: receiver,
            version: Arc::new(RwLock::new(0)),
        }
    }

    /// Get current configuration
    pub fn get_config(&self) -> McpConfig {
        self.current_config.read().unwrap().clone()
    }

    /// Get configuration change receiver for watching changes
    pub fn get_change_receiver(&self) -> watch::Receiver<u64> {
        self.change_receiver.clone()
    }

    /// Switch to a different configuration file
    pub async fn switch_config_file(&self, new_path: &str) -> Result<(), Error> {
        info!("Switching configuration to: {}", new_path);

        // Validate file exists
        if !Path::new(new_path).exists() {
            return Err(Error::Config(format!(
                "設定ファイルが見つかりません: {}",
                new_path
            )));
        }

        // Load new configuration
        let new_config = self.load_config_from_file(new_path).await?;

        // Update configuration
        {
            let mut config = self.current_config.write().unwrap();
            *config = new_config;
        }

        // Update config path
        {
            let mut path = self.config_path.write().unwrap();
            *path = Some(new_path.to_string());
        }

        // Increment version and notify watchers
        self.notify_config_change().await;

        info!("Configuration switched successfully to: {}", new_path);
        Ok(())
    }

    /// Reload configuration from current file
    pub async fn reload_config(&self) -> Result<(), Error> {
        let config_path = {
            let path = self.config_path.read().unwrap();
            path.clone()
        };

        match config_path {
            Some(path) => {
                info!("Reloading configuration from: {}", path);
                self.switch_config_file(&path).await
            }
            None => {
                warn!("No configuration file path set, loading default config");
                let new_config =
                    McpConfig::load().map_err(|e| crate::error::Error::Internal(e.to_string()))?;

                {
                    let mut config = self.current_config.write().unwrap();
                    *config = new_config;
                }

                self.notify_config_change().await;
                info!("Default configuration reloaded");
                Ok(())
            }
        }
    }

    /// Update configuration directly
    pub async fn update_config(&self, new_config: McpConfig) -> Result<(), Error> {
        {
            let mut config = self.current_config.write().unwrap();
            *config = new_config;
        }

        self.notify_config_change().await;
        info!("Configuration updated directly");
        Ok(())
    }

    /// Get current configuration file path
    pub fn get_config_path(&self) -> Option<String> {
        self.config_path.read().unwrap().clone()
    }

    /// Get current configuration version
    pub fn get_version(&self) -> u64 {
        *self.version.read().unwrap()
    }

    /// Load configuration from file
    async fn load_config_from_file(&self, path: &str) -> Result<McpConfig, Error> {
        // Check if file exists
        if !Path::new(path).exists() {
            return Err(Error::Config(format!(
                "設定ファイルが存在しません: {}",
                path
            )));
        }

        // Use a custom config loader that reads from specific file
        use config::Config;

        let settings = ::config::Config::builder()
            .add_source(
                ::config::Config::try_from(&McpConfig::default())
                    .map_err(|e| crate::error::Error::Internal(e.to_string()))?,
            )
            .add_source(::config::File::with_name(path))
            .build()
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        let mut loaded_config: McpConfig = settings
            .try_deserialize()
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        // Apply environment variable expansion for WordPress config
        if let Some(ref mut wp_config) = loaded_config.handlers.wordpress {
            McpConfig::expand_wordpress_config(wp_config);
        }

        Ok(loaded_config)
    }

    /// Notify watchers of configuration change
    async fn notify_config_change(&self) {
        let new_version = {
            let mut version = self.version.write().unwrap();
            *version += 1;
            *version
        };

        if let Err(e) = self.change_sender.send(new_version) {
            error!("Failed to notify configuration change: {}", e);
        }
    }
}

/// Interactive configuration switcher
pub struct ConfigSwitcher {
    manager: Arc<DynamicConfigManager>,
}

impl ConfigSwitcher {
    pub fn new(manager: Arc<DynamicConfigManager>) -> Self {
        Self { manager }
    }

    /// Run interactive configuration switching
    pub async fn run_interactive_switch(&self) -> Result<(), Error> {
        use std::io::{self, Write};

        println!("🔧 動的設定変更");
        println!("════════════════════════════════════════════════════════════");
        println!();

        // Show current configuration
        let current_path = self
            .manager
            .get_config_path()
            .unwrap_or_else(|| "デフォルト設定".to_string());
        let current_version = self.manager.get_version();

        println!("📋 現在の設定:");
        println!("   - ファイル: {}", current_path);
        println!("   - バージョン: {}", current_version);
        println!();

        // Show options
        println!("設定変更オプション:");
        println!("  1. 設定ファイルを切り替え");
        println!("  2. 現在の設定をリロード");
        println!("  3. WordPress設定のみ変更");
        println!("  4. 設定の確認");
        println!("  0. キャンセル");
        println!();

        loop {
            print!("選択してください [1-4, 0]: ");
            io::stdout().flush().map_err(Error::Io)?;

            let mut input = String::new();
            match io::stdin().read_line(&mut input) {
                Ok(0) => {
                    println!("入力が終了しました。");
                    return Ok(());
                }
                Ok(_) => match input.trim() {
                    "1" => {
                        self.switch_config_file().await?;
                        break;
                    }
                    "2" => {
                        self.reload_current_config().await?;
                        break;
                    }
                    "3" => {
                        self.change_wordpress_config().await?;
                        break;
                    }
                    "4" => {
                        self.show_current_config().await?;
                    }
                    "0" => {
                        println!("キャンセルしました。");
                        return Ok(());
                    }
                    _ => {
                        println!("⚠️  無効な選択です。1-4または0を入力してください。");
                    }
                },
                Err(e) => return Err(Error::Io(e)),
            }
        }

        Ok(())
    }

    /// Switch to different configuration file
    async fn switch_config_file(&self) -> Result<(), Error> {
        use std::io::{self, Write};

        println!("\n📁 設定ファイル切り替え");
        println!("────────────────────────────────────────────────────────────");

        // List available config files
        self.list_available_configs().await;

        print!("新しい設定ファイルのパスを入力してください: ");
        io::stdout().flush().map_err(Error::Io)?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(Error::Io)?;
        let file_path = input.trim();

        if file_path.is_empty() {
            println!("⚠️  ファイルパスが入力されませんでした。");
            return Ok(());
        }

        match self.manager.switch_config_file(file_path).await {
            Ok(()) => {
                println!("✅ 設定ファイルの切り替えが完了しました!");
                println!("   新しい設定: {}", file_path);
            }
            Err(e) => {
                println!("❌ 設定ファイルの切り替えに失敗しました: {}", e);
            }
        }

        Ok(())
    }

    /// Reload current configuration
    async fn reload_current_config(&self) -> Result<(), Error> {
        println!("\n🔄 設定リロード");
        println!("────────────────────────────────────────────────────────────");

        match self.manager.reload_config().await {
            Ok(()) => {
                println!("✅ 設定のリロードが完了しました!");
                let new_version = self.manager.get_version();
                println!("   新しいバージョン: {}", new_version);
            }
            Err(e) => {
                println!("❌ 設定のリロードに失敗しました: {}", e);
            }
        }

        Ok(())
    }

    /// Change WordPress configuration only
    async fn change_wordpress_config(&self) -> Result<(), Error> {
        use std::io::{self, Write};

        println!("\n🔗 WordPress設定変更");
        println!("────────────────────────────────────────────────────────────");

        print!("WordPress URL: ");
        io::stdout().flush().map_err(Error::Io)?;
        let mut url = String::new();
        io::stdin().read_line(&mut url).map_err(Error::Io)?;
        let url = url.trim().to_string();

        if url.is_empty() {
            println!("⚠️  URLが入力されませんでした。");
            return Ok(());
        }

        print!("WordPress ユーザー名: ");
        io::stdout().flush().map_err(Error::Io)?;
        let mut username = String::new();
        io::stdin()
            .read_line(&mut username)
            .map_err(Error::Io)?;
        let username = username.trim().to_string();

        print!("Application Password: ");
        io::stdout().flush().map_err(Error::Io)?;
        let mut password = String::new();
        io::stdin()
            .read_line(&mut password)
            .map_err(Error::Io)?;
        let password = password.trim().to_string();

        // Update current configuration
        let mut current_config = self.manager.get_config();

        if let Some(ref mut wp_config) = current_config.handlers.wordpress {
            wp_config.url = url.clone();
            wp_config.username = username.clone();
            wp_config.password = password.clone();
        } else {
            // Create new WordPress config if it doesn't exist
            current_config.handlers.wordpress = Some(crate::config::WordPressConfig {
                url: url.clone(),
                username: username.clone(),
                password: password.clone(),
                enabled: Some(true),
                timeout_seconds: Some(30),
                rate_limit: Some(crate::config::RateLimitConfig::default()),
                encrypted_credentials: None,
            });
        }

        match self.manager.update_config(current_config).await {
            Ok(()) => {
                println!("✅ WordPress設定の更新が完了しました!");
                println!("   URL: {}", url);
                println!("   ユーザー名: {}", username);
            }
            Err(e) => {
                println!("❌ WordPress設定の更新に失敗しました: {}", e);
            }
        }

        Ok(())
    }

    /// Show current configuration details
    async fn show_current_config(&self) -> Result<(), Error> {
        println!("\n📋 現在の設定詳細");
        println!("────────────────────────────────────────────────────────────");

        let config = self.manager.get_config();
        let path = self
            .manager
            .get_config_path()
            .unwrap_or_else(|| "デフォルト".to_string());
        let version = self.manager.get_version();

        println!("設定ファイル: {}", path);
        println!("バージョン: {}", version);
        println!();

        // Server configuration
        println!("🔧 サーバー設定:");
        println!("   - バインドアドレス: {:?}", config.server.bind_addr);
        println!("   - STDIOモード: {:?}", config.server.stdio);
        println!("   - ログレベル: {:?}", config.server.log_level);
        println!();

        // WordPress configuration
        if let Some(wp_config) = &config.handlers.wordpress {
            println!("🔗 WordPress設定:");
            println!("   - URL: {}", wp_config.url);
            println!("   - ユーザー名: {}", wp_config.username);
            println!("   - 有効: {:?}", wp_config.enabled);
            println!("   - タイムアウト: {:?}秒", wp_config.timeout_seconds);
        } else {
            println!("🔗 WordPress設定: 未設定");
        }

        println!();
        Ok(())
    }

    /// List available configuration files
    async fn list_available_configs(&self) {
        println!("\n📁 利用可能な設定ファイル:");

        let config_files = [
            "mcp-config.toml",
            "mcp-config-demo.toml",
            "mcp-config.toml.example",
            "config.toml",
            "config/mcp.toml",
        ];

        for file in &config_files {
            if Path::new(file).exists() {
                println!("   ✅ {}", file);
            } else {
                println!("   ❌ {} (存在しません)", file);
            }
        }

        println!();
    }
}
