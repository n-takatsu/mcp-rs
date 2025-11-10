use reqwest::Client;
use serde_json::json;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 WordPress API 接続テスト開始...\n");

    // 環境変数から設定を取得
    let wordpress_url = env::var("WORDPRESS_URL").unwrap_or_else(|_| {
        println!("⚠️  WORDPRESS_URL が設定されていません。デフォルト値を使用: http://localhost");
        "http://localhost".to_string()
    });

    let username = env::var("WORDPRESS_USERNAME").ok();
    let password = env::var("WORDPRESS_PASSWORD").ok();

    println!("📍 WordPress URL: {}", wordpress_url);
    println!("👤 ユーザー名: {}", username.as_deref().unwrap_or("未設定"));
    println!(
        "🔐 パスワード: {}",
        if password.is_some() {
            "設定済み"
        } else {
            "未設定"
        }
    );
    println!();

    let client = Client::new();

    // 1. WordPress REST API が利用可能かチェック
    println!("1️⃣  WordPress REST API 可用性チェック...");
    let api_url = format!("{}/wp-json/wp/v2", wordpress_url);

    match client.get(&api_url).send().await {
        Ok(response) => {
            println!("   ✅ REST API 利用可能 (Status: {})", response.status());

            if response.status().is_success() {
                if let Ok(api_info) = response.json::<serde_json::Value>().await {
                    if let Some(routes) = api_info.get("routes") {
                        println!(
                            "   📋 利用可能なAPIエンドポイント数: {}",
                            routes.as_object().map(|o| o.len()).unwrap_or(0)
                        );
                    }
                }
            }
        }
        Err(e) => {
            println!("   ❌ REST API 接続失敗: {}", e);
            println!("   💡 WordPressサイトが起動しているか確認してください");
            return Ok(());
        }
    }

    // 2. 公開投稿一覧の取得テスト
    println!("\n2️⃣  公開投稿一覧取得テスト...");
    let posts_url = format!("{}/wp-json/wp/v2/posts?per_page=3", wordpress_url);

    match client.get(&posts_url).send().await {
        Ok(response) => {
            println!("   ✅ 投稿一覧取得成功 (Status: {})", response.status());

            if let Ok(posts) = response.json::<serde_json::Value>().await {
                if let Some(posts_array) = posts.as_array() {
                    println!("   📝 取得した投稿数: {}", posts_array.len());

                    for (i, post) in posts_array.iter().enumerate() {
                        if let Some(title) = post.get("title").and_then(|t| t.get("rendered")) {
                            println!(
                                "      {}. {}",
                                i + 1,
                                title.as_str().unwrap_or("タイトル不明")
                            );
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("   ❌ 投稿一覧取得失敗: {}", e);
        }
    }

    // 3. 認証が必要な操作のテスト（設定されている場合）
    if let (Some(user), Some(pass)) = (&username, &password) {
        println!("\n3️⃣  認証テスト...");

        // 認証情報でユーザー情報を取得
        let me_url = format!("{}/wp-json/wp/v2/users/me", wordpress_url);

        match client
            .get(&me_url)
            .basic_auth(user, Some(pass))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    println!("   ✅ 認証成功");

                    if let Ok(user_info) = response.json::<serde_json::Value>().await {
                        if let Some(name) = user_info.get("name") {
                            println!(
                                "   👋 ログインユーザー: {}",
                                name.as_str().unwrap_or("不明")
                            );
                        }
                        if user_info.get("capabilities").is_some() {
                            println!("   🔑 ユーザー権限: 確認済み");
                        }
                    }
                } else {
                    println!("   ❌ 認証失敗 (Status: {})", response.status());
                    match response.status().as_u16() {
                        401 => println!("   💡 ユーザー名またはパスワードが間違っています"),
                        403 => println!("   💡 アクセス権限がありません"),
                        _ => println!("   💡 予期しないエラーです"),
                    }
                }
            }
            Err(e) => {
                println!("   ❌ 認証リクエスト失敗: {}", e);
            }
        }

        // 4. 投稿作成テスト（権限がある場合）
        println!("\n4️⃣  投稿作成テスト...");
        let create_url = format!("{}/wp-json/wp/v2/posts", wordpress_url);

        let test_post = json!({
            "title": "MCP-RS テスト投稿",
            "content": "この投稿は mcp-rs WordPress 統合のテストで作成されました。",
            "status": "draft"  // ドラフトとして作成
        });

        match client
            .post(&create_url)
            .basic_auth(user, Some(pass))
            .json(&test_post)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    println!("   ✅ テスト投稿作成成功");

                    if let Ok(created_post) = response.json::<serde_json::Value>().await {
                        if let Some(id) = created_post.get("id") {
                            println!("   📝 作成された投稿ID: {}", id);
                        }
                        if let Some(link) = created_post.get("link") {
                            println!("   🔗 投稿URL: {}", link.as_str().unwrap_or("不明"));
                        }
                    }
                } else {
                    println!("   ❌ 投稿作成失敗 (Status: {})", response.status());
                    match response.status().as_u16() {
                        401 => println!(
                            "   💡 認証エラー: Application Passwordsの設定を確認してください"
                        ),
                        403 => println!("   💡 権限エラー: 投稿作成権限がありません"),
                        _ => {
                            if let Ok(error_text) = response.text().await {
                                println!("   📄 エラー詳細: {}", error_text);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("   ❌ 投稿作成リクエスト失敗: {}", e);
            }
        }
    } else {
        println!("\n3️⃣  認証情報が設定されていないため、認証テストをスキップします");
        println!("   💡 認証テストを実行するには以下を設定してください:");
        println!("      export WORDPRESS_USERNAME=\"your_username\"");
        println!("      export WORDPRESS_PASSWORD=\"your_app_password\"");
    }

    println!("\n🎯 テスト完了");
    println!("\n📚 Application Passwords の設定方法:");
    println!("   1. WordPress管理画面 > ユーザー > プロフィール");
    println!("   2. 'アプリケーションパスワード' セクション");
    println!("   3. 新しいアプリケーションパスワードを生成");
    println!("   4. 生成されたパスワードを WORDPRESS_PASSWORD に設定");

    Ok(())
}
