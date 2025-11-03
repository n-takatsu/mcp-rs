mod config;
mod core;
mod error;
mod handlers;
mod mcp;
mod protocol;
mod security;
mod server;
mod transport;
mod types;

use config::McpConfig;
use core::{PluginInfo, Runtime, RuntimeConfig};
use error::Error;
use handlers::WordPressHandler;
use mcp::McpServer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // コマンドライン引数チェック
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--generate-config" {
        McpConfig::generate_sample_config()?;
        return Ok(());
    }

    // 設定を読み込み
    let config = McpConfig::load()?;

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

    // Create MCP server with runtime
    let mut server = McpServer::new();

    // Handler Registry を取得してWordPressハンドラーを登録
    let handler_registry = runtime.handler_registry();

    // WordPressハンドラーを追加（設定がある場合）
    if let Some(wp_config) = &config.handlers.wordpress {
        if wp_config.enabled.unwrap_or(true) {
            println!("🔗 WordPress統合を有効化: {}", wp_config.url);

            let wordpress_handler = WordPressHandler::try_new(wp_config.clone())
                .map_err(|e| Error::Internal(format!("WordPress handler initialization failed: {}", e)))?;
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
            println!("⚠️  WordPress統合は無効になっています");
        }
    } else {
        println!("ℹ️  WordPress設定が見つかりません");
        println!("💡 --generate-config でサンプル設定ファイルを生成できます");
    }

    // Run server
    if config.server.stdio.unwrap_or(false) {
        println!("📞 STDIO モードで待機中...");
        server.run_stdio().await?;
    } else {
        let addr = config
            .server
            .bind_addr
            .as_deref()
            .unwrap_or("127.0.0.1:8080");
        println!("🌍 TCP サーバーを開始: http://{}", addr);
        server.run(addr).await?;
    }

    // Graceful shutdown
    runtime.shutdown().await?;

    Ok(())
}
