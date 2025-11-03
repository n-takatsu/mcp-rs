//! Configuration Management for MCP-RS
//!
//! This module provides configuration structures and utilities for managing
//! MCP (Model Context Protocol) server settings, including plugin configuration.
//!
//! # Examples
//!
//! ## Basic Configuration Usage
//!
//! ```rust
//! use mcp_rs::config::McpConfig;
//!
//! // Use default configuration
//! let config = McpConfig::default();
//! println!("Server bind address: {:?}", config.server.bind_addr);
//! println!("Log level: {:?}", config.server.log_level);
//! ```
//!
//! ## Plugin Configuration
//!
//! ```rust
//! use mcp_rs::config::{McpConfig, PluginsConfig};
//! use std::collections::HashMap;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut config = McpConfig::default();
//!
//! // Configure plugin settings
//! if let Some(ref mut plugins) = config.plugins {
//!     plugins.auto_load = Some(true);
//!     plugins.hot_reload = Some(false);
//!     plugins.search_paths = Some(vec![
//!         "./my_plugins".to_string(),
//!         "/opt/custom_plugins".to_string(),
//!     ]);
//! }
//!
//! // Convert to plugin config for use with plugin system
//! if let Some(plugin_config) = config.to_plugin_config() {
//!     println!("Plugin search paths: {:?}", plugin_config.search_paths);
//!     println!("Auto-load enabled: {}", plugin_config.auto_load);
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// 前方宣言用の型エイリアス
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    pub server: ServerConfig,
    pub transport: TransportConfig,
    pub handlers: HandlersConfig,
    pub plugins: Option<PluginsConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub bind_addr: Option<String>,
    pub stdio: Option<bool>,
    pub log_level: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransportConfig {
    /// Transport type: "stdio", "http", "websocket"
    pub transport_type: Option<String>,
    /// Stdio transport configuration
    pub stdio: Option<StdioTransportConfig>,
    /// HTTP transport configuration
    pub http: Option<HttpTransportConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StdioTransportConfig {
    pub buffer_size: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub content_length_header: Option<bool>,
    pub framing_method: Option<String>, // "content-length" | "line-based"
    pub max_message_size: Option<usize>,
    pub pretty_print: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpTransportConfig {
    pub addr: Option<String>,
    pub port: Option<u16>,
    pub enable_cors: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HandlersConfig {
    pub wordpress: Option<WordPressConfig>,
    // 将来の拡張用
    // pub github: Option<GitHubConfig>,
    // pub custom: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginsConfig {
    /// Plugin search directories
    pub search_paths: Option<Vec<String>>,
    /// Auto-load plugins on startup
    pub auto_load: Option<bool>,
    /// Plugin-specific configurations
    pub plugins: Option<std::collections::HashMap<String, serde_json::Value>>,
    /// Enable hot reloading of plugins
    pub hot_reload: Option<bool>,
    /// Maximum number of plugins to load
    pub max_plugins: Option<usize>,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self {
            search_paths: Some(vec![
                "./plugins".to_string(),
                "/usr/local/lib/mcp-rs/plugins".to_string(),
            ]),
            auto_load: Some(true),
            plugins: Some(std::collections::HashMap::new()),
            hot_reload: Some(false),
            max_plugins: Some(50),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WordPressConfig {
    pub url: String,
    pub username: String,
    pub password: String, // Application Password
    pub enabled: Option<bool>,
    pub timeout_seconds: Option<u64>,
    /// レート制限設定
    pub rate_limit: Option<RateLimitConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    /// 最大リクエスト数/秒
    pub requests_per_second: u32,
    /// バーストリクエスト許可数
    pub burst_size: u32,
    /// レート制限有効化フラグ
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10, // 10 requests/sec
            burst_size: 20,          // 20 burst requests
            enabled: true,
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_addr: Some("127.0.0.1:8080".to_string()),
                stdio: Some(false),
                log_level: Some("info".to_string()),
            },
            transport: TransportConfig {
                transport_type: Some("stdio".to_string()),
                stdio: Some(StdioTransportConfig {
                    buffer_size: Some(8192),
                    timeout_ms: Some(30000),
                    content_length_header: Some(true),
                    framing_method: Some("content-length".to_string()),
                    max_message_size: Some(1048576),
                    pretty_print: Some(false),
                }),
                http: None,
            },
            handlers: HandlersConfig { wordpress: None },
            plugins: Some(PluginsConfig {
                search_paths: Some(vec![
                    "./plugins".to_string(),
                    "/usr/local/lib/mcp-rs/plugins".to_string(),
                ]),
                auto_load: Some(true),
                plugins: Some(std::collections::HashMap::new()),
                hot_reload: Some(false),
                max_plugins: Some(50),
            }),
        }
    }
}

impl McpConfig {
    /// 文字列内の環境変数を展開する (${VAR_NAME} 形式)
    pub fn expand_env_vars(input: &str) -> String {
        let mut result = input.to_string();
        let mut processed_vars = std::collections::HashSet::new();
        let max_iterations = 100; // 無限ループ防止
        let mut iteration_count = 0;

        // ${VAR_NAME} パターンを検索して置換
        loop {
            iteration_count += 1;
            if iteration_count > max_iterations {
                warn!(
                    "環境変数展開で最大反復回数({})に達しました。処理を停止します。",
                    max_iterations
                );
                break;
            }

            if let Some(start) = result.find("${") {
                if let Some(end_pos) = result[start..].find('}') {
                    let end = start + end_pos;
                    let var_name = &result[start + 2..end];

                    // 既に処理済みで値が見つからなかった変数は再処理しない
                    let var_pattern = format!("${{{}}}", var_name);
                    if processed_vars.contains(&var_pattern) {
                        warn!(
                            "環境変数 '{}' は既に処理済みで値が見つかりません。スキップします。",
                            var_name
                        );
                        // この変数をスキップして次を探す - より安全な方法で処理停止
                        break;
                    }

                    match std::env::var(var_name) {
                        Ok(env_value) => {
                            debug!(
                                "環境変数展開成功: {} = {}",
                                var_name,
                                &env_value[..env_value.len().min(20)]
                            );
                            result.replace_range(start..end + 1, &env_value);
                            // 成功した場合は続行
                        }
                        Err(_) => {
                            warn!("環境変数 '{}' が設定されていません。", var_name);
                            processed_vars.insert(var_pattern.clone());

                            // 環境変数が見つからない場合の処理選択肢：
                            // 1. エラーとして処理を停止
                            // 2. 空文字に置換
                            // 3. プレースホルダーに置換

                            // Option 1: エラーとして停止（推奨）
                            return result.replace(&var_pattern, &format!("[ERROR:{}]", var_name));

                            // Option 2: 空文字に置換（コメントアウト）
                            // result.replace_range(start..end + 1, "");

                            // Option 3: 分かりやすいプレースホルダー（コメントアウト）
                            // result.replace_range(start..end + 1, &format!("[MISSING:{}]", var_name));
                        }
                    }
                } else {
                    warn!("無効な環境変数形式が検出されました。開始位置: {}", start);
                    // 無効な形式の場合、その部分をエラーマーカーに置換
                    result.replace_range(start..start + 2, "[INVALID_ENV_VAR]");
                    break;
                }
            } else {
                break; // ${がない場合は正常終了
            }
        }

        debug!("環境変数展開完了。反復回数: {}", iteration_count);
        result
    }

    /// WordPressConfig の環境変数を展開
    pub fn expand_wordpress_config(config: &mut WordPressConfig) {
        config.url = Self::expand_env_vars(&config.url);
        config.username = Self::expand_env_vars(&config.username);
        config.password = Self::expand_env_vars(&config.password);
    }

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

        // WordPressConfig の環境変数展開を適用
        if let Some(ref mut wp_config) = final_config.handlers.wordpress {
            info!("WordPress設定で環境変数展開を適用中...");
            Self::expand_wordpress_config(wp_config);
        }

        // WordPress設定の環境変数上書き
        if let Ok(wp_url) = std::env::var("WORDPRESS_URL") {
            if final_config.handlers.wordpress.is_none() {
                final_config.handlers.wordpress = Some(WordPressConfig {
                    url: wp_url,
                    username: std::env::var("WORDPRESS_USERNAME").unwrap_or_default(),
                    password: std::env::var("WORDPRESS_PASSWORD").unwrap_or_default(),
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                    rate_limit: Some(RateLimitConfig::default()),
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
            transport: TransportConfig {
                transport_type: Some("stdio".to_string()),
                stdio: Some(StdioTransportConfig {
                    buffer_size: Some(8192),
                    timeout_ms: Some(30000),
                    content_length_header: Some(true),
                    framing_method: Some("content-length".to_string()),
                    max_message_size: Some(1048576),
                    pretty_print: Some(false),
                }),
                http: Some(HttpTransportConfig {
                    addr: Some("127.0.0.1".to_string()),
                    port: Some(8080),
                    enable_cors: Some(true),
                }),
            },
            handlers: HandlersConfig {
                wordpress: Some(WordPressConfig {
                    url: "${WORDPRESS_URL}".to_string(),
                    username: "${WORDPRESS_USERNAME}".to_string(),
                    password: "${WORDPRESS_PASSWORD}".to_string(),
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                    rate_limit: Some(RateLimitConfig::default()),
                }),
            },
            plugins: Some(PluginsConfig {
                search_paths: Some(vec![
                    "./plugins".to_string(),
                    "/usr/local/lib/mcp-rs/plugins".to_string(),
                ]),
                auto_load: Some(true),
                plugins: Some(std::collections::HashMap::new()),
                hot_reload: Some(false),
                max_plugins: Some(50),
            }),
        };

        let toml_content = toml::to_string_pretty(&sample_config)?;

        let sample_content = format!(
            r#"# MCP-RS Configuration File
# 
# このファイルは mcp-config.toml として保存してください
# セキュリティのため、認証情報は環境変数を使用することを推奨します

{}

# 🔒 セキュリティ設定説明:
# 
# 環境変数を使用した安全な設定方法:
# 
# 1. 環境変数を設定:
#    export WORDPRESS_URL="https://your-wordpress-site.com"
#    export WORDPRESS_USERNAME="your_username"  
#    export WORDPRESS_PASSWORD="your_app_password"
#
# 2. または .env ファイルを使用:
#    WORDPRESS_URL=https://your-wordpress-site.com
#    WORDPRESS_USERNAME=your_username
#    WORDPRESS_PASSWORD=your_app_password
#
# 3. 設定ファイルでは ${{VAR_NAME}} 形式で参照:
#    url = "${{WORDPRESS_URL}}"
#    username = "${{WORDPRESS_USERNAME}}"
#    password = "${{WORDPRESS_PASSWORD}}"
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
# 🔑 Application Password の生成方法:
# 1. WordPress管理画面 > ユーザー > プロフィール
# 2. 'アプリケーションパスワード' セクション
# 3. 新しいアプリケーション名を入力 (例: "MCP-RS")
# 4. 'Add New Application Password' をクリック
# 5. 生成されたパスワードを環境変数に設定
#
# ⚠️  注意: パスワードを設定ファイルに直接記載しないでください！
"#,
            toml_content
        );

        std::fs::write("mcp-config.toml.example", sample_content)?;
        println!("📝 サンプル設定ファイルを生成しました: mcp-config.toml.example");
        println!("💡 このファイルを mcp-config.toml にコピーして編集してください");

        Ok(())
    }

    /// Convert to transport configuration
    pub fn to_transport_config(&self) -> crate::transport::TransportConfig {
        use crate::transport::{stdio::StdioConfig, FramingMethod, TransportType};

        let transport_type = match self.transport.transport_type.as_deref() {
            Some("stdio") => TransportType::Stdio,
            Some("http") => {
                let addr = self
                    .transport
                    .http
                    .as_ref()
                    .and_then(|h| h.addr.as_deref())
                    .unwrap_or("127.0.0.1");
                let port = self
                    .transport
                    .http
                    .as_ref()
                    .and_then(|h| h.port)
                    .unwrap_or(8080);
                TransportType::Http {
                    addr: format!("{}:{}", addr, port)
                        .parse()
                        .unwrap_or_else(|_| "127.0.0.1:8080".parse().unwrap()),
                }
            }
            _ => TransportType::Stdio, // Default fallback
        };

        let stdio_config = if let Some(stdio) = &self.transport.stdio {
            StdioConfig {
                buffer_size: stdio.buffer_size.unwrap_or(8192),
                timeout_ms: stdio.timeout_ms.unwrap_or(30000),
                content_length_header: stdio.content_length_header.unwrap_or(true),
                framing_method: match stdio.framing_method.as_deref() {
                    Some("line-based") => FramingMethod::LineBased,
                    _ => FramingMethod::ContentLength,
                },
                max_message_size: stdio.max_message_size.unwrap_or(1048576),
                pretty_print: stdio.pretty_print.unwrap_or(false),
            }
        } else {
            StdioConfig::default()
        };

        crate::transport::TransportConfig {
            transport_type,
            stdio: stdio_config,
        }
    }

    /// Convert to plugin configuration
    pub fn to_plugin_config(&self) -> Option<PluginConfig> {
        self.plugins.as_ref().map(|plugins| PluginConfig {
            search_paths: plugins
                .search_paths
                .as_ref()
                .map(|paths| paths.iter().map(PathBuf::from).collect())
                .unwrap_or_default(),
            auto_load: plugins.auto_load.unwrap_or(true),
            plugins: plugins.plugins.clone().unwrap_or_default(),
            hot_reload: plugins.hot_reload.unwrap_or(false),
            max_plugins: plugins.max_plugins,
        })
    }
}

/// Plugin configuration structure
#[derive(Debug, Clone)]
pub struct PluginConfig {
    /// Plugin search directories
    pub search_paths: Vec<PathBuf>,
    /// Auto-load plugins on startup
    pub auto_load: bool,
    /// Plugin-specific configurations
    pub plugins: HashMap<String, serde_json::Value>,
    /// Enable hot reloading of plugins
    pub hot_reload: bool,
    /// Maximum number of plugins to load
    pub max_plugins: Option<usize>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            search_paths: vec![
                PathBuf::from("./plugins"),
                PathBuf::from("/usr/local/lib/mcp-rs/plugins"),
            ],
            auto_load: true,
            plugins: HashMap::new(),
            hot_reload: false,
            max_plugins: Some(50),
        }
    }
}
