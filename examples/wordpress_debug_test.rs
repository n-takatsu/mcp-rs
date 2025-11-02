// WordPress詳細診断テスト
// より詳細な情報でWordPress接続をデバッグ

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== WordPress詳細診断テスト ===");
    
    // 設定ファイルから読み込み
    let config_content = std::fs::read_to_string("mcp-config.toml")?;
    let config: toml::Value = toml::from_str(&config_content)?;
    
    let wp_config = config
        .get("handlers")
        .and_then(|h| h.get("wordpress"))
        .ok_or("WordPress設定が見つかりません")?;
    
    let url = wp_config.get("url")
        .and_then(|u| u.as_str())
        .ok_or("URLが設定されていません")?;
    
    let username = wp_config.get("username")
        .and_then(|u| u.as_str())
        .ok_or("ユーザー名が設定されていません")?;
    
    let password = wp_config.get("password")
        .and_then(|p| p.as_str())
        .ok_or("パスワードが設定されていません")?;
    
    println!("📋 設定情報:");
    println!("  URL: {}", url);
    println!("  ユーザー名: {}", username);
    println!("  パスワード長: {} 文字", password.len());
    println!("  パスワード形式: {}", if password.contains(' ') { "スペース含む" } else { "スペースなし" });
    
    let client = reqwest::Client::new();
    
    // 1. WordPressサイトの基本確認
    println!("\n🌐 1. WordPressサイト基本確認...");
    match client.get(url).send().await {
        Ok(response) => {
            println!("  ✅ サイトアクセス: {} ({})", response.status(), response.status().canonical_reason().unwrap_or("不明"));
        }
        Err(e) => {
            println!("  ❌ サイトアクセス失敗: {}", e);
            return Err(e.into());
        }
    }
    
    // 2. REST API基本確認
    println!("\n🔌 2. REST API基本確認...");
    let api_url = format!("{}/wp-json/wp/v2/", url);
    match client.get(&api_url).send().await {
        Ok(response) => {
            println!("  ✅ REST API: {} ({})", response.status(), response.status().canonical_reason().unwrap_or("不明"));
            if response.status().is_success() {
                if let Ok(json) = response.json::<serde_json::Value>().await {
                    if let Some(name) = json.get("name") {
                        println!("  📝 サイト名: {}", name);
                    }
                    if let Some(namespaces) = json.get("namespaces") {
                        println!("  🔧 API名前空間: {}", namespaces);
                    }
                }
            }
        }
        Err(e) => {
            println!("  ❌ REST API確認失敗: {}", e);
        }
    }
    
    // 3. 認証確認（複数のエンドポイントで試行）
    println!("\n🔐 3. 認証確認...");
    
    // 3.1 users/me エンドポイント
    println!("  📍 users/me エンドポイント...");
    let users_me_url = format!("{}/wp-json/wp/v2/users/me", url);
    let response = client
        .get(&users_me_url)
        .basic_auth(username, Some(password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;
    
    println!("    ステータス: {} ({})", response.status(), response.status().canonical_reason().unwrap_or("不明"));
    
    if response.status().is_success() {
        let user_data: serde_json::Value = response.json().await?;
        println!("    ✅ 認証成功！");
        if let Some(name) = user_data.get("name") {
            println!("    👤 ユーザー名: {}", name);
        }
        if let Some(roles) = user_data.get("roles") {
            println!("    🏷️  権限: {}", roles);
        }
        if let Some(id) = user_data.get("id") {
            println!("    🆔 ユーザーID: {}", id);
        }
    } else {
        let error_text = response.text().await?;
        println!("    ❌ 認証失敗");
        println!("    📄 エラー詳細: {}", error_text);
        
        // 3.2 別のエンドポイントで再試行
        println!("  📍 users エンドポイント（一般）...");
        let users_url = format!("{}/wp-json/wp/v2/users", url);
        let users_response = client
            .get(&users_url)
            .basic_auth(username, Some(password))
            .header("User-Agent", "MCP-RS/0.1.0")
            .send()
            .await?;
        
        println!("    ステータス: {} ({})", users_response.status(), users_response.status().canonical_reason().unwrap_or("不明"));
        
        if users_response.status().is_success() {
            println!("    ✅ 一般ユーザーエンドポイントは成功");
        } else {
            let users_error = users_response.text().await?;
            println!("    ❌ 一般ユーザーエンドポイントも失敗: {}", users_error);
        }
    }
    
    // 4. レスポンスヘッダー確認
    println!("\n📋 4. 詳細診断...");
    let diagnostic_response = client
        .get(&format!("{}/wp-json/wp/v2/users/me", url))
        .basic_auth(username, Some(password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;
    
    println!("  📊 レスポンスヘッダー:");
    for (name, value) in diagnostic_response.headers() {
        if name.as_str().to_lowercase().contains("www-authenticate") || 
           name.as_str().to_lowercase().contains("content-type") ||
           name.as_str().to_lowercase().contains("server") {
            println!("    {}: {}", name, value.to_str().unwrap_or("不明"));
        }
    }
    
    println!("\n💡 推奨対策:");
    println!("1. WordPress管理画面で新しいアプリケーションパスワードを生成");
    println!("2. ユーザー名が正確か確認（大文字小文字も含む）");
    println!("3. アプリケーションパスワード機能が有効か確認");
    println!("4. セキュリティプラグインがAPI接続をブロックしていないか確認");
    println!("5. WordPressのREST API設定を確認");
    
    Ok(())
}