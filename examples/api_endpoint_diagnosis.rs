use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 WordPress API エンドポイント詳細診断");
    println!("==========================================");

    // 設定読み込み
    let config = McpConfig::load()?;

    if let Some(wp_config) = config.handlers.wordpress {
        println!("📍 診断対象:");
        println!("   URL: {}", wp_config.url);
        println!("   Username: {} (管理者権限確認済み)", wp_config.username);

        let handler = WordPressHandler::new(wp_config);

        // 異なるAPIエンドポイントの個別テスト
        println!("\n🔍 API エンドポイント別アクセステスト:");

        // 1. wp/v2/categories (成功例)
        println!("\n1. カテゴリーAPI (/wp/v2/categories)");
        match handler.get_categories().await {
            Ok(categories) => {
                println!("   ✅ アクセス成功 ({}件)", categories.len());
                println!("   📄 レスポンスヘッダー情報取得可能");
            }
            Err(e) => {
                println!("   ❌ アクセス失敗: {}", e);
            }
        }

        // 2. wp/v2/posts
        println!("\n2. 投稿API (/wp/v2/posts)");
        match handler.get_all_content().await {
            Ok((posts, pages)) => {
                println!(
                    "   ✅ アクセス成功 (投稿{}件、ページ{}件)",
                    posts.len(),
                    pages.len()
                );
            }
            Err(e) => {
                println!("   ❌ アクセス失敗: {}", e);
            }
        }

        // 3. wp/v2/media
        println!("\n3. メディアAPI (/wp/v2/media)");
        match handler.get_media().await {
            Ok(media) => {
                println!("   ✅ アクセス成功 ({}件)", media.len());
            }
            Err(e) => {
                println!("   ❌ アクセス失敗: {}", e);
            }
        }

        // 4. wp/v2/tags
        println!("\n4. タグAPI (/wp/v2/tags)");
        match handler.get_tags().await {
            Ok(tags) => {
                println!("   ✅ アクセス成功 ({}件)", tags.len());
            }
            Err(e) => {
                println!("   ❌ アクセス失敗: {}", e);
            }
        }

        // 5. wp/v2/settings (問題のAPI)
        println!("\n5. 設定API (/wp/v2/settings) ⚠️ 問題のエンドポイント");
        match handler.get_settings().await {
            Ok(settings) => {
                println!("   ✅ アクセス成功");
                if let Some(title) = &settings.title {
                    println!("      サイトタイトル: {}", title);
                }
            }
            Err(e) => {
                println!("   ❌ アクセス失敗: {}", e);
                println!("   🔍 詳細分析:");

                let error_str = format!("{}", e);
                if error_str.contains("401") {
                    println!(
                        "      → 認証エラー: アプリケーションパスワードまたはユーザー権限の問題"
                    );
                } else if error_str.contains("403") {
                    println!("      → 権限エラー: 特定の権限が不足");
                } else if error_str.contains("404") {
                    println!("      → エンドポイント不存在: REST APIまたはプラグインの問題");
                } else {
                    println!("      → その他のエラー: 詳細調査が必要");
                }
            }
        }

        println!("\n📊 診断結果分析:");
        println!("   🎯 問題の特定:");
        println!("      • 基本的なコンテンツAPI (posts, categories, tags, media) = ✅ 正常");
        println!("      • 設定管理API (settings) = ❌ 401エラー");
        println!();
        println!("   🔍 考えられる原因:");
        println!("      1. WordPress REST API設定で /wp/v2/settings が特別に制限されている");
        println!("      2. セキュリティプラグインが設定APIへのアクセスを制限している");
        println!("      3. WordPressのバージョンによる設定API仕様の違い");
        println!("      4. アプリケーションパスワードの権限範囲制限");
        println!();
        println!("   💡 推奨調査項目:");
        println!("      • WordPress管理画面で「設定」→「パーマリンク」→「変更を保存」を実行");
        println!("      • セキュリティプラグイン（Wordfence等）の設定確認");
        println!("      • WordPress バージョンと REST API 有効性の確認");
        println!("      • 他のアプリケーションパスワードでのテスト");
    } else {
        println!("❌ WordPress設定が見つかりません");
    }

    Ok(())
}
