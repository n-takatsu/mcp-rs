mod mcp;
mod handlers;
mod config;

use std::sync::Arc;
use mcp::{McpServer};
use handlers::WordPressHandler;
use config::McpConfig;

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
    
    // ログレベルを設定
    if let Some(log_level) = &config.server.log_level {
        std::env::set_var("RUST_LOG", log_level);
    }
    
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 MCP-RS サーバーを開始します...");
    
    // 設定情報を表示
    if config.server.stdio.unwrap_or(false) {
        println!("📡 モード: STDIO (MCP クライアント接続用)");
    } else {
        println!("📡 モード: TCP サーバー");
        println!("🌐 バインドアドレス: {}", config.server.bind_addr.as_deref().unwrap_or("127.0.0.1:8080"));
    }

    // Create MCP server
    let mut server = McpServer::new();

    // WordPressハンドラーを追加（設定がある場合）
    if let Some(wp_config) = &config.handlers.wordpress {
        if wp_config.enabled.unwrap_or(true) {
            println!("🔗 WordPress統合を有効化: {}", wp_config.url);
            
            let wordpress_handler = WordPressHandler::new(
                wp_config.url.clone(),
                if wp_config.username.is_empty() { None } else { Some(wp_config.username.clone()) },
                if wp_config.password.is_empty() { None } else { Some(wp_config.password.clone()) },
            );

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
        let addr = config.server.bind_addr.as_deref().unwrap_or("127.0.0.1:8080");
        println!("🌍 TCP サーバーを開始: http://{}", addr);
        server.run(addr).await?;
    }

    Ok(())
}
