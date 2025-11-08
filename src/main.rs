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
use core::{PluginInfo, Runtime, RuntimeConfig};
use error::Error;
use handlers::WordPressHandler;
// use mcp_rs::mcp_server::McpServer;
use security::{SecureMcpServer, SecurityConfig};
use setup::setup_config_interactive;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // コマンドライン引数チェック
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--generate-config" => {
                McpConfig::generate_sample_config()?;
                return Ok(());
            }
            "--setup-config" => {
                setup_config_interactive().await?;
                return Ok(());
            }
            _ => {}
        }
    }

    // 設定を読み込み（見つからない場合は対話的セットアップを提案）
    let config = match McpConfig::load() {
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

    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 MCP-RS サーバーを開始します...");

    // Runtime を初期化
    runtime.initialize().await?;

    // 設定情報を表示
    if config.server.stdio.unwrap_or(false) {
        println!("📡 モード: STDIO (MCP クライアント接続用)");
    } else {
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

    // Create MCP server with runtime (temporarily disabled)
    // let mut server = McpServer::new();

    // Handler Registry を取得してWordPressハンドラーを登録
    let handler_registry = runtime.handler_registry();

    // WordPressハンドラーを追加（設定がある場合）
    if let Some(wp_config) = &config.handlers.wordpress {
        if wp_config.enabled.unwrap_or(true) {
            println!("🔗 WordPress統合を有効化: {}", wp_config.url);

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
            // server.add_handler("wordpress".to_string(), Arc::new(wordpress_handler));
        } else {
            println!("⚠️  WordPress統合は無効になっています");
        }
    } else {
        println!("ℹ️  WordPress設定が見つかりません");
        println!("💡 --generate-config でサンプル設定ファイルを生成できます");
    }

    // Run server (temporarily disabled)
    if config.server.stdio.unwrap_or(false) {
        println!("📞 STDIO モードで待機中...");
        // server.run_stdio().await?;
    } else {
        let addr = config
            .server
            .bind_addr
            .as_deref()
            .unwrap_or("127.0.0.1:8080");
        // .parse()
        // .expect("Invalid address format");

        println!("� HTTP サーバー開始予定: http://{}", addr);
        println!(
            "💡 WebSocketサーバーの例を実行してください: cargo run --example axum_websocket_server"
        );

        // server.run(addr).await?;
    }

    // Graceful shutdown
    runtime.shutdown().await?;

    Ok(())
}

/// Check if any configuration file exists
fn config_file_exists() -> bool {
    let config_paths = [
        "mcp-config.toml",
        "config.toml",
        "config/mcp.toml",
        "~/.config/mcp-rs/config.toml",
    ];

    config_paths.iter().any(|path| std::path::Path::new(path).exists())
}

/// Ask user if they want to run interactive setup
fn should_run_interactive_setup() -> Result<bool, Box<dyn std::error::Error>> {
    use std::io::{self, Write};
    
    loop {
        print!("対話的セットアップを実行しますか？ [Y/n]: ");
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        
        match input.as_str() {
            "" | "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("⚠️  'y' または 'n' で答えてください。"),
        }
    }
}
