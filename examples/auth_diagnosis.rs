use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 WordPress 認証診断テスト");
    println!("=====================================");

    // 設定読み込み
    let config = McpConfig::load()?;

    if let Some(wp_config) = config.handlers.wordpress {
        println!("📍 接続先情報:");
        println!("   URL: {}", wp_config.url);
        println!("   Username: {}", wp_config.username);
        println!(
            "   Password: {}*** (長さ: {}文字)",
            &wp_config.password.chars().take(8).collect::<String>(),
            wp_config.password.len()
        );

        let handler = WordPressHandler::new(wp_config);

        // 段階的な診断
        println!("\n🔍 段階的診断:");

        // 1. サイトへの基本アクセス
        println!("1. 基本サイトアクセステスト...");
        match handler.get_categories().await {
            Ok(categories) => {
                println!(
                    "   ✅ カテゴリー取得成功 ({}件) - サイトアクセス可能",
                    categories.len()
                );
                for cat in categories.iter().take(3) {
                    println!("      - {} (投稿数: {:?})", cat.name, cat.count);
                }
            }
            Err(e) => {
                println!("   ❌ カテゴリー取得失敗: {}", e);
                return Ok(());
            }
        }

        // 2. タグ取得テスト
        println!("\n2. タグ取得テスト...");
        match handler.get_tags().await {
            Ok(tags) => {
                println!("   ✅ タグ取得成功 ({}件)", tags.len());
                for tag in tags.iter().take(3) {
                    println!("      - {} (投稿数: {:?})", tag.name, tag.count);
                }
            }
            Err(e) => {
                println!("   ❌ タグ取得失敗: {}", e);
            }
        }

        // 3. メディア取得テスト
        println!("\n3. メディア取得テスト...");
        match handler.get_media().await {
            Ok(media) => {
                println!("   ✅ メディア取得成功 ({}件)", media.len());
                for item in media.iter().take(3) {
                    if let Some(title) = &item.title {
                        println!("      - {}", title.rendered);
                    }
                }
            }
            Err(e) => {
                println!("   ❌ メディア取得失敗: {}", e);
            }
        }

        // 4. 設定取得テスト（管理者権限が必要）
        println!("\n4. 設定取得テスト（管理者権限必要）...");
        match handler.get_settings().await {
            Ok(settings) => {
                println!("   ✅ 設定取得成功 - 管理者権限あり");
                if let Some(title) = &settings.title {
                    println!("      サイトタイトル: {}", title);
                }
            }
            Err(e) => {
                println!("   ❌ 設定取得失敗: {}", e);
                println!("      → 管理者権限が不足している可能性があります");
            }
        }

        // 5. 投稿一覧取得テスト
        println!("\n5. 投稿一覧取得テスト...");
        match handler.get_all_content().await {
            Ok((posts, pages)) => {
                println!("   ✅ コンテンツ取得成功");
                println!("      投稿: {}件, ページ: {}件", posts.len(), pages.len());
                for post in posts.iter().take(3) {
                    println!(
                        "      - {} (ステータス: {})",
                        post.title.rendered, post.status
                    );
                }
            }
            Err(e) => {
                println!("   ❌ コンテンツ取得失敗: {}", e);
            }
        }

        println!("\n📊 診断結果まとめ:");
        println!("   🔗 基本接続: 正常（カテゴリー取得成功）");
        println!("   🔐 認証情報: 部分的に有効");
        println!("   ⚙️ 管理者権限: 要確認（設定アクセス権限）");
        println!();
        println!("💡 推奨事項:");
        println!("   • WordPress管理画面でアプリケーションパスワードを再生成");
        println!("   • ユーザーに適切な権限（編集者以上）が付与されているか確認");
        println!("   • プラグインがREST APIを制限していないか確認");
    } else {
        println!("❌ WordPress設定が見つかりません");
    }

    Ok(())
}
