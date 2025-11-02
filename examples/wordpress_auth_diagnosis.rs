// WordPress認証方式とトークン設定の詳細診断
// アプリケーションパスワードとREST APIの設定状況を確認

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== WordPress認証方式・トークン設定診断 ===");
    
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
    
    println!("📋 認証設定:");
    println!("  URL: {}", url);
    println!("  ユーザー名: {}", username);
    println!("  認証方式: Application Password (Basic Auth)");
    println!("  パスワード形式: {}", if password.contains(' ') { "スペース区切り（標準）" } else { "連続文字" });
    
    let client = reqwest::Client::new();
    
    // 1. WordPress基本情報とREST API設定確認
    println!("\n🔍 1. WordPress基本情報・REST API設定確認...");
    
    let api_index_response = client
        .get(&format!("{}/wp-json/", url))
        .send()
        .await?;
    
    if api_index_response.status().is_success() {
        let api_data: serde_json::Value = api_index_response.json().await?;
        
        println!("  ✅ REST API基本接続: 成功");
        
        if let Some(namespaces) = api_data.get("namespaces") {
            println!("  📂 利用可能な名前空間: {}", namespaces);
        }
        
        if let Some(routes) = api_data.get("routes") {
            if let Some(routes_obj) = routes.as_object() {
                println!("  🛤️  利用可能なルート数: {}", routes_obj.len());
                
                // 認証関連のルートを確認
                for (route, _) in routes_obj {
                    if route.contains("users") || route.contains("auth") {
                        println!("    - {}", route);
                    }
                }
            }
        }
    } else {
        println!("  ❌ REST API基本接続失敗: {}", api_index_response.status());
    }
    
    // 2. 認証ヘッダーとレスポンス詳細分析
    println!("\n🔐 2. 認証詳細分析...");
    
    let auth_response = client
        .get(&format!("{}/wp-json/wp/v2/users/me", url))
        .basic_auth(username, Some(password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .header("Accept", "application/json")
        .send()
        .await?;
    
    println!("  📊 認証レスポンス詳細:");
    println!("    ステータス: {} ({})", auth_response.status(), auth_response.status().canonical_reason().unwrap_or("不明"));
    
    // レスポンスヘッダーの詳細確認
    println!("  📋 レスポンスヘッダー:");
    for (name, value) in auth_response.headers() {
        let name_str = name.as_str().to_lowercase();
        if name_str.contains("auth") || 
           name_str.contains("token") || 
           name_str.contains("expire") || 
           name_str.contains("cache") ||
           name_str.contains("x-") ||
           name_str.contains("server") {
            println!("    {}: {}", name, value.to_str().unwrap_or("不明"));
        }
    }
    
    let response_text = auth_response.text().await?;
    
    if response_text.contains("rest_not_logged_in") {
        println!("  ❌ 認証失敗: アプリケーションパスワード認証が拒否されました");
        
        // エラー詳細をパース
        if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&response_text) {
            if let Some(code) = error_json.get("code") {
                println!("    エラーコード: {}", code);
            }
            if let Some(message) = error_json.get("message") {
                println!("    エラーメッセージ: {}", message);
            }
            if let Some(data) = error_json.get("data") {
                println!("    追加データ: {}", data);
            }
        }
    } else {
        println!("  ✅ 認証成功");
    }
    
    // 3. アプリケーションパスワード機能の確認
    println!("\n🔑 3. アプリケーションパスワード機能確認...");
    
    // WordPressのアプリケーションパスワード関連APIを確認
    let app_password_check = client
        .get(&format!("{}/wp-json/wp/v2/", url))
        .basic_auth(username, Some(password))
        .send()
        .await?;
    
    if app_password_check.status().is_success() {
        println!("  ✅ Basic認証自体は機能している");
    } else {
        println!("  ❌ Basic認証が完全に拒否されている");
    }
    
    // 4. セキュリティプラグイン・設定の推測
    println!("\n🛡️ 4. セキュリティ設定推測...");
    
    // よくあるセキュリティヘッダーをチェック
    let security_check = client
        .head(url)
        .send()
        .await?;
    
    println!("  🔍 セキュリティ関連ヘッダー:");
    for (name, value) in security_check.headers() {
        let name_str = name.as_str().to_lowercase();
        if name_str.contains("security") || 
           name_str.contains("protection") || 
           name_str.contains("x-frame") ||
           name_str.contains("content-security") ||
           name_str.contains("x-") {
            println!("    {}: {}", name, value.to_str().unwrap_or("不明"));
        }
    }
    
    println!("\n💡 トークン・認証設定に関する推測:");
    println!("  1. アプリケーションパスワード: WordPressコア機能（通常無期限）");
    println!("  2. 24時間有効期限: JWT認証プラグインまたはOAuth設定の可能性");
    println!("  3. 現在の認証失敗: アプリケーションパスワード自体の問題");
    
    println!("\n🔧 確認すべき設定:");
    println!("  □ WordPress管理画面 > ユーザー > プロフィール > アプリケーションパスワード");
    println!("  □ プラグイン: JWT Authentication, OAuth設定");
    println!("  □ セキュリティプラグイン: Wordfence, SiteGuard WP Plugin等");
    println!("  □ .htaccess ファイルのBasic認証制限");
    println!("  □ サーバーレベルの認証制限");
    
    Ok(())
}