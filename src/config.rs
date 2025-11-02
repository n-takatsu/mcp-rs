use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct McpConfig {
    pub server: ServerConfig,
    pub handlers: HandlersConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    pub bind_addr: Option<String>,
    pub stdio: Option<bool>,
    pub log_level: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HandlersConfig {
    pub wordpress: Option<WordPressConfig>,
    // 将来の拡張用
    // pub github: Option<GitHubConfig>,
    // pub custom: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WordPressConfig {
    pub url: String,
    pub username: String,
    pub password: String, // Application Password
    pub enabled: Option<bool>,
    pub timeout_seconds: Option<u64>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_addr: Some("127.0.0.1:8080".to_string()),
                stdio: Some(false),
                log_level: Some("info".to_string()),
            },
            handlers: HandlersConfig { wordpress: None },
        }
    }
}

impl McpConfig {
    /// 設定ファイルから読み込み、環境変数で上書き
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let mut settings = config::Config::builder();

        // デフォルト値を設定
        let default_config = McpConfig::default();
        settings = settings.add_source(config::Config::try_from(&default_config)?);

        // 設定ファイルを読み込み（複数の場所を試行）
        let config_paths = [
            "mcp-config.toml",
            "config.toml",
            "config/mcp.toml",
            "~/.config/mcp-rs/config.toml",
        ];

        for path in &config_paths {
            if std::path::Path::new(path).exists() {
                println!("📁 設定ファイルを読み込み: {}", path);
                settings = settings.add_source(config::File::with_name(path));
                break;
            }
        }

        // 環境変数で上書き (MCP_で始まる変数)
        settings = settings.add_source(
            config::Environment::with_prefix("MCP")
                .separator("_")
                .try_parsing(true),
        );

        // 特定の環境変数も直接対応（後方互換性）
        settings = settings.add_source(config::Environment::default().try_parsing(true));

        let config: McpConfig = settings.build()?.try_deserialize()?;

        // 環境変数による個別上書き
        let mut final_config = config;

        // WordPress設定の環境変数上書き
        if let Ok(wp_url) = std::env::var("WORDPRESS_URL") {
            if final_config.handlers.wordpress.is_none() {
                final_config.handlers.wordpress = Some(WordPressConfig {
                    url: wp_url,
                    username: std::env::var("WORDPRESS_USERNAME").unwrap_or_default(),
                    password: std::env::var("WORDPRESS_PASSWORD").unwrap_or_default(),
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                });
            } else if let Some(ref mut wp_config) = final_config.handlers.wordpress {
                wp_config.url = wp_url;
                if let Ok(username) = std::env::var("WORDPRESS_USERNAME") {
                    wp_config.username = username;
                }
                if let Ok(password) = std::env::var("WORDPRESS_PASSWORD") {
                    wp_config.password = password;
                }
            }
        }

        // サーバー設定の環境変数上書き
        if let Ok(bind_addr) = std::env::var("BIND_ADDR") {
            final_config.server.bind_addr = Some(bind_addr);
        }

        if std::env::var("MCP_STDIO").is_ok() {
            final_config.server.stdio = Some(true);
        }

        Ok(final_config)
    }

    /// サンプル設定ファイルを生成
    pub fn generate_sample_config() -> Result<(), Box<dyn std::error::Error>> {
        let sample_config = McpConfig {
            server: ServerConfig {
                bind_addr: Some("127.0.0.1:8080".to_string()),
                stdio: Some(false),
                log_level: Some("info".to_string()),
            },
            handlers: HandlersConfig {
                wordpress: Some(WordPressConfig {
                    url: "https://your-wordpress-site.com".to_string(),
                    username: "your_username".to_string(),
                    password: "your_application_password".to_string(),
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                }),
            },
        };

        let toml_content = toml::to_string_pretty(&sample_config)?;

        let sample_content = format!(
            r#"# MCP-RS Configuration File
# 
# このファイルは mcp-config.toml として保存してください
# 環境変数での上書きも可能です (例: MCP_SERVER_BIND_ADDR=0.0.0.0:8080)

{}

# 設定説明:
# 
# [server]
# bind_addr = TCP サーバーのバインドアドレス (stdio=false の場合)
# stdio = true にすると標準入出力モードで動作 (MCP クライアント用)
# log_level = ログレベル (trace, debug, info, warn, error)
#
# [handlers.wordpress]
# url = WordPress サイトの URL
# username = WordPress ユーザー名
# password = Application Password (WordPress管理画面で生成)
# enabled = このハンドラーを有効にするか
#
# Application Password の生成方法:
# 1. WordPress管理画面 > ユーザー > プロフィール
# 2. 'アプリケーションパスワード' セクション
# 3. 新しいアプリケーション名を入力 (例: "MCP-RS")
# 4. 'Add New Application Password' をクリック
# 5. 生成されたパスワードをコピーして上記 password に設定
"#,
            toml_content
        );

        std::fs::write("mcp-config.toml.example", sample_content)?;
        println!("📝 サンプル設定ファイルを生成しました: mcp-config.toml.example");
        println!("💡 このファイルを mcp-config.toml にコピーして編集してください");

        Ok(())
    }
}
