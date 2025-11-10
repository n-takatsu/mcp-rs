//! MCP-RS Transport統合型エントリポイント
//!
//! 新しいTransport抽象化を使用したクリーンなアーキテクチャ

#![allow(dead_code)]
#![allow(unused_imports)]

mod config;
mod core;
mod error;
mod handlers;
mod mcp;
mod protocol;
mod runtime_control;
mod security;
mod server;
mod setup;
mod transport;
mod types;

use mcp_rs::config::McpConfig;
use mcp_rs::core::{Runtime, RuntimeConfig};
use mcp_rs::handlers::WordPressHandler;
use mcp_rs::logging::{init_logging, LogConfig};
use std::sync::Arc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 設定読み込み（ログ設定のため最初に実行）
    let config = load_config().await?;

    // ログシステム初期化
    let log_config = LogConfig::from_server_config(&config.server);
    init_logging(&log_config)?;

    info!("🚀 MCP-RS v0.15.1 - Transport統合アーキテクチャ");
    info!("📂 ログファイル場所: {}", log_config.log_dir.display());
    info!("✅ 設定読み込み完了");

    // Runtime初期化
    let runtime_config = RuntimeConfig {
        mcp_config: config.clone(),
        enable_metrics: true,
        default_timeout_seconds: 30,
        max_concurrent_requests: 100,
    };

    let runtime = Runtime::new(runtime_config);

    // ハンドラー登録
    register_handlers(&runtime, &config).await?;

    // Runtime開始
    info!("🔄 Transport統合ランタイム初期化中...");
    runtime.initialize().await.map_err(|e| {
        error!("Runtime初期化失敗: {}", e);
        Box::new(e) as Box<dyn std::error::Error>
    })?;

    info!("✅ MCP-RSサーバー起動完了");
    info!("💡 Ctrl+C で終了");

    // メインループ
    let runtime_arc = Arc::new(runtime);
    let main_task = tokio::spawn({
        let runtime = runtime_arc.clone();
        async move {
            while runtime.is_ready().await {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    });

    // 終了シグナル待機
    tokio::select! {
        _ = main_task => info!("🔄 メインタスク終了"),
        _ = tokio::signal::ctrl_c() => info!("🔄 終了シグナル受信"),
    }

    // Graceful shutdown
    runtime_arc
        .shutdown()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    info!("👋 MCP-RS終了");
    Ok(())
}

/// 設定ファイル読み込み（引数処理含む）
async fn load_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // --config引数処理
    if let Some(config_index) = args.iter().position(|arg| arg == "--config") {
        if let Some(path) = args.get(config_index + 1) {
            return load_config_from_file(path).await;
        }
    }

    // デフォルト設定読み込み
    McpConfig::load()
}

/// 指定パスから設定読み込み
async fn load_config_from_file(path: &str) -> Result<McpConfig, Box<dyn std::error::Error>> {
    if !std::path::Path::new(path).exists() {
        return Err(format!("設定ファイルが存在しません: {}", path).into());
    }

    let content = tokio::fs::read_to_string(path).await?;
    let config: McpConfig = toml::from_str(&content)?;
    info!("✅ カスタム設定読み込み: {}", path);
    Ok(config)
}

/// ハンドラー登録
async fn register_handlers(
    runtime: &Runtime,
    config: &McpConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let registry = runtime.handler_registry();

    // WordPressハンドラー
    if let Some(wp_config) = &config.handlers.wordpress {
        if wp_config.enabled.unwrap_or(false) {
            let wp_handler = WordPressHandler::try_new(wp_config.clone())
                .map_err(|e| format!("WordPress handler creation failed: {}", e))?;

            let mut registry_lock = registry.write().await;
            registry_lock
                .register_handler(
                    "wordpress".to_string(),
                    Arc::new(wp_handler),
                    mcp_rs::core::PluginInfo {
                        name: "WordPress Handler".to_string(),
                        version: "0.1.0".to_string(),
                        description: "WordPress REST API integration".to_string(),
                        author: Some("MCP-RS".to_string()),
                        config: None,
                        enabled: true,
                    },
                )
                .map_err(|e| format!("Failed to register WordPress handler: {}", e))?;

            info!("✅ WordPressハンドラー登録完了: {}", wp_config.url);
        }
    }

    Ok(())
}
