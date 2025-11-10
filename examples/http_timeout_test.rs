use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;
use mcp_rs::mcp::{McpHandler, ToolCallParams};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 詳細なログ設定を初期化
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("🌐 WordPress HTTP接続タイムアウトテスト開始");

    // 設定ファイルから読み込み
    let config = match McpConfig::load() {
        Ok(config) => {
            info!("✅ 設定ファイル読み込み成功");
            if let Some(ref wp_config) = config.handlers.wordpress {
                debug!("WordPress URL: {}", wp_config.url);
                debug!("Username: {}", wp_config.username);
                debug!("Timeout: {}秒", wp_config.timeout_seconds.unwrap_or(30));
            }
            config
        }
        Err(e) => {
            error!("❌ 設定ファイル読み込み失敗: {}", e);
            return Err(e);
        }
    };

    println!("\n=== WordPress HTTP接続タイムアウトテスト ===");
    if let Some(ref wp_config) = config.handlers.wordpress {
        println!("WordPress URL: {}", wp_config.url);
        println!("Username: {}", wp_config.username);
        println!("Timeout: {}秒", wp_config.timeout_seconds.unwrap_or(30));
    } else {
        error!("❌ WordPress設定が見つかりません");
        return Err("WordPress設定が見つかりません".into());
    }

    // テスト1: 通常のHTTP接続テスト
    test_normal_http_connection(&config).await;

    // テスト2: 存在しないホストへのタイムアウトテスト
    test_nonexistent_host_http().await;

    // テスト3: 無効なURLでのタイムアウトテスト
    test_invalid_url_http().await;

    info!("🏁 すべてのHTTPタイムアウトテストが完了しました");
    Ok(())
}

async fn test_normal_http_connection(config: &McpConfig) {
    info!("📋 テスト1: 通常のWordPress HTTP接続");
    let start = Instant::now();

    if let Some(wp_config) = &config.handlers.wordpress {
        let handler = WordPressHandler::new(wp_config.clone());
        info!("WordPressHandler作成完了");

        // 実際のHTTPリクエストを送信するメソッドを呼び出し
        let tool_params = ToolCallParams {
            name: "get_posts".to_string(),
            arguments: None,
        };

        match handler.call_tool(tool_params).await {
            Ok(result) => {
                let duration = start.elapsed();
                info!("✅ 正常HTTP接続成功 (所要時間: {:?})", duration);
                println!(
                    "   取得結果: {}",
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                );
            }
            Err(e) => {
                let duration = start.elapsed();
                error!("❌ 正常HTTP接続失敗 (所要時間: {:?}): {}", duration, e);
                println!("   エラー: {}", e);
            }
        }
    } else {
        error!("❌ WordPress設定が見つかりません");
    }
}

async fn test_nonexistent_host_http() {
    info!("📋 テスト2: 存在しないホストへのHTTP接続");
    let start = Instant::now();

    let fake_config = mcp_rs::config::WordPressConfig {
        url: "https://nonexistent-domain-12345.com".to_string(),
        username: "test".to_string(),
        password: "test".to_string(),
        enabled: Some(true),
        timeout_seconds: Some(5), // 短いタイムアウト
        rate_limit: None,
        encrypted_credentials: None, // 平文認証情報を使用
    };

    let handler = WordPressHandler::new(fake_config);
    info!("存在しないホスト用WordPressHandler作成完了");

    // 実際のHTTPリクエストを送信
    let tool_params = ToolCallParams {
        name: "get_posts".to_string(),
        arguments: None,
    };

    match handler.call_tool(tool_params).await {
        Ok(_) => {
            let duration = start.elapsed();
            warn!("⚠️ 予期しない成功 (所要時間: {:?})", duration);
            println!("   警告: 存在しないホストから応答がありました");
        }
        Err(e) => {
            let duration = start.elapsed();
            info!(
                "✅ 期待通りタイムアウト/エラー (所要時間: {:?}): {}",
                duration, e
            );
            println!("   タイムアウトエラー: {}", e);

            if duration < Duration::from_secs(10) {
                info!("✅ タイムアウトが適切に機能している (10秒未満)");
                println!("   ✓ タイムアウト時間が適切です");
            } else {
                warn!("⚠️ タイムアウトが長すぎる可能性");
                println!("   ⚠ タイムアウトが予想より長いです");
            }
        }
    }
}

async fn test_invalid_url_http() {
    info!("📋 テスト3: 無効なURLでのHTTP接続");
    let start = Instant::now();

    let fake_config = mcp_rs::config::WordPressConfig {
        url: "invalid-url-format".to_string(),
        username: "test".to_string(),
        password: "test".to_string(),
        enabled: Some(true),
        timeout_seconds: Some(3),
        rate_limit: None,
        encrypted_credentials: None, // 平文認証情報を使用
    };

    let handler = WordPressHandler::new(fake_config);
    info!("無効URL用WordPressHandler作成完了");

    // 実際のHTTPリクエストを送信
    let tool_params = ToolCallParams {
        name: "get_posts".to_string(),
        arguments: None,
    };

    match handler.call_tool(tool_params).await {
        Ok(_) => {
            let duration = start.elapsed();
            warn!("⚠️ 予期しない成功 (所要時間: {:?})", duration);
            println!("   警告: 無効なURLから応答がありました");
        }
        Err(e) => {
            let duration = start.elapsed();
            info!("✅ 期待通りエラー (所要時間: {:?}): {}", duration, e);
            println!("   URLエラー: {}", e);

            if duration < Duration::from_secs(5) {
                info!("✅ URLエラーが迅速に検出されました");
                println!("   ✓ エラー検出が高速です");
            }
        }
    }
}
