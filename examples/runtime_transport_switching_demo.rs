//! Runtime STDIO/HTTP切り替えデモ
//!
//! 既存のmain.rsに統合可能な動的Transport切り替え実装例

use mcp_rs::config::{DynamicConfigManager, McpConfig};
use mcp_rs::runtime_control::{InteractiveController, RuntimeCommand, RuntimeController};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 基本的な初期化（既存のmain.rsと同様）
    let config = load_config().await?;
    init_logging(&config).await?;

    info!("🚀 MCP-RS v0.15.1 - Dynamic Transport Edition");

    // 動的設定管理の初期化
    let config_manager = Arc::new(DynamicConfigManager::new(config.clone(), get_config_path()));

    // デフォルトのTransportConfigを作成
    let transport_config = mcp_rs::transport::TransportConfig::default();

    // ランタイムコントローラー初期化
    let (runtime_controller, command_sender) =
        RuntimeController::new(transport_config, config_manager.clone())?;

    info!("🎛️ Runtime Control開始");
    info!("💡 実行中にSTDIO/HTTP切り替え可能");

    // バックグラウンドでRuntime Controllerを実行
    let runtime_task = tokio::spawn(async move {
        if let Err(e) = runtime_controller.run().await {
            error!("Runtime controller error: {}", e);
        }
    });

    // インタラクティブコントロールの開始（オプション）
    let _interactive_controller = InteractiveController::new(command_sender.clone());
    let interactive_task = tokio::spawn(async move {
        info!("🎮 Interactive Control利用可能");
        info!("💡 別ターミナルで制御可能、またはCLI引数で制御");

        // CLIからの制御例
        handle_cli_commands(command_sender).await;
    });

    // メインループ
    tokio::select! {
        _ = runtime_task => info!("Runtime task終了"),
        _ = interactive_task => info!("Interactive task終了"),
        _ = tokio::signal::ctrl_c() => {
            info!("🔄 終了シグナル受信");
        }
    }

    info!("👋 MCP-RS終了");
    Ok(())
}

/// CLI引数からの動的制御処理
async fn handle_cli_commands(command_sender: mpsc::Sender<RuntimeCommand>) {
    let args: Vec<String> = std::env::args().collect();

    // 実行時引数での制御例
    for arg in &args {
        match arg.as_str() {
            "--switch-stdio" => {
                info!("🔄 CLI: STDIO切り替え");
                let _ = command_sender.send(RuntimeCommand::SwitchToStdio).await;
            }
            "--switch-http" => {
                info!("🔄 CLI: HTTP切り替え");
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8081));
                let _ = command_sender
                    .send(RuntimeCommand::SwitchToHttp(addr))
                    .await;
            }
            "--reload-config" => {
                info!("🔄 CLI: 設定リロード");
                let _ = command_sender.send(RuntimeCommand::ReloadConfig).await;
            }
            "--status" => {
                info!("📊 CLI: ステータス表示");
                let _ = command_sender.send(RuntimeCommand::ShowStatus).await;
            }
            _ => {}
        }
    }
}

/// 設定ファイル読み込み（既存のものと同じ）
async fn load_config() -> Result<McpConfig, Box<dyn std::error::Error>> {
    McpConfig::load()
}

/// 設定ファイルパスを取得
fn get_config_path() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(config_index) = args.iter().position(|arg| arg == "--config") {
        args.get(config_index + 1).cloned()
    } else {
        None
    }
}

/// ログ初期化（既存のものと同じ）
async fn init_logging(_config: &McpConfig) -> Result<(), Box<dyn std::error::Error>> {
    // 既存のログ初期化ロジック
    tracing_subscriber::fmt::init();
    Ok(())
}

/// 使用例とCLIオプション表示
#[allow(dead_code)]
fn show_usage_examples() {
    println!("🎯 MCP-RS Runtime Control Examples:");
    println!();
    println!("基本起動:");
    println!("  cargo run");
    println!();
    println!("STDIO切り替え:");
    println!("  cargo run -- --switch-stdio");
    println!();
    println!("HTTP切り替え:");
    println!("  cargo run -- --switch-http");
    println!();
    println!("設定リロード:");
    println!("  cargo run -- --reload-config");
    println!();
    println!("ステータス確認:");
    println!("  cargo run -- --status");
    println!();
    println!("設定ファイル指定:");
    println!("  cargo run -- --config mcp-config-claude.toml");
    println!();
    println!("Claude Desktop用STDIO設定:");
    println!("  cargo run -- --config mcp-config-claude.toml --switch-stdio");
    println!();
}
