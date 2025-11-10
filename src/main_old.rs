//! MCP-RS Binary Entry Point
//!
//! This binary provides the main executable for the MCP-RS server.

#![allow(dead_code)] // Allow unused code for future extensibility
#![allow(unused_imports)] // Allow unused imports during development

mod config;
mod core;
mod error;
mod handlers;
mod mcp;
mod protocol;
mod security;
mod server;
mod setup;
mod transport;
mod types;

use config::McpConfig;
use core::{Runtime, RuntimeConfig};
use handlers::WordPressHandler;
use std::sync::Arc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 新しいTransport統合アーキテクチャの起動
    run_transport_integrated_server().await
}

/// 🚀 Transport統合型MCP-RSサーバー
async fn run_transport_integrated_server() -> Result<(), Box<dyn std::error::Error>> {
    // コマンドライン引数処理
    let args: Vec<String> = std::env::args().collect();
    let mut custom_config_path: Option<String> = None;

    // Parse command line arguments
    if args.len() > 1 {
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--generate-config" => {
                    McpConfig::generate_sample_config()?;
                    return Ok(());
                }
                "--setup-config" => {
                    setup_config_interactive().await?;
                    return Ok(());
                }
                "--demo-setup" => {
                    setup::DemoSetup::run_demo().await?;
                    return Ok(());
                }
                "--config" => {
                    if i + 1 < args.len() {
                        custom_config_path = Some(args[i + 1].clone());
                        i += 1; // Skip next argument as it's the config file path
                    } else {
                        eprintln!("❌ --config オプションには設定ファイルのパスが必要です");
                        return Err("Missing config file path".into());
                    }
                }
                "--switch-config" => {
                    // Load current config and run interactive switcher
                    let config = McpConfig::load()?;
                    let manager = Arc::new(DynamicConfigManager::new(config, None));
                    let switcher = ConfigSwitcher::new(manager);
                    switcher.run_interactive_switch().await?;
                    return Ok(());
                }
                "--reload-config" => {
                    println!("🔄 設定リロード機能はサーバー実行中のみ利用可能です");
                    println!("💡 サーバー起動後に --switch-config を使用してください");
                    return Ok(());
                }
                "--help" | "-h" => {
                    print_help();
                    return Ok(());
                }
                _ => {}
            }
            i += 1;
        }
    }

    // 設定を読み込み（カスタムパスまたはデフォルト）
    let config = match custom_config_path {
        Some(path) => {
            match load_config_from_file(&path).await {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("❌ 設定ファイル読み込みエラー: {}", e);
                    return Err(e.into());
                }
            }
        }
        None => match McpConfig::load() {
            Ok(config) => config,
            Err(_) => {
                // 設定ファイルが見つからない場合
                if !config_file_exists() {
                    println!("⚠️  設定ファイルが見つかりません。");
                    println!();
                    println!("📋 設定オプション:");
                    println!("  1. 対話的セットアップを実行: --setup-config");
                    println!("  2. サンプル設定を生成: --generate-config");
                    println!("  3. デフォルト設定で続行");
                    println!();

                    if should_run_interactive_setup()? {
                        setup_config_interactive().await?;
                        // セットアップ完了後に設定を再読み込み
                        McpConfig::load()?
                    } else {
                        println!("ℹ️  デフォルト設定で続行します。");
                        McpConfig::default()
                    }
                } else {
                    return Err("設定ファイルの読み込みに失敗しました".into());
                }
            }
        },
    };

    // Core Runtime を初期化
    let runtime_config = RuntimeConfig {
        mcp_config: config.clone(),
        max_concurrent_requests: 100,
        default_timeout_seconds: 30,
        enable_metrics: false,
    };

    let runtime = Runtime::new(runtime_config);

    // ログレベルを設定
    if let Some(log_level) = &config.server.log_level {
        std::env::set_var("RUST_LOG", log_level);
    }

    let is_stdio = config.server.stdio.unwrap_or(false);

    // Initialize logging - STDIOモードでは無効化
    if !is_stdio {
        tracing_subscriber::fmt::init();
    }

    // STDIOモード以外でのみログ出力
    if !is_stdio {
        println!("� MCP-RS サーバーを開始します...");
        println!("📡 モード: TCP サーバー");
        println!(
            "🌐 バインドアドレス: {}",
            config
                .server
                .bind_addr
                .as_deref()
                .unwrap_or("127.0.0.1:8080")
        );
    }

    // Runtime を初期化
    runtime.initialize().await?;

    // Create MCP server with runtime
    let mut server = crate::mcp::server::McpServer::new();

    // Handler Registry を取得してWordPressハンドラーを登録
    let handler_registry = runtime.handler_registry();

    // WordPressハンドラーを追加（設定がある場合）
    if let Some(wp_config) = &config.handlers.wordpress {
        if wp_config.enabled.unwrap_or(true) {
            if !is_stdio {
                println!("🔗 WordPress統合を有効化: {}", wp_config.url);
            }

            let wordpress_handler = WordPressHandler::try_new(wp_config.clone()).map_err(|e| {
                Error::Internal(format!("WordPress handler initialization failed: {}", e))
            })?;
            let plugin_info = PluginInfo::new(
                "wordpress".to_string(),
                "0.1.0".to_string(),
                "WordPress REST API integration".to_string(),
            );

            // Handler Registry に登録
            {
                let mut registry = handler_registry.write().await;
                registry.register_handler(
                    "wordpress".to_string(),
                    Arc::new(wordpress_handler.clone()),
                    plugin_info,
                )?;
            }

            // Legacy MCP Server にも追加（段階的移行のため）
            server.add_handler("wordpress".to_string(), Arc::new(wordpress_handler));
        } else {
            if !is_stdio {
                println!("⚠️  WordPress統合は無効になっています");
            }
        }
    } else {
        if !is_stdio {
            println!("ℹ️  WordPress設定が見つかりません");
            println!("💡 --generate-config でサンプル設定ファイルを生成できます");
        }
    }

    // Run server
    if config.server.stdio.unwrap_or(false) {
        // STDIO mode - STDIOサーバーを起動
        server.run_stdio().await?;
    } else {
        let addr = config
            .server
            .bind_addr
            .as_deref()
            .unwrap_or("127.0.0.1:8080");

        println!("🌐 HTTPとTCPの両サーバーを開始: http://{}", addr);
        println!("� TCP JSON-RPC: ライン区切りプロトコル (既存クライアント用)");
        println!("🌐 HTTP JSON-RPC: POST /mcp (AI Agent用)");
        println!("�💡 Ctrl+C で終了");

        // Transport統合アーキテクチャ: HTTPサーバーは transport layer で処理

        // Parse address for HTTP server (use different port to avoid conflict)
        let tcp_addr = addr;
        let http_port = if addr.contains(':') {
            let port: u16 = addr
                .split(':')
                .nth(1)
                .unwrap_or("8080")
                .parse()
                .unwrap_or(8080);
            port + 1 // HTTP server on next port
        } else {
            8081
        };
        let http_addr = format!("127.0.0.1:{}", http_port);

        println!("🔗 TCP サーバー: {}", tcp_addr);
        println!("🔗 HTTP サーバー: http://{}", http_addr);

        // 新しいTransport統合アーキテクチャを使用
        // RuntimeはTransportを通じて自動的にHTTPとTCPの両方を管理
        
        // 🚀 新しいTransport統合アーキテクチャ
        // RuntimeがTransportを通じて自動的にHTTPとTCPを管理
        
        println!("🔄 Runtime初期化中...");
        if let Err(e) = runtime.initialize().await {
            error!("❌ Runtime初期化失敗: {}", e);
            return Err(Box::new(e));
        }

        println!("✅ Runtime初期化完了");
        println!("🎯 新しいTransport統合モードで動作中");
        println!("💡 Ctrl+C で終了");

        // メインイベントループ: Transport経由でリクエスト処理
        let runtime_task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                while runtime.is_ready().await {
                    // Transport経由でメッセージを処理
                    // 実際の処理は Runtime内部で自動実行
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        });

        // 終了シグナル待機
        tokio::select! {
            _ = runtime_task => println!("🔄 Runtime処理が終了しました"),
            _ = tokio::signal::ctrl_c() => println!("\n🔄 終了シグナルを受信しました"),
        }
    }

    // Graceful shutdown
    runtime.shutdown().await?;

    Ok(())
}

/// Load configuration from specific file
async fn load_config_from_file(path: &str) -> Result<McpConfig, Error> {
    if !std::path::Path::new(path).exists() {
        return Err(Error::Config(format!(
            "設定ファイルが存在しません: {}",
            path
        )));
    }

    let settings = ::config::Config::builder()
        .add_source(::config::Config::try_from(&McpConfig::default())?)
        .add_source(::config::File::with_name(path))
        .build()?;

    let mut loaded_config: McpConfig = settings.try_deserialize()?;

    // Apply environment variable expansion for WordPress config
    if let Some(ref mut wp_config) = loaded_config.handlers.wordpress {
        McpConfig::expand_wordpress_config(wp_config);
    }

    Ok(loaded_config)
}

/// Print help message
fn print_help() {
    println!("🚀 MCP-RS - Model Context Protocol Server");
    println!();
    println!("使用方法:");
    println!("  mcp-rs [オプション]");
    println!();
    println!("オプション:");
    println!("  --config <file>      指定された設定ファイルを使用");
    println!("  --generate-config    サンプル設定ファイルを生成");
    println!("  --setup-config       対話的設定セットアップを実行");
    println!("  --demo-setup         デモンストレーション モードで実行");
    println!("  --switch-config      設定ファイルの動的切り替え");
    println!("  --reload-config      設定の再読み込み (実行中のみ)");
    println!("  --help, -h           このヘルプメッセージを表示");
    println!();
    println!("例:");
    println!("  mcp-rs                              # デフォルト設定で起動");
    println!("  mcp-rs --config custom.toml        # カスタム設定で起動");
    println!("  mcp-rs --setup-config              # 対話的設定作成");
    println!("  mcp-rs --switch-config              # 動的設定変更");
    println!();
    println!("設定ファイル:");
    println!("  デフォルト検索順: mcp-config.toml, config.toml, config/mcp.toml");
    println!();
}

/// Check if any configuration file exists
fn config_file_exists() -> bool {
    let config_paths = [
        "mcp-config.toml",
        "config.toml",
        "config/mcp.toml",
        "~/.config/mcp-rs/config.toml",
    ];

    config_paths
        .iter()
        .any(|path| std::path::Path::new(path).exists())
}

/// Ask user if they want to run interactive setup
fn should_run_interactive_setup() -> Result<bool, Box<dyn std::error::Error>> {
    use std::io::{self, Write};

    let mut retry_count = 0;
    const MAX_RETRIES: u32 = 3;

    loop {
        print!("対話的セットアップを実行しますか？ [Y/n]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                // EOF reached, default to no
                println!("デフォルト設定で続行します。");
                return Ok(false);
            }
            Ok(_) => {
                let input = input.trim().to_lowercase();

                match input.as_str() {
                    "" | "y" | "yes" => return Ok(true),
                    "n" | "no" => return Ok(false),
                    _ => {
                        retry_count += 1;
                        if retry_count >= MAX_RETRIES {
                            println!("⚠️  最大試行回数に達しました。デフォルト設定で続行します。");
                            return Ok(false);
                        }
                        println!("⚠️  'y' または 'n' で答えてください。");
                    }
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
}
