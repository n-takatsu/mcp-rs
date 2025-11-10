use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔬 MCP-RS 包括的テスト");
    println!("=====================================");

    // 1. 設定ファイルの読み込みテスト
    println!("\n1️⃣  設定ファイル読み込みテスト");
    let config = match McpConfig::load() {
        Ok(config) => {
            println!("   ✅ 設定ファイル読み込み成功");
            config
        }
        Err(e) => {
            println!("   ❌ 設定ファイル読み込み失敗: {}", e);
            return Err(e);
        }
    };

    // 2. 環境変数展開テスト
    println!("\n2️⃣  環境変数展開テスト");
    if let Some(wp_config) = &config.handlers.wordpress {
        println!("   📝 WordPress設定:");
        println!("      URL: {}", wp_config.url);
        println!("      Username: {}", wp_config.username);
        println!(
            "      Password: {}***",
            &wp_config.password.chars().take(8).collect::<String>()
        );

        // 環境変数が正しく設定されているかチェック
        let env_vars = ["TEST_WP_URL", "TEST_WP_USER", "TEST_WP_PASS"];
        for var in &env_vars {
            match env::var(var) {
                Ok(value) => {
                    let display_value = if var.contains("PASS") {
                        format!("{}***", &value.chars().take(8).collect::<String>())
                    } else {
                        value
                    };
                    println!("   ✅ {}: {}", var, display_value);
                }
                Err(_) => println!("   ⚠️  {}: 未設定", var),
            }
        }
    } else {
        println!("   ❌ WordPress設定が見つかりません");
        return Ok(());
    }

    // 3. WordPressハンドラー初期化テスト
    println!("\n3️⃣  WordPressハンドラー初期化テスト");
    let handler = if let Some(wp_config) = config.handlers.wordpress.clone() {
        println!("   ✅ WordPressハンドラー初期化成功");
        WordPressHandler::new(wp_config)
    } else {
        println!("   ❌ WordPress設定が見つかりません");
        return Ok(());
    };

    // 4. ヘルスチェック実行
    println!("\n4️⃣  WordPress ヘルスチェック実行");
    println!("   ⏱️  タイムアウト: 30秒");

    let health_check = handler.health_check().await;
    println!("   📊 ヘルスチェック結果:");
    println!(
        "      総合ステータス: {}",
        if health_check.site_accessible
            && health_check.rest_api_available
            && health_check.authentication_valid
            && health_check.permissions_adequate
            && health_check.media_upload_possible
        {
            "✅ 正常"
        } else {
            "⚠️ 問題あり"
        }
    );
    println!(
        "      サイトアクセス: {}",
        if health_check.site_accessible {
            "✅"
        } else {
            "❌"
        }
    );
    println!(
        "      REST API: {}",
        if health_check.rest_api_available {
            "✅"
        } else {
            "❌"
        }
    );
    println!(
        "      認証: {}",
        if health_check.authentication_valid {
            "✅"
        } else {
            "❌"
        }
    );
    println!(
        "      権限: {}",
        if health_check.permissions_adequate {
            "✅"
        } else {
            "❌"
        }
    );
    println!(
        "      メディアアップロード: {}",
        if health_check.media_upload_possible {
            "✅"
        } else {
            "❌"
        }
    );

    if !health_check.error_details.is_empty() {
        println!("   🚨 検出された問題:");
        for (i, issue) in health_check.error_details.iter().enumerate() {
            println!("      {}. {}", i + 1, issue);
        }
    }

    if let Some(site_info) = &health_check.site_info {
        println!("   ℹ️  サイト情報:");
        println!("      名前: {}", site_info.name);
        println!("      説明: {}", site_info.description);
        println!("      URL: {}", site_info.url);
        if let Some(email) = &site_info.admin_email {
            println!("      管理者メール: {}", email);
        }
    }

    // 5. 基本API呼び出しテスト
    println!("\n5️⃣  基本API呼び出しテスト");

    // WordPressの設定取得
    println!("   ⚙️  WordPress設定取得中...");
    match handler.get_settings().await {
        Ok(settings) => {
            println!("      ✅ WordPress設定取得成功");
            if let Some(title) = &settings.title {
                println!("         サイトタイトル: {}", title);
            }
            if let Some(desc) = &settings.description {
                println!("         サイト説明: {}", desc);
            }
        }
        Err(e) => {
            println!("      ❌ WordPress設定取得失敗: {}", e);
        }
    }

    // カテゴリー一覧取得
    println!("   📂 カテゴリー一覧取得中...");
    match handler.get_categories().await {
        Ok(categories) => {
            println!("      ✅ カテゴリー取得成功 ({}件)", categories.len());
            for cat in categories.iter().take(3) {
                println!("         - {} (ID: {:?})", cat.name, cat.id);
            }
        }
        Err(e) => {
            println!("      ❌ カテゴリー取得失敗: {}", e);
        }
    }

    println!("\n🎯 テスト完了!");
    println!("=====================================");

    Ok(())
}
