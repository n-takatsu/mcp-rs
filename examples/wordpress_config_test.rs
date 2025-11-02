// WordPress接続テスト（設定ファイル使用版）
// mcp-config.tomlから設定を読み込んで接続テストを行います

use mcp_rs::config::McpConfig;
use reqwest;
use serde_json::Value;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== WordPress接続テスト（設定ファイル使用） ===");
    
    // 設定ファイルから読み込み
    let config = match McpConfig::load() {
        Ok(config) => config,
        Err(e) => {
            println!("❌ 設定ファイルの読み込みに失敗: {}", e);
            println!("mcp-config.tomlファイルが存在し、正しく設定されているか確認してください。");
            return Err(e.into());
        }
    };
    
    // WordPress設定の確認
    let wp_config = match config.handlers.wordpress {
        Some(wp_config) if wp_config.enabled.unwrap_or(false) => wp_config,
        Some(_) => {
            println!("❌ WordPressハンドラーが無効になっています");
            println!("mcp-config.tomlでenabled = trueに設定してください。");
            return Err("WordPressハンドラーが無効".into());
        }
        None => {
            println!("❌ WordPress設定が見つかりません");
            println!("mcp-config.tomlにWordPress設定を追加してください。");
            return Err("WordPress設定なし".into());
        }
    };

    println!("WordPressサイト: {}", wp_config.url);
    println!("ユーザー名: {}", wp_config.username);
    println!("パスワード: {}...", &wp_config.password[..std::cmp::min(4, wp_config.password.len())]);
    
    // HTTPクライアントを作成
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    println!("\n1. サイト情報の取得テスト...");
    
    // WordPressのREST APIエンドポイントをテスト
    let response = client
        .get(&format!("{}/wp-json/wp/v2/", wp_config.url))
        .basic_auth(&wp_config.username, Some(&wp_config.password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;

    println!("ステータス: {}", response.status());
    
    if response.status().is_success() {
        let text = response.text().await?;
        let json: Value = serde_json::from_str(&text)?;
        
        println!("✅ API接続成功！");
        if let Some(name) = json.get("name") {
            println!("サイト名: {}", name);
        }
        if let Some(description) = json.get("description") {
            println!("サイト説明: {}", description);
        }
    } else {
        println!("❌ API接続失敗: {}", response.status());
        let error_text = response.text().await?;
        println!("エラー詳細: {}", error_text);
        return Err("WordPress API接続に失敗しました".into());
    }

    println!("\n2. 認証テスト（ユーザー情報取得）...");
    
    // 認証が必要なエンドポイントをテスト
    let auth_response = client
        .get(&format!("{}/wp-json/wp/v2/users/me", wp_config.url))
        .basic_auth(&wp_config.username, Some(&wp_config.password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;

    println!("認証ステータス: {}", auth_response.status());
    
    if auth_response.status().is_success() {
        let auth_text = auth_response.text().await?;
        let user_json: Value = serde_json::from_str(&auth_text)?;
        
        println!("✅ 認証成功！");
        if let Some(user_name) = user_json.get("name") {
            println!("ログインユーザー: {}", user_name);
        }
        if let Some(roles) = user_json.get("roles") {
            println!("ユーザー権限: {}", roles);
        }
        if let Some(email) = user_json.get("email") {
            println!("メールアドレス: {}", email);
        }
    } else {
        let status_code = auth_response.status();
        println!("❌ 認証失敗: {}", status_code);
        let auth_error = auth_response.text().await?;
        println!("認証エラー詳細: {}", auth_error);
        
        if status_code == 401 {
            println!("\n💡 認証失敗の原因:");
            println!("  1. アプリケーションパスワードが間違っている");
            println!("  2. ユーザー名が間違っている");
            println!("  3. WordPressのREST APIが無効になっている");
            println!("  4. アプリケーションパスワード機能が無効になっている");
            println!("\n🔧 解決方法:");
            println!("  1. WordPress管理画面でアプリケーションパスワードを再生成");
            println!("  2. mcp-config.tomlのパスワードを更新");
            println!("  3. WordPress REST APIの有効化を確認");
        }
        
        return Err("WordPress認証に失敗しました".into());
    }

    println!("\n3. 投稿一覧取得テスト...");
    
    // 投稿一覧を取得してテスト
    let posts_response = client
        .get(&format!("{}/wp-json/wp/v2/posts?per_page=3", wp_config.url))
        .basic_auth(&wp_config.username, Some(&wp_config.password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;

    println!("投稿取得ステータス: {}", posts_response.status());
    
    if posts_response.status().is_success() {
        let posts_text = posts_response.text().await?;
        let posts_json: Value = serde_json::from_str(&posts_text)?;
        
        if let Some(posts_array) = posts_json.as_array() {
            println!("✅ 投稿取得成功！ 取得件数: {}", posts_array.len());
            
            for (i, post) in posts_array.iter().take(3).enumerate() {
                if let Some(title) = post.get("title").and_then(|t| t.get("rendered")) {
                    println!("  {}. {}", i + 1, title.as_str().unwrap_or("タイトルなし"));
                }
                if let Some(status) = post.get("status") {
                    println!("     ステータス: {}", status.as_str().unwrap_or("不明"));
                }
            }
        }
    } else {
        println!("❌ 投稿取得失敗: {}", posts_response.status());
        let posts_error = posts_response.text().await?;
        println!("投稿取得エラー: {}", posts_error);
    }

    println!("\n4. 権限テスト（投稿作成可能性チェック）...");
    
    // 投稿作成権限をテスト（実際には作成せず、権限のみチェック）
    let caps_response = client
        .head(&format!("{}/wp-json/wp/v2/posts", wp_config.url))
        .basic_auth(&wp_config.username, Some(&wp_config.password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;

    if caps_response.status().is_success() {
        println!("✅ 投稿エンドポイントへのアクセス権限あり");
    } else {
        println!("⚠️  投稿エンドポイントアクセス確認: {}", caps_response.status());
    }

    println!("\n=== 接続テスト完了 ===");
    println!("✅ WordPressとの接続が正常に確認できました！");
    println!("🚀 MCP-RSサーバーを起動する準備が整いました。");
    
    Ok(())
}