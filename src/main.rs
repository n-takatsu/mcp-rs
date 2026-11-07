//! MCP-RS Transport統合型エントリポイント
//!
//! 新しいTransport抽象化を使用したクリーンなアーキテクチャ

#![allow(dead_code)]
#![allow(unused_imports)]
// src/lib.rsと同じ理由（async_traitのマクロ展開によるclippyの誤検出）。
// このバイナリは下記のモジュールを独自に読み込むため、lib.rs側の許可は
// 効かない。
#![allow(clippy::double_must_use)]

mod config;
mod core;
mod error;
mod handlers;
mod logging;
mod mcp;
mod protocol;
mod security;
mod server;
mod session;
mod transport;
mod types;
mod zero_trust;

use mcp_rs::config::McpConfig;
use mcp_rs::handlers::WordPressHandler;
use mcp_rs::logging::{init_logging, LogConfig};
use std::sync::Arc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // サブコマンドは設定ファイルを読み込む前に処理する（設定ファイルが
    // まだ無い状態でencrypted_credentialsを作るためのコマンドなので）
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(mcp_rs::security::credential_tool::SUBCOMMAND) {
        // Box<dyn Error>をmainから返すとDebug表記（"..."付き）で出るため、
        // ここでDisplay表記で出して終了コード1にする
        if let Err(e) = mcp_rs::security::credential_tool::run(&args[2..]) {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    // 設定読み込み（ログ設定のため最初に実行）
    let config = load_config().await?;

    // ログシステム初期化
    // `_log_guard`はプロセス終了まで保持する必要がある（dropすると
    // バックグラウンドの書き込みスレッドが終了し、以後のログが
    // 実際にはファイルへ届かなくなる）。`let _ = ...`で捨てないこと。
    let log_config = LogConfig::from_server_config(&config.server);
    let _log_guard = init_logging(&log_config)?;

    info!(
        "🚀 MCP-RS v{} - Transport統合アーキテクチャ",
        env!("CARGO_PKG_VERSION")
    );
    // 実際のログ出力先（フォールバック解決後）はinit_logging内部で
    // 既に1箇所だけ表示済み（"📂 ログ出力先: ..."）。ここで
    // log_config.log_dir（未解決の設定値）を別に表示すると、
    // フォールバックが発生した場合に2つの異なるパスが出て矛盾する。
    info!("✅ 設定読み込み完了");

    // どちらのモードも、ハンドラー（WordPress・プラグイン）を登録した
    // McpServerでリクエストを処理する。HTTPモードは以前core::Runtime経由
    // だったが、受信ループが動いておらずリクエストを処理できなかった（#400）
    let transport_config = config.to_transport_config();
    match transport_config.transport_type {
        mcp_rs::transport::TransportType::Http { addr } => {
            let mut http_config = transport_config.http.clone();
            http_config.bind_addr = addr;
            run_http_server(&config, http_config).await
        }
        _ => {
            if let Some(other) = config
                .transport
                .transport_type
                .as_deref()
                .filter(|t| *t != "stdio")
            {
                tracing::warn!(
                    "Unknown transport type '{}': running in STDIO mode. Use \"stdio\" or \"http\"",
                    other
                );
            }
            run_stdio_server(&config).await
        }
    }
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
    let mut config: McpConfig =
        toml::from_str(&content).map_err(|e| mcp_rs::config::with_config_hint(e, &content))?;

    // WordPressConfig の環境変数展開を適用（McpConfig::load() と同様の処理）
    if let Some(ref mut wp_config) = config.handlers.wordpress {
        McpConfig::expand_wordpress_config(wp_config);
        wp_config.resolve_encrypted_credentials()?;
    }

    info!("✅ カスタム設定読み込み: {}", path);
    Ok(config)
}

/// 設定に従ってハンドラー（WordPress・プラグイン）を登録したMCPサーバーを作る
async fn build_server(
    config: &McpConfig,
    http_mode: bool,
) -> Result<mcp_rs::mcp::McpServer, Box<dyn std::error::Error>> {
    let mut server = mcp_rs::mcp::McpServer::new();

    if let Some(wp_config) = &config.handlers.wordpress {
        if wp_config.enabled.unwrap_or(false) {
            let wp_handler = WordPressHandler::try_new(wp_config.clone())
                .map_err(|e| format!("WordPress handler creation failed: {}", e))?;

            // STDIOと違い、HTTP/WebSocketのクライアントは別のマシンにいる
            // ことがある。その場合、file_pathで読まれるのはクライアントの
            // 手元ではなく、このサーバー上のファイルになる
            if http_mode && !wp_config.upload_allowed_dirs.is_empty() {
                tracing::warn!(
                    "upload_allowed_dirs is set: clients connected over HTTP/WebSocket can upload files from these folders on this server via upload_media file_path"
                );
            }

            server.add_handler("wordpress".to_string(), Arc::new(wp_handler));
            info!("✅ WordPressハンドラー登録完了: {}", wp_config.url);
        }
    }

    // プラグイン（#394）。起動できなかったプラグインは警告を出して飛ばす
    for (name, handler) in mcp_rs::plugin_proxy::start_plugins(&config.plugins).await {
        server.add_handler(format!("plugin:{}", name), handler);
    }

    Ok(server)
}

/// STDIOモードでMCPサーバーを実行（initialize / tools/list / tools/call 等に応答）
async fn run_stdio_server(config: &McpConfig) -> Result<(), Box<dyn std::error::Error>> {
    let server = build_server(config, false).await?;
    info!("🔌 STDIOモードで待機中");
    server.run_stdio().await
}

/// HTTPモードでMCPサーバーを実行する。`/mcp`（と`/`）へのPOST、および
/// `enable_websocket_upgrade`が有効なら`/ws`のWebSocketで、STDIOと同じ
/// 処理（McpServer）に応答させる（#400）。Ctrl+Cで終了する。
async fn run_http_server(
    config: &McpConfig,
    http_config: mcp_rs::transport::HttpConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = build_server(config, true).await?;

    let transport = mcp_rs::transport::HttpTransport::new(http_config)?
        .with_request_handler(Arc::new(server.dispatcher()));
    transport.start_server().await?;
    if let Some(addr) = transport.bound_addr().await {
        info!("🌐 HTTPモードで待機中: http://{}/mcp", addr);
    }
    info!("💡 Ctrl+C で終了");

    tokio::signal::ctrl_c().await?;
    info!("👋 MCP-RS終了");
    // serverを破棄すると、プラグインの子プロセスも終了する
    drop(server);
    Ok(())
}
