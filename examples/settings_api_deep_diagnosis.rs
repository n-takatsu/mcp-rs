use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;
use reqwest::Client;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 WordPress 設定API 詳細診断");
    println!("=======================================");

    // 設定読み込み
    let config = McpConfig::load()?;

    if let Some(wp_config) = config.handlers.wordpress {
        println!("📍 診断対象:");
        println!("   URL: {}", wp_config.url);
        println!("   Username: {}", wp_config.username);

        let _handler = WordPressHandler::new(wp_config.clone());

        // 1. 直接HTTPクライアントでのテスト
        println!("\n🔍 Phase 1: 直接HTTP認証テスト");

        let client = Client::new();
        let settings_url = format!("{}/wp-json/wp/v2/settings", wp_config.url);

        println!("   URL: {}", settings_url);

        let response = client
            .get(&settings_url)
            .basic_auth(&wp_config.username, Some(&wp_config.password))
            .header("User-Agent", "mcp-rs/1.0")
            .send()
            .await;

        match response {
            Ok(resp) => {
                println!("   ✅ HTTP接続成功");
                let status = resp.status();
                println!("   📊 ステータスコード: {}", status);
                println!("   📋 レスポンスヘッダー:");

                for (name, value) in resp.headers() {
                    if name.as_str().to_lowercase().contains("auth")
                        || name.as_str().to_lowercase().contains("www")
                        || name.as_str().to_lowercase().contains("content")
                    {
                        println!("      {}: {:?}", name, value);
                    }
                }

                if status.is_success() {
                    let text = resp.text().await?;
                    if text.len() > 100 {
                        println!("   📄 レスポンス: {}...", &text[..100]);
                    } else {
                        println!("   📄 レスポンス: {}", text);
                    }
                } else {
                    let text = resp.text().await.unwrap_or_default();
                    println!("   ❌ エラーレスポンス: {}", text);

                    // 401エラーの詳細分析
                    if status == 401 {
                        println!("   🔍 401 Unauthorized 詳細分析:");
                        if text.contains("rest_not_logged_in") {
                            println!("      → WordPress REST API 認証エラー");
                        } else if text.contains("application_password") {
                            println!("      → アプリケーションパスワード関連エラー");
                        } else if text.contains("capability") {
                            println!("      → 権限不足エラー");
                        } else if text.contains("nonce") {
                            println!("      → Nonce検証エラー");
                        } else {
                            println!("      → 不明な認証エラー");
                        }
                    }
                }
            }
            Err(e) => {
                println!("   ❌ HTTP接続失敗: {}", e);
            }
        }

        // 2. WordPress REST API 認証情報確認
        println!("\n🔍 Phase 2: REST API 認証情報確認");

        let auth_url = format!("{}/wp-json/wp/v2/users/me", wp_config.url);
        println!("   認証確認URL: {}", auth_url);

        let auth_response = client
            .get(&auth_url)
            .basic_auth(&wp_config.username, Some(&wp_config.password))
            .header("User-Agent", "mcp-rs/1.0")
            .send()
            .await;

        match auth_response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let user_data: Result<Value, _> = resp.json().await;
                    match user_data {
                        Ok(user) => {
                            println!("   ✅ 認証情報有効");
                            if let Some(name) = user.get("name") {
                                println!("      ユーザー名: {}", name);
                            }
                            if let Some(roles) = user.get("roles") {
                                println!("      権限: {:?}", roles);
                            }
                            if let Some(capabilities) = user.get("capabilities") {
                                println!("      capabilities確認...");
                                if let Some(manage_options) = capabilities.get("manage_options") {
                                    println!("      manage_options: {}", manage_options);
                                }
                            }
                        }
                        Err(e) => {
                            println!("   ⚠️ JSON解析エラー: {}", e);
                        }
                    }
                } else {
                    println!("   ❌ 認証情報無効: {}", resp.status());
                }
            }
            Err(e) => {
                println!("   ❌ 認証確認失敗: {}", e);
            }
        }

        // 3. WordPress バージョン確認
        println!("\n🔍 Phase 3: WordPress バージョン・機能確認");

        let root_url = format!("{}/wp-json/", wp_config.url);
        println!("   WordPressルートAPI: {}", root_url);

        let root_response = client
            .get(&root_url)
            .header("User-Agent", "mcp-rs/1.0")
            .send()
            .await;

        match root_response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let root_data: Result<Value, _> = resp.json().await;
                    match root_data {
                        Ok(data) => {
                            if let Some(namespaces) = data.get("namespaces") {
                                println!("   ✅ 利用可能な名前空間: {:?}", namespaces);
                            }
                            if let Some(routes) = data.get("routes") {
                                if let Some(settings_route) = routes.get("/wp/v2/settings") {
                                    println!("   ✅ 設定APIルート存在確認");
                                    println!("      設定: {:?}", settings_route);
                                } else {
                                    println!("   ❌ 設定APIルートが見つかりません");
                                }
                            }
                        }
                        Err(e) => {
                            println!("   ⚠️ ルートAPI解析エラー: {}", e);
                        }
                    }
                } else {
                    println!("   ❌ ルートAPI取得失敗: {}", resp.status());
                }
            }
            Err(e) => {
                println!("   ❌ ルートAPI接続失敗: {}", e);
            }
        }

        println!("\n📊 診断結果まとめ:");
        println!("   🎯 SiteGuard確認: 404ではなく401エラーのため、SiteGuardが原因ではない");
        println!("   🔍 調査継続項目:");
        println!("      • WordPress REST API設定の詳細確認");
        println!("      • アプリケーションパスワードの権限スコープ");
        println!("      • WordPress バージョン固有の制限");
        println!("      • プラグインによる設定API制限");
    } else {
        println!("❌ WordPress設定が見つかりません");
    }

    Ok(())
}
