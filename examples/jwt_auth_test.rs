// JWT認証テスト
// JWT Authentication for WP REST APIプラグインを使用した認証テスト

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== JWT認証テスト ===");
    
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
    
    println!("📋 JWT認証設定:");
    println!("  URL: {}", url);
    println!("  ユーザー名: {}", username);
    
    let client = reqwest::Client::new();
    
    // 1. JWTトークン取得
    println!("\n🔑 1. JWTトークン取得...");
    
    let token_request = serde_json::json!({
        "username": username,
        "password": password
    });
    
    let token_response = client
        .post(&format!("{}/wp-json/jwt-auth/v1/token", url))
        .header("Content-Type", "application/json")
        .json(&token_request)
        .send()
        .await?;
    
    println!("  トークン取得レスポンス: {}", token_response.status());
    
    if token_response.status().is_success() {
        let token_data: serde_json::Value = token_response.json().await?;
        
        if let Some(token) = token_data.get("token") {
            let token_str = token.as_str().unwrap_or("");
            println!("  ✅ JWTトークン取得成功");
            println!("  🎫 トークン: {}...", &token_str[..std::cmp::min(20, token_str.len())]);
            
            if let Some(user_email) = token_data.get("user_email") {
                println!("  📧 ユーザーメール: {}", user_email);
            }
            if let Some(user_nicename) = token_data.get("user_nicename") {
                println!("  👤 ユーザー名: {}", user_nicename);
            }
            if let Some(user_display_name) = token_data.get("user_display_name") {
                println!("  🏷️  表示名: {}", user_display_name);
            }
            
            // 2. JWTトークンを使用してユーザー情報取得
            println!("\n🔐 2. JWTトークンでユーザー情報取得...");
            
            let auth_header = format!("Bearer {}", token_str);
            let user_response = client
                .get(&format!("{}/wp-json/wp/v2/users/me", url))
                .header("Authorization", &auth_header)
                .header("Content-Type", "application/json")
                .send()
                .await?;
            
            println!("  ユーザー情報取得: {}", user_response.status());
            
            if user_response.status().is_success() {
                let user_data: serde_json::Value = user_response.json().await?;
                println!("  ✅ JWT認証成功！");
                
                if let Some(name) = user_data.get("name") {
                    println!("  👤 ユーザー名: {}", name);
                }
                if let Some(roles) = user_data.get("roles") {
                    println!("  🏷️  権限: {}", roles);
                }
                if let Some(id) = user_data.get("id") {
                    println!("  🆔 ユーザーID: {}", id);
                }
                
                // 3. 投稿一覧取得テスト
                println!("\n📄 3. JWT認証で投稿一覧取得...");
                
                let posts_response = client
                    .get(&format!("{}/wp-json/wp/v2/posts?per_page=3", url))
                    .header("Authorization", &auth_header)
                    .send()
                    .await?;
                
                if posts_response.status().is_success() {
                    let posts_data: serde_json::Value = posts_response.json().await?;
                    if let Some(posts_array) = posts_data.as_array() {
                        println!("  ✅ 投稿取得成功！ 件数: {}", posts_array.len());
                        
                        for (i, post) in posts_array.iter().take(3).enumerate() {
                            if let Some(title) = post.get("title").and_then(|t| t.get("rendered")) {
                                println!("    {}. {}", i + 1, title.as_str().unwrap_or("タイトルなし"));
                            }
                        }
                    }
                } else {
                    println!("  ❌ 投稿取得失敗: {}", posts_response.status());
                }
                
                println!("\n🎉 JWT認証が正常に動作しています！");
                println!("💡 MCP-RSをJWT認証モードで実装することをお勧めします。");
                
            } else {
                let error_text = user_response.text().await?;
                println!("  ❌ JWT認証でのユーザー情報取得失敗: {}", error_text);
            }
            
        } else {
            println!("  ❌ JWTトークンが含まれていません");
        }
        
    } else {
        let error_text = token_response.text().await?;
        println!("  ❌ JWTトークン取得失敗");
        println!("  📄 エラー詳細: {}", error_text);
        
        // JWTでも失敗した場合、アプリケーションパスワードを使用したWordPressの通常ログインパスワードの可能性
        println!("\n💡 確認事項:");
        println!("  1. 現在のパスワードがWordPressの通常ログインパスワードか");
        println!("  2. JWTプラグインの設定が正しいか");
        println!("  3. wp-config.phpでJWT_AUTH_SECRET_KEYが設定されているか");
    }
    
    Ok(())
}