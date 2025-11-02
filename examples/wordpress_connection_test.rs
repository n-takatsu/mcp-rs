// WordPress接続テスト専用プログラム
// アプリケーションパスワードの接続確認を行います

use reqwest;
use serde_json::Value;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== WordPress接続テスト開始 ===");
    
    // 設定を環境変数から取得（セキュリティ上、直接コードに書かない）
    let url = std::env::var("WORDPRESS_URL")
        .unwrap_or_else(|_| "https://redring.jp".to_string());
    let username = std::env::var("WORDPRESS_USERNAME")
        .unwrap_or_else(|_| "wpmaster".to_string());
    let password = std::env::var("WORDPRESS_PASSWORD")
        .expect("環境変数 WORDPRESS_PASSWORD を設定してください");

    println!("WordPressサイト: {}", url);
    println!("ユーザー名: {}", username);
    
    // HTTPクライアントを作成（Basic認証付き）
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    println!("\n1. サイト情報の取得テスト...");
    
    // WordPressのREST APIエンドポイントをテスト
    let response = client
        .get(&format!("{}/wp-json/wp/v2/", url))
        .basic_auth(&username, Some(&password))
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
        .get(&format!("{}/wp-json/wp/v2/users/me", url))
        .basic_auth(&username, Some(&password))
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
    } else {
        println!("❌ 認証失敗: {}", auth_response.status());
        let auth_error = auth_response.text().await?;
        println!("認証エラー詳細: {}", auth_error);
        return Err("WordPress認証に失敗しました".into());
    }

    println!("\n3. 投稿一覧取得テスト...");
    
    // 投稿一覧を取得してテスト
    let posts_response = client
        .get(&format!("{}/wp-json/wp/v2/posts?per_page=3", url))
        .basic_auth(&username, Some(&password))
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
            }
        }
    } else {
        println!("❌ 投稿取得失敗: {}", posts_response.status());
        let posts_error = posts_response.text().await?;
        println!("投稿取得エラー: {}", posts_error);
    }

    println!("\n=== 接続テスト完了 ===");
    println!("✅ WordPressとの接続が正常に確認できました！");
    
    Ok(())
}