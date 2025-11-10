use base64::{engine::general_purpose, Engine as _};
use reqwest::Client;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 WordPress セキュリティ診断 & トラブルシューティング\n");

    let wordpress_url =
        env::var("WORDPRESS_URL").unwrap_or_else(|_| "http://localhost".to_string());
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

    // 1. WordPressサイトの基本接続確認
    println!("1️⃣  基本接続テスト...");
    match client.get(&wordpress_url).send().await {
        Ok(response) => {
            println!("   ✅ サイト接続成功 (Status: {})", response.status());

            // レスポンスヘッダーを確認
            let headers = response.headers();
            if let Some(server) = headers.get("server") {
                println!("   🖥️  サーバー: {:?}", server);
            }
            if let Some(security) = headers.get("x-frame-options") {
                println!(
                    "   🔒 セキュリティヘッダー検出: X-Frame-Options = {:?}",
                    security
                );
            }
        }
        Err(e) => {
            println!("   ❌ サイト接続失敗: {}", e);
            return Ok(());
        }
    }

    // 2. REST API Discovery
    println!("\n2️⃣  REST API Discovery...");
    let discovery_url = format!("{}/wp-json", wordpress_url);

    match client.get(&discovery_url).send().await {
        Ok(response) => {
            println!("   ℹ️  Discovery Status: {}", response.status());

            if response.status().is_success() {
                if let Ok(discovery_data) = response.json::<serde_json::Value>().await {
                    if let Some(namespaces) = discovery_data.get("namespaces") {
                        println!("   📋 利用可能な名前空間:");
                        if let Some(ns_array) = namespaces.as_array() {
                            for ns in ns_array {
                                println!("      - {}", ns.as_str().unwrap_or("不明"));
                            }
                        }
                    }

                    if let Some(authentication) = discovery_data.get("authentication") {
                        println!("   🔑 認証情報: {:?}", authentication);
                    }
                }
            }
        }
        Err(e) => {
            println!("   ❌ Discovery失敗: {}", e);
        }
    }

    // 3. 詳細なエラー情報取得
    println!("\n3️⃣  詳細エラー診断...");
    let api_url = format!("{}/wp-json/wp/v2", wordpress_url);

    match client.get(&api_url).send().await {
        Ok(response) => {
            println!("   📊 API Root Status: {}", response.status());

            let headers = response.headers();
            println!("   📋 レスポンスヘッダー:");

            // 重要なヘッダーをチェック
            let important_headers = [
                "www-authenticate",
                "x-wp-nonce",
                "access-control-allow-origin",
                "x-robots-tag",
                "x-content-type-options",
                "x-frame-options",
                "content-security-policy",
            ];

            for header_name in &important_headers {
                if let Some(value) = headers.get(*header_name) {
                    println!("      {}: {:?}", header_name, value);
                }
            }

            // エラーレスポンスの詳細を取得
            if !response.status().is_success() {
                let response_text = response.text().await.unwrap_or_default();
                println!("   📄 エラーレスポンス内容:");

                // HTMLレスポンスの場合、タイトルを抽出
                if response_text.contains("<title>") {
                    if let Some(start) = response_text.find("<title>") {
                        if let Some(end) = response_text[start..].find("</title>") {
                            let title = &response_text[start + 7..start + end];
                            println!("      ページタイトル: {}", title);
                        }
                    }
                }

                // セキュリティプラグインの兆候をチェック
                let security_indicators = [
                    ("Wordfence", "wordfence"),
                    ("SiteGuard", "siteguard"),
                    ("iThemes Security", "ithemes"),
                    ("Sucuri", "sucuri"),
                    ("Cloudflare", "cloudflare"),
                    ("All In One WP Security", "aiowps"),
                ];

                let response_lower = response_text.to_lowercase();
                for (plugin_name, indicator) in &security_indicators {
                    if response_lower.contains(indicator) {
                        println!("      🛡️  検出されたセキュリティ: {}", plugin_name);
                    }
                }

                // 短いレスポンスの場合は全文表示
                if response_text.len() < 500 {
                    println!("      内容: {}", response_text);
                }
            }
        }
        Err(e) => {
            println!("   ❌ API診断失敗: {}", e);
        }
    }

    // 4. User-Agent テスト
    println!("\n4️⃣  User-Agent テスト...");
    let test_agents = [
        ("Standard", "mcp-rs/1.0"),
        (
            "Browser-like",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        ),
        ("WordPress App", "WordPress/6.0; https://wordpress.org"),
    ];

    for (agent_type, user_agent) in &test_agents {
        match client
            .get(&api_url)
            .header("User-Agent", *user_agent)
            .send()
            .await
        {
            Ok(response) => {
                println!(
                    "   {} User-Agent: {} (Status: {})",
                    if response.status().is_success() {
                        "✅"
                    } else {
                        "❌"
                    },
                    agent_type,
                    response.status()
                );
            }
            Err(_) => {
                println!("   ❌ {} User-Agent: 接続失敗", agent_type);
            }
        }
    }

    // 5. 認証ヘッダーテスト（認証情報がある場合）
    if let (Some(user), Some(pass)) = (&username, &password) {
        println!("\n5️⃣  認証方式テスト...");

        // Basic認証のヘッダーを手動で作成
        let auth_string = format!("{}:{}", user, pass);
        let auth_b64 = general_purpose::STANDARD.encode(auth_string);
        let auth_header = format!("Basic {}", auth_b64);

        println!(
            "   🔐 Basic認証ヘッダー: Authorization: Basic [{}文字]",
            auth_b64.len()
        );

        let me_url = format!("{}/wp-json/wp/v2/users/me", wordpress_url);

        // 様々な認証方式でテスト
        let auth_tests = [
            ("reqwest basic_auth", None),
            ("Manual Authorization header", Some(auth_header.clone())),
        ];

        for (test_name, manual_header) in &auth_tests {
            let mut request = client.get(&me_url);

            if let Some(header_value) = manual_header {
                request = request.header("Authorization", header_value);
            } else {
                request = request.basic_auth(user, Some(pass));
            }

            match request.send().await {
                Ok(response) => {
                    println!(
                        "   {} {}: Status {}",
                        if response.status().is_success() {
                            "✅"
                        } else {
                            "❌"
                        },
                        test_name,
                        response.status()
                    );

                    if !response.status().is_success() {
                        if let Ok(error_text) = response.text().await {
                            if error_text.len() < 200 {
                                println!("      エラー: {}", error_text.trim());
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("   ❌ {}: {}", test_name, e);
                }
            }
        }
    }

    println!("\n📚 トラブルシューティング推奨事項:");
    println!("   1. セキュリティプラグインのREST API設定を確認");
    println!("   2. .htaccessファイルでREST APIアクセスが制限されていないか確認");
    println!("   3. サーバーレベルでのIP制限や地域制限を確認");
    println!("   4. WordPressの「設定 > パーマリンク」で構造を再保存");
    println!("   5. Application Passwordsが正しく生成されているか確認");

    Ok(())
}
