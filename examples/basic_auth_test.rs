// WordPress基本認証テスト（通常のログインパスワード使用）
// アプリケーションパスワード以外での認証可能性をテスト

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== WordPress基本認証テスト ===");
    println!("⚠️  注意: このテストは通常のWordPressログインパスワードを使用します");
    println!("    （アプリケーションパスワードではありません）");
    
    // 環境変数から通常のパスワードを取得（セキュリティのため）
    let normal_password = std::env::var("WP_LOGIN_PASSWORD")
        .unwrap_or_else(|_| {
            println!("環境変数 WP_LOGIN_PASSWORD が設定されていません");
            println!("通常のWordPressログインパスワードを環境変数に設定してください:");
            println!("$env:WP_LOGIN_PASSWORD=\"your_normal_password\"");
            std::process::exit(1);
        });
    
    let url = "https://redring.jp";
    let username = "wpmaster";
    
    println!("URL: {}", url);
    println!("ユーザー名: {}", username);
    println!("パスワード: 通常ログインパスワード（環境変数から取得）");
    
    let client = reqwest::Client::new();
    
    // 通常のログインパスワードでBasic認証テスト
    println!("\n🔐 通常パスワードでREST API認証テスト...");
    
    let response = client
        .get(&format!("{}/wp-json/wp/v2/users/me", url))
        .basic_auth(username, Some(&normal_password))
        .header("User-Agent", "MCP-RS/0.1.0")
        .send()
        .await?;
    
    println!("ステータス: {}", response.status());
    
    if response.status().is_success() {
        let user_data: serde_json::Value = response.json().await?;
        println!("✅ 通常パスワードでの認証成功！");
        
        if let Some(name) = user_data.get("name") {
            println!("ユーザー名: {}", name);
        }
        if let Some(roles) = user_data.get("roles") {
            println!("権限: {}", roles);
        }
        
        println!("\n💡 結論: WordPressの基本認証は機能している");
        println!("    → アプリケーションパスワード機能に固有の問題がある可能性");
        
    } else {
        let error_text = response.text().await?;
        println!("❌ 通常パスワードでも認証失敗");
        println!("エラー詳細: {}", error_text);
        
        println!("\n💡 結論: WordPress REST API認証が全般的に制限されている");
        println!("    → セキュリティプラグインやサーバー設定の問題の可能性");
    }
    
    Ok(())
}