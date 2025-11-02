// 簡易WordPress接続確認
// 設定ファイルからWordPress認証のみをテスト

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== 簡易WordPress認証テスト ===");
    
    // 設定ファイルから読み込み
    let config_content = std::fs::read_to_string("mcp-config.toml")?;
    println!("✅ 設定ファイル読み込み成功");
    
    // TOMLをパース
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
    
    println!("URL: {}", url);
    println!("ユーザー名: {}", username);
    println!("パスワード: {}...", &password[..std::cmp::min(4, password.len())]);
    
    // HTTP クライアント作成
    let client = reqwest::Client::new();
    
    // 認証テスト
    println!("\n🔐 認証テスト実行中...");
    let response = client
        .get(&format!("{}/wp-json/wp/v2/users/me", url))
        .basic_auth(username, Some(password))
        .send()
        .await?;
    
    println!("レスポンスステータス: {}", response.status());
    
    if response.status().is_success() {
        let user_data: serde_json::Value = response.json().await?;
        println!("✅ 認証成功！");
        
        if let Some(name) = user_data.get("name") {
            println!("ユーザー名: {}", name);
        }
        if let Some(email) = user_data.get("email") {
            println!("メール: {}", email);
        }
        
        println!("\n🎉 新しいアプリケーションパスワードが正常に動作しています！");
        
    } else {
        println!("❌ 認証失敗");
        let error_text = response.text().await?;
        println!("エラー詳細: {}", error_text);
        
        println!("\n🔧 確認事項:");
        println!("1. アプリケーションパスワードが正しく入力されているか");
        println!("2. ユーザー名が正しいか");
        println!("3. WordPressサイトが正常に動作しているか");
        println!("4. REST APIが有効になっているか");
    }
    
    Ok(())
}