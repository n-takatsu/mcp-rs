//! WordPress Site Builder Demo
//!
//! mcp-rs を使って WordPress サイトの初期構成を一括構築するデモ。
//! カテゴリ・タグ・固定ページ・ブログ記事をコードから自動生成する。
//!
//! ## 実行方法
//! ```bash
//! export WORDPRESS_URL="https://your-site.example.com"
//! export WORDPRESS_USERNAME="your_username"
//! export WORDPRESS_PASSWORD="your_app_password"
//!
//! cargo run --example wordpress_site_builder_demo
//! ```
//!
//! `WORDPRESS_URL` には `/wp-json` を含めず、サイトのベース URL を指定してください。
//!
//! アプリケーションパスワードの取得:
//!   WordPress 管理画面 → ユーザー → プロフィール → アプリケーションパスワード
//!
//! ## 構築ステップ
//! 1. WordPress 接続確認（ヘルスチェック）
//! 2. サイト基本設定（タイトル・説明・タイムゾーン）
//! 3. カテゴリ作成
//! 4. タグ作成
//! 5. 固定ページ作成
//! 6. ブログ記事作成

use mcp_rs::{
    config::WordPressConfig,
    handlers::wordpress::{PostCreateParams, SettingsUpdateParams, WordPressHandler},
};
use std::collections::HashSet;
use std::env;
use std::io::{self, Write};
use tokio::time::{sleep, Duration};

// ─────────────────────────────────────────────────────────────────────────────
// サイト固有の設定 — ここを編集してください
// ─────────────────────────────────────────────────────────────────────────────

const SITE_TITLE: &str = "My Project Site";
const SITE_DESCRIPTION: &str = "A project site built with mcp-rs";
const SITE_TIMEZONE: &str = "Asia/Tokyo";
const SITE_LANGUAGE: &str = "ja";

/// 作成するカテゴリ一覧: (名前, 説明)
const CATEGORIES: &[(&str, &str)] = &[
    ("お知らせ", "リリース・更新情報"),
    ("技術ブログ", "技術的な解説・知見"),
    ("使い方", "機能説明・チュートリアル"),
    ("開発ログ", "開発進捗・振り返り"),
];

/// 作成するタグ一覧: (名前, 説明)
const TAGS: &[(&str, &str)] = &[
    ("Rust", "Rust プログラミング言語"),
    ("オープンソース", "オープンソース開発"),
    ("チュートリアル", "入門・手順解説"),
    ("リリース", "バージョンアップ情報"),
];

// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_target(false)
        .init();

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║     WordPress Site Builder — Powered by mcp-rs       ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!();

    // 環境変数から認証情報を取得
    let wp_url_raw =
        env::var("WORDPRESS_URL").unwrap_or_else(|_| "https://your-site.example.com".to_string());
    // /wp-json の二重付与を防ぐため、末尾スラッシュと /wp-json を正規化する
    let wp_url = wp_url_raw
        .trim_end_matches('/')
        .strip_suffix("/wp-json")
        .unwrap_or(wp_url_raw.trim_end_matches('/'))
        .to_string();
    let wp_username = env::var("WORDPRESS_USERNAME").unwrap_or_default();
    let wp_password = env::var("WORDPRESS_PASSWORD").unwrap_or_default();

    if wp_username.is_empty() || wp_password.is_empty() {
        eprintln!("❌ 環境変数を設定してください:");
        eprintln!("   export WORDPRESS_URL='https://your-site.example.com'");
        eprintln!("   export WORDPRESS_USERNAME='your_username'");
        eprintln!("   export WORDPRESS_PASSWORD='your_app_password'");
        std::process::exit(1);
    }

    println!("📡 接続先: {}", wp_url);
    println!("👤 ユーザー: {}", wp_username);
    println!();

    let mut warning_count = 0usize;

    let handler = WordPressHandler::try_new(WordPressConfig {
        url: wp_url,
        username: wp_username,
        password: wp_password,
        enabled: Some(true),
        timeout_seconds: Some(30),
        rate_limit: None,
        encrypted_credentials: None,
    })
    .map_err(|e| io::Error::other(format!("WordPress ハンドラーの初期化に失敗しました: {}", e)))?;

    // ── Step 1: ヘルスチェック ──────────────────────────────────────────────
    print!("[1/6] 🔍 WordPress 接続確認... ");
    io::stdout().flush()?;
    let health = handler.health_check().await;
    if health.error_details.is_empty() {
        let site_name = health
            .site_info
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("—");
        println!("✅ 接続成功 (サイト名: {})", site_name);
    } else {
        println!("⚠️  警告あり");
        for err in &health.error_details {
            eprintln!("     {}", err);
        }
        eprintln!("❌ ヘルスチェックで問題が検出されたため、Step 2 以降を中止します。");
        return Ok(());
    }

    // ── Step 2: サイト設定 ──────────────────────────────────────────────────
    print!("[2/6] ⚙️  サイト基本設定を更新中... ");
    io::stdout().flush()?;
    handler
        .update_settings(SettingsUpdateParams {
            title: Some(SITE_TITLE.to_string()),
            description: Some(SITE_DESCRIPTION.to_string()),
            timezone: Some(SITE_TIMEZONE.to_string()),
            show_on_front: None,
            page_on_front: None,
            page_for_posts: None,
            posts_per_page: None,
            default_category: None,
            language: Some(SITE_LANGUAGE.to_string()),
        })
        .await?;
    println!("✅");
    sleep(Duration::from_millis(300)).await;

    // ── Step 3: カテゴリ作成 ────────────────────────────────────────────────
    println!("[3/6] 🏷️  カテゴリを作成中...");
    let mut category_ids: Vec<(String, u64)> = Vec::new();
    let existing_categories = handler.get_categories().await?;
    for (name, desc) in CATEGORIES {
        if let Some(existing) = existing_categories.iter().find(|c| c.name == *name) {
            if let Some(id) = existing.id {
                println!("   ♻️  「{}」既存を利用 (ID: {})", name, id);
                category_ids.push((name.to_string(), id));
                sleep(Duration::from_millis(200)).await;
                continue;
            }
            return Err(std::io::Error::other(format!(
                "カテゴリ「{}」の既存IDが取得できませんでした",
                name
            ))
            .into());
        }

        match handler.create_category(name, Some(desc), None).await {
            Ok(cat) => {
                if let Some(id) = cat.id {
                    println!("   ✅ 「{}」(ID: {})", name, id);
                    category_ids.push((name.to_string(), id));
                } else {
                    return Err(std::io::Error::other(format!(
                        "カテゴリ「{}」の作成後にIDが取得できませんでした",
                        name
                    ))
                    .into());
                }
            }
            Err(e) => {
                return Err(std::io::Error::other(format!(
                    "カテゴリ「{}」の作成に失敗しました: {}",
                    name, e
                ))
                .into());
            }
        }
        sleep(Duration::from_millis(200)).await;
    }

    // ── Step 4: タグ作成 ────────────────────────────────────────────────────
    println!("[4/6] 🔖 タグを作成中...");
    let mut tag_ids: Vec<(String, u64)> = Vec::new();
    let existing_tags = handler.get_tags().await?;
    for (name, desc) in TAGS {
        if let Some(existing) = existing_tags.iter().find(|t| t.name == *name) {
            if let Some(id) = existing.id {
                println!("   ♻️  「{}」既存を利用 (ID: {})", name, id);
                tag_ids.push((name.to_string(), id));
                sleep(Duration::from_millis(200)).await;
                continue;
            }
            return Err(std::io::Error::other(format!(
                "タグ「{}」の既存IDが取得できませんでした",
                name
            ))
            .into());
        }

        match handler.create_tag(name, Some(desc)).await {
            Ok(tag) => {
                if let Some(id) = tag.id {
                    println!("   ✅ 「{}」(ID: {})", name, id);
                    tag_ids.push((name.to_string(), id));
                } else {
                    return Err(std::io::Error::other(format!(
                        "タグ「{}」の作成後にIDが取得できませんでした",
                        name
                    ))
                    .into());
                }
            }
            Err(e) => {
                return Err(std::io::Error::other(format!(
                    "タグ「{}」の作成に失敗しました: {}",
                    name, e
                ))
                .into());
            }
        }
        sleep(Duration::from_millis(200)).await;
    }

    // ── Step 5: 固定ページ作成 ──────────────────────────────────────────────
    println!("[5/6] 📄 固定ページを作成中...");
    let (existing_posts, existing_pages) = handler.get_all_content().await?;
    let mut existing_page_titles: HashSet<String> = existing_pages
        .iter()
        .map(|p| normalize_title(&p.title.rendered))
        .collect();
    let mut existing_post_titles: HashSet<String> = existing_posts
        .iter()
        .map(|p| normalize_title(&p.title.rendered))
        .collect();

    for (title, content) in sample_pages() {
        if existing_page_titles.contains(&normalize_title(&title)) {
            println!("   ♻️  「{}」既存を利用", title);
            sleep(Duration::from_millis(200)).await;
            continue;
        }

        match handler
            .create_advanced_post(PostCreateParams {
                title: title.clone(),
                content,
                post_type: "page".to_string(),
                status: "publish".to_string(),
                date: None,
                categories: None,
                tags: None,
                featured_media_id: None,
                meta: None,
            })
            .await
        {
            Ok(page) => {
                if let Some(page_id) = page.id {
                    println!("   ✅ 「{}」(ID: {})", title, page_id);
                    existing_page_titles.insert(normalize_title(&title));
                } else {
                    warning_count += 1;
                    println!("   ⚠️  「{}」作成成功だがIDが取得できませんでした", title);
                }
            }
            Err(e) => {
                warning_count += 1;
                println!("   ⚠️  「{}」作成失敗: {}", title, e);
            }
        }
        sleep(Duration::from_millis(400)).await;
    }

    // ── Step 6: ブログ記事作成 ──────────────────────────────────────────────
    println!("[6/6] ✍️  ブログ記事を作成中...");

    let find_cat = |name: &str| -> Option<u64> {
        category_ids
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, id)| *id)
    };
    let find_tag =
        |name: &str| -> Option<u64> { tag_ids.iter().find(|(n, _)| n == name).map(|(_, id)| *id) };

    for (title, content, cats, tags) in sample_posts(&find_cat, &find_tag) {
        if existing_post_titles.contains(&normalize_title(&title)) {
            println!("   ♻️  「{}」既存を利用", title);
            sleep(Duration::from_millis(200)).await;
            continue;
        }

        if cats.is_empty() || tags.is_empty() {
            return Err(std::io::Error::other(format!(
                "投稿「{}」に必要なカテゴリまたはタグのIDが解決できませんでした",
                title
            ))
            .into());
        }

        let categories = if cats.is_empty() { None } else { Some(cats) };
        let tags = if tags.is_empty() { None } else { Some(tags) };
        match handler
            .create_advanced_post(PostCreateParams {
                title: title.clone(),
                content,
                post_type: "post".to_string(),
                status: "publish".to_string(),
                date: None,
                categories,
                tags,
                featured_media_id: None,
                meta: None,
            })
            .await
        {
            Ok(post) => {
                if let Some(post_id) = post.id {
                    println!("   ✅ 「{}」(ID: {})", title, post_id);
                    existing_post_titles.insert(normalize_title(&title));
                } else {
                    warning_count += 1;
                    println!("   ⚠️  「{}」作成成功だがIDが取得できませんでした", title);
                }
            }
            Err(e) => {
                warning_count += 1;
                println!("   ⚠️  「{}」作成失敗: {}", title, e);
            }
        }
        sleep(Duration::from_millis(400)).await;
    }

    println!();
    if warning_count == 0 {
        println!("╔══════════════════════════════════════════════════════╗");
        println!("║  🎉 サイト初期構築完了！                              ║");
        println!("╚══════════════════════════════════════════════════════╝");
    } else {
        println!("╔══════════════════════════════════════════════════════╗");
        println!("║  ⚠️  一部の処理で失敗が発生しました                    ║");
        println!("╚══════════════════════════════════════════════════════╝");
        return Err(std::io::Error::other(format!(
            "{} 件の処理失敗が発生しました。ログを確認してください。",
            warning_count
        ))
        .into());
    }

    Ok(())
}

fn normalize_title(title: &str) -> String {
    title.trim().to_lowercase()
}

/// サンプルの固定ページ: (タイトル, 本文HTML)
///
/// 実際のプロジェクトに合わせて内容を差し替えてください。
fn sample_pages() -> Vec<(String, String)> {
    vec![
        (
            "About".to_string(),
            r#"<!-- wp:paragraph -->
<p>このサイトについての説明をここに記載します。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
        ),
        (
            "Contact".to_string(),
            r#"<!-- wp:paragraph -->
<p>お問い合わせ方法についての説明をここに記載します。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
        ),
    ]
}

/// サンプルのブログ記事: (タイトル, 本文HTML, カテゴリID一覧, タグID一覧)
///
/// 実際のプロジェクトに合わせて内容を差し替えてください。
fn sample_posts<F, G>(find_cat: &F, find_tag: &G) -> Vec<(String, String, Vec<u64>, Vec<u64>)>
where
    F: Fn(&str) -> Option<u64>,
    G: Fn(&str) -> Option<u64>,
{
    vec![
        (
            "サイトをオープンしました".to_string(),
            r#"<!-- wp:paragraph -->
<p>ようこそ。このサイトは <a href="https://github.com/rireki-ai/mcp-rs">mcp-rs</a> を使って自動構築されました。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
            vec![find_cat("お知らせ")]
                .into_iter()
                .flatten()
                .collect(),
            vec![find_tag("リリース"), find_tag("オープンソース")]
                .into_iter()
                .flatten()
                .collect(),
        ),
        (
            "mcp-rs で WordPress を自動構築する方法".to_string(),
            r#"<!-- wp:paragraph -->
<p>mcp-rs の WordPress ツール群を使うと、カテゴリ・タグ・ページ・記事をコードから一括生成できます。</p>
<!-- /wp:paragraph -->

<!-- wp:code -->
<pre class="wp-block-code"><code class="language-bash">export WORDPRESS_URL="https://your-site.example.com"
export WORDPRESS_USERNAME="your_username"
export WORDPRESS_PASSWORD="your_app_password"
cargo run --example wordpress_site_builder_demo</code></pre>
<!-- /wp:code -->"#
                .to_string(),
            vec![find_cat("技術ブログ")]
                .into_iter()
                .flatten()
                .collect(),
            vec![find_tag("Rust"), find_tag("チュートリアル")]
                .into_iter()
                .flatten()
                .collect(),
        ),
    ]
}
