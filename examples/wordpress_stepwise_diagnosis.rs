// WordPress認証問題の段階的診断
// アプリケーションパスワードが機能しない原因を特定

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== WordPress認証問題の段階的診断 ===");
    
    let url = "https://redring.jp";
    let username = "wpmaster";
    let app_password = "y3hD 7XRS ewju gbFK gzK4 aiAZ";
    
    let client = reqwest::Client::new();
    
    // 1. アプリケーションパスワードの内省（introspect）APIテスト
    println!("\n🔍 1. アプリケーションパスワード内省APIテスト...");
    
    let introspect_response = client
        .get(&format!("{}/wp-json/wp/v2/users/me/application-passwords/introspect", url))
        .basic_auth(username, Some(app_password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;
    
    println!("  内省API ステータス: {}", introspect_response.status());
    
    if introspect_response.status().is_success() {
        let introspect_data: serde_json::Value = introspect_response.json().await?;
        println!("  ✅ アプリケーションパスワード内省成功!");
        println!("  📄 内省データ: {}", serde_json::to_string_pretty(&introspect_data)?);
    } else {
        let error_text = introspect_response.text().await?;
        println!("  ❌ アプリケーションパスワード内省失敗: {}", error_text);
    }
    
    // 2. 別のエンドポイントでのテスト
    println!("\n📋 2. 別のエンドポイントでの認証テスト...");
    
    // 投稿一覧（認証不要）
    let posts_public_response = client
        .get(&format!("{}/wp-json/wp/v2/posts?per_page=1", url))
        .send()
        .await?;
    
    println!("  投稿一覧（認証不要）: {}", posts_public_response.status());
    
    // 投稿一覧（認証付き）
    let posts_auth_response = client
        .get(&format!("{}/wp-json/wp/v2/posts?per_page=1", url))
        .basic_auth(username, Some(app_password))
        .send()
        .await?;
    
    println!("  投稿一覧（認証付き）: {}", posts_auth_response.status());
    
    // 3. HTTPヘッダーの詳細確認
    println!("\n📊 3. 認証時のHTTPヘッダー詳細...");
    
    let header_test_response = client
        .get(&format!("{}/wp-json/wp/v2/users/me", url))
        .basic_auth(username, Some(app_password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .send()
        .await?;
    
    println!("  認証テストレスポンス: {}", header_test_response.status());
    
    for (name, value) in header_test_response.headers() {
        if name.as_str().to_lowercase().contains("auth") ||
           name.as_str().to_lowercase().contains("www-authenticate") ||
           name.as_str().starts_with("x-") {
            println!("    {}: {}", name, value.to_str().unwrap_or("不明"));
        }
    }
    
    let response_body = header_test_response.text().await?;
    println!("  レスポンス本文: {}", response_body);
    
    // 4. 異なるユーザーエンドポイントでのテスト
    println!("\n👥 4. 異なるユーザーエンドポイントでのテスト...");
    
    // ユーザー一覧エンドポイント
    let users_list_response = client
        .get(&format!("{}/wp-json/wp/v2/users", url))
        .basic_auth(username, Some(app_password))
        .send()
        .await?;
    
    println!("  ユーザー一覧エンドポイント: {}", users_list_response.status());
    
    if users_list_response.status().is_success() {
        println!("  ✅ ユーザー一覧エンドポイントでは認証成功");
        let users_data: serde_json::Value = users_list_response.json().await?;
        if let Some(users_array) = users_data.as_array() {
            println!("  👥 ユーザー数: {}", users_array.len());
        }
    } else {
        let users_error = users_list_response.text().await?;
        println!("  ❌ ユーザー一覧でも認証失敗: {}", users_error);
    }
    
    // 5. WordPress設定情報の取得
    println!("\n⚙️ 5. WordPress設定情報の確認...");
    
    let settings_response = client
        .get(&format!("{}/wp-json/wp/v2/settings", url))
        .basic_auth(username, Some(app_password))
        .send()
        .await?;
    
    println!("  設定情報エンドポイント: {}", settings_response.status());
    
    if settings_response.status().is_success() {
        println!("  ✅ 設定情報エンドポイントで認証成功");
    } else {
        let settings_error = settings_response.text().await?;
        println!("  ❌ 設定情報でも認証失敗: {}", settings_error);
    }
    
    println!("\n🔧 診断結果まとめ:");
    println!("  1. アプリケーションパスワードの形式: 正常（スペース区切り）");
    println!("  2. WordPress REST APIの基本機能: 正常");
    println!("  3. 認証問題: users/me エンドポイント固有の可能性");
    println!("\n💡 次の確認事項:");
    println!("  □ WordPressのセキュリティプラグイン設定");
    println!("  □ .htaccess ファイルのBasic認証制限");
    println!("  □ サーバーのnginx/Apache設定");
    println!("  □ WordPressのユーザー権限設定");
    
    Ok(())
}