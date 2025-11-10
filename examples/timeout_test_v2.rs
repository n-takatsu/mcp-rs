use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;
use mcp_rs::mcp::McpHandler;
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

    info!("🕒 WordPress接続タイムアウトテスト開始");

    // 設定ファイルから読み込み
    let config = match McpConfig::load() {
        Ok(config) => {
            info!("✅ 設定ファイル読み込み成功");
            if let Some(ref wp_config) = config.handlers.wordpress {
                debug!("WordPress URL: {}", wp_config.url);
                debug!("Username: {}", wp_config.username);
            }
            config
        }
        Err(e) => {
            error!("❌ 設定ファイル読み込み失敗: {}", e);
            return Err(e);
        }
    };

    println!("\n=== WordPress接続タイムアウトテスト ===");
    if let Some(ref wp_config) = config.handlers.wordpress {
        println!("WordPress URL: {}", wp_config.url);
        println!("Username: {}", wp_config.username);
    } else {
        error!("❌ WordPress設定が見つかりません");
        return Err("WordPress設定が見つかりません".into());
    }

    // テスト1: 通常の接続テスト
    test_normal_connection(&config).await;

    // テスト2: 存在しないホストへのタイムアウトテスト
    test_nonexistent_host().await;

    // テスト3: 無効なURLでのタイムアウトテスト
    test_invalid_url().await;

    info!("🏁 すべてのタイムアウトテストが完了しました");
    Ok(())
}

async fn test_normal_connection(config: &McpConfig) {
    info!("📋 テスト1: 通常のWordPress接続");
    let start = Instant::now();

    if let Some(wp_config) = &config.handlers.wordpress {
        let handler = WordPressHandler::new(wp_config.clone());
        info!("WordPressHandler作成完了");

        match handler.list_tools().await {
            Ok(tools) => {
                let duration = start.elapsed();
                info!("✅ 正常接続成功 (所要時間: {:?})", duration);
                println!("   ツール数: {}", tools.len());
                for tool in tools.iter().take(3) {
                    println!("   - {}: {}", tool.name, tool.description);
                }
                if tools.len() > 3 {
                    println!("   ... 他{}個のツール", tools.len() - 3);
                }
            }
            Err(e) => {
                let duration = start.elapsed();
                error!("❌ 正常接続失敗 (所要時間: {:?}): {}", duration, e);
                println!("   エラー: {}", e);
            }
        }
    } else {
        error!("❌ WordPress設定が見つかりません");
    }
}

async fn test_nonexistent_host() {
    info!("📋 テスト2: 存在しないホストへの接続");
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

    match handler.list_tools().await {
        Ok(_) => {
            let duration = start.elapsed();
            warn!("⚠️ 予期しない成功 (所要時間: {:?})", duration);
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
            } else {
                warn!("⚠️ タイムアウトが長すぎる可能性");
            }
        }
    }
}

async fn test_invalid_url() {
    info!("📋 テスト3: 無効なURLでの接続");
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

    match handler.list_tools().await {
        Ok(_) => {
            let duration = start.elapsed();
            warn!("⚠️ 予期しない成功 (所要時間: {:?})", duration);
        }
        Err(e) => {
            let duration = start.elapsed();
            info!("✅ 期待通りエラー (所要時間: {:?}): {}", duration, e);
            println!("   URLエラー: {}", e);
        }
    }
}
