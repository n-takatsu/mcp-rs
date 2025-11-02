use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;
use mcp_rs::mcp::McpHandler;
use std::env;
use std::time::Instant;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定を初期化
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
    println!("🕒 WordPress接続タイムアウトテスト");

    let wordpress_url =
        env::var("WORDPRESS_URL").unwrap_or_else(|_| "https://redring.jp".to_string());
    let username = env::var("WORDPRESS_USERNAME").ok();
    let password = env::var("WORDPRESS_PASSWORD").ok();

    println!("📍 テスト対象: {}", wordpress_url);
    println!("👤 ユーザー名: {}", username.as_deref().unwrap_or("未設定"));
    println!();

    // MCP-RSのWordPressHandlerを使用
    let handler = mcp_rs::handlers::WordPressHandler::new(wordpress_url, username, password);

    // 1. 正常な接続テスト
    println!("1️⃣  正常接続テスト（タイムアウト: 30秒）...");
    let start = Instant::now();

    match handler.list_tools().await {
        Ok(tools) => {
            let elapsed = start.elapsed();
            println!(
                "   ✅ 成功 ({}ms) - 利用可能ツール: {}",
                elapsed.as_millis(),
                tools.len()
            );
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("   ❌ 失敗 ({}ms): {}", elapsed.as_millis(), e);
        }
    }

    // 2. 存在しないホストでタイムアウトテスト
    println!("\n2️⃣  タイムアウトテスト（存在しないホスト）...");
    let timeout_handler = mcp_rs::handlers::WordPressHandler::new(
        "https://this-domain-definitely-does-not-exist-12345.com".to_string(),
        None,
        None,
    );

    let start = Instant::now();
    match timeout_handler.list_tools().await {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("   😮 予期しない成功 ({}ms)", elapsed.as_millis());
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("   ✅ 期待通りの失敗 ({}ms): {}", elapsed.as_millis(), e);
            if elapsed.as_secs() <= 15 {
                println!("   👍 タイムアウトが適切に動作しています");
            } else {
                println!("   ⚠️  タイムアウトが遅すぎる可能性があります");
            }
        }
    }

    // 3. 遅いレスポンスシミュレーション（httpbin.org使用）
    println!("\n3️⃣  遅いレスポンステスト...");
    let slow_handler = mcp_rs::handlers::WordPressHandler::new(
        "https://httpbin.org/delay/5".to_string(), // 5秒遅延
        None,
        None,
    );

    let start = Instant::now();
    match slow_handler.list_tools().await {
        Ok(_) => {
            let elapsed = start.elapsed();
            println!("   😮 予期しない成功 ({}ms)", elapsed.as_millis());
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("   ⏱️  失敗 ({}ms): {}", elapsed.as_millis(), e);
            if elapsed.as_secs() >= 5 && elapsed.as_secs() <= 8 {
                println!("   👍 適切にレスポンス待機して失敗しました");
            }
        }
    }

    println!("\n📊 タイムアウト設定まとめ:");
    println!("   • 接続タイムアウト: 10秒");
    println!("   • 全体タイムアウト: 30秒");
    println!("   • リトライ回数: 3回");
    println!("   • リトライ間隔: 1秒（指数関数的増加）");
    println!("   • User-Agent: mcp-rs/1.0");

    Ok(())
}
