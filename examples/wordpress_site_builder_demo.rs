//! WordPress Site Builder Demo
//!
//! mcp-rs を使って WordPress サイトの初期構成を一括構築するデモ。
//! カテゴリ・タグ・固定ページ・メニュー・ブログ記事をコードから自動生成する。
//! 実行は WordPress REST API とアプリケーションパスワードを利用する。
//! wp-admin のログイン操作や reCAPTCHA 自動化には依存しない。
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
//! 6. メニュー作成
//! 7. ブログ記事作成

use crossterm::event::{read, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use mcp_rs::{
    config::WordPressConfig,
    handlers::wordpress::{
        PostCreateParams, SettingsUpdateParams, WordPressCategory, WordPressHandler, WordPressTag,
    },
};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use tokio::time::{sleep, Duration};

// -----------------------------------------------------------------------------
// サイト固有の設定
// -----------------------------------------------------------------------------

const SITE_TITLE: &str = "redring.jp";
const SITE_DESCRIPTION: &str = "3DCAD・幾何処理・設計自動化を軸に発信する RedRing の公式サイト";
const SITE_TIMEZONE: &str = "Asia/Tokyo";
const SITE_LANGUAGE: &str = "ja";
const FRONT_PAGE_TITLE: &str = "ホーム";
const SITE_GUIDE_TITLE: &str = "サイトガイド";
const POSTS_PAGE_TITLE: &str = "投稿一覧";
const ABOUT_US_TITLE: &str = "About Us";
const CONTRIBUTORS_TITLE: &str = "Contributors募集";
const PRIMARY_MENU_NAME: &str = "Primary Navigation";
const FOOTER_MENU_NAME: &str = "Footer Navigation";
const WORDPRESS_LOCAL_ENV_FILE: &str = ".env.wordpress.local";

const CATEGORIES: &[(&str, &str)] = &[
    ("ウェブサイト", "サイト運用・導線設計・UI改善"),
    ("お知らせ", "リリース・更新情報"),
    ("RedRing思想", "思想・設計哲学"),
    ("構造設計", "構造美と設計原則"),
    ("技術信頼性", "品質・安全性・運用性"),
    ("実装記録", "実装手順・検証ログ"),
];

const TAGS: &[(&str, &str)] = &[
    ("サイト運用", "サイト全体の運用・保守・改善"),
    ("UI改善", "見た目と操作性の改善"),
    ("導線整理", "情報の流れと回遊性の改善"),
    ("RedRing", "RedRing プロジェクト関連"),
    ("mcp-rs", "mcp-rs 実装関連"),
    ("MCP", "Model Context Protocol"),
    ("Rust", "Rust プログラミング言語"),
    ("WordPress", "WordPress 連携"),
    ("セキュリティ", "セキュリティ関連"),
    ("監査ログ", "変更履歴と操作証跡の記録"),
    ("監視", "異常検知と状態監視"),
    ("生成AI", "AI 活用・実装"),
    ("運用自動化", "運用効率化と自動化"),
    ("構造美", "情報設計と視覚的構造"),
    ("GitHub", "開発情報への導線"),
];

#[derive(Debug, Clone)]
struct MenuItemSpec {
    title: String,
    page_title: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_target(false)
        .init();

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║     WordPress Site Builder — Powered by mcp-rs       ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!("REST Mode: WordPress REST API + Application Password");
    println!();

    if let Some(path) = load_local_wordpress_env()? {
        println!("🔐 ローカル環境ファイルを読み込みました: {}", path);
    }

    let wp_url_raw = resolve_wp_url()?;
    let wp_url = wp_url_raw
        .trim_end_matches('/')
        .strip_suffix("/wp-json")
        .unwrap_or(wp_url_raw.trim_end_matches('/'))
        .to_string();
    let wp_username = resolve_wp_username()?;
    let wp_password = resolve_wp_password()?;

    println!("📡 接続先: {}", wp_url);
    println!("👤 ユーザー: {}", wp_username);
    println!();

    let mut warning_count = 0usize;

    let handler = WordPressHandler::try_new(WordPressConfig {
        url: wp_url.clone(),
        username: wp_username.clone(),
        password: wp_password.clone(),
        enabled: Some(true),
        timeout_seconds: Some(30),
        rate_limit: None,
        encrypted_credentials: None,
    })
    .map_err(|e| io::Error::other(format!("WordPress ハンドラーの初期化に失敗しました: {}", e)))?;

    print!("[1/7] 🔍 WordPress 接続確認... ");
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

        if direct_rest_api_probe(&wp_url).await {
            println!("   ↪️  直接REST疎通が成功したため、処理を継続します");
        } else {
            eprintln!("❌ ヘルスチェックで問題が検出されたため、Step 2 以降を中止します。");
            return Ok(());
        }
    }

    print!("[2/7] ⚙️  サイト基本設定を更新中... ");
    io::stdout().flush()?;
    handler
        .update_settings(SettingsUpdateParams {
            title: Some(SITE_TITLE.to_string()),
            description: Some(SITE_DESCRIPTION.to_string()),
            timezone: Some(SITE_TIMEZONE.to_string()),
            show_on_front: None,
            page_on_front: None,
            page_for_posts: None,
            posts_per_page: Some(12),
            default_category: None,
            language: Some(SITE_LANGUAGE.to_string()),
        })
        .await?;
    println!("✅");
    sleep(Duration::from_millis(300)).await;

    println!("[3/7] 🏷️  カテゴリを作成中...");
    let mut category_ids: Vec<(String, u64)> = Vec::new();
    let existing_categories = fetch_all_categories(&wp_url, &wp_username, &wp_password).await?;
    for (name, desc) in CATEGORIES {
        if let Some(existing) = existing_categories.iter().find(|c| c.name == *name) {
            if let Some(id) = existing.id {
                println!("   ♻️  「{}」既存を利用 (ID: {})", name, id);
                category_ids.push((name.to_string(), id));
                sleep(Duration::from_millis(200)).await;
                continue;
            }
            return Err(io::Error::other(format!(
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
                    return Err(io::Error::other(format!(
                        "カテゴリ「{}」の作成後にIDが取得できませんでした",
                        name
                    ))
                    .into());
                }
            }
            Err(e) => {
                return Err(io::Error::other(format!(
                    "カテゴリ「{}」の作成に失敗しました: {}",
                    name, e
                ))
                .into());
            }
        }
        sleep(Duration::from_millis(200)).await;
    }

    println!("[4/7] 🔖 タグを作成中...");
    let mut tag_ids: Vec<(String, u64)> = Vec::new();
    let existing_tags = fetch_all_tags(&wp_url, &wp_username, &wp_password).await?;
    for (name, desc) in TAGS {
        if let Some(existing) = existing_tags.iter().find(|t| t.name == *name) {
            if let Some(id) = existing.id {
                println!("   ♻️  「{}」既存を利用 (ID: {})", name, id);
                tag_ids.push((name.to_string(), id));
                sleep(Duration::from_millis(200)).await;
                continue;
            }
            return Err(io::Error::other(format!(
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
                    return Err(io::Error::other(format!(
                        "タグ「{}」の作成後にIDが取得できませんでした",
                        name
                    ))
                    .into());
                }
            }
            Err(e) => {
                return Err(io::Error::other(format!(
                    "タグ「{}」の作成に失敗しました: {}",
                    name, e
                ))
                .into());
            }
        }
        sleep(Duration::from_millis(200)).await;
    }

    println!("[5/7] 📄 固定ページを作成中...");
    let (existing_posts, existing_pages) = handler.get_all_content().await?;
    let mut page_id_by_title: HashMap<String, u64> = HashMap::new();
    for page in &existing_pages {
        if let Some(page_id) = page.id {
            page_id_by_title.insert(normalize_title(&page.title.rendered), page_id);
        }
    }
    let mut existing_post_ids_by_title: HashMap<String, u64> = HashMap::new();
    for post in &existing_posts {
        if let Some(post_id) = post.id {
            existing_post_ids_by_title.insert(normalize_title(&post.title.rendered), post_id);
        }
    }

    for (title, content) in sample_pages() {
        let normalized_title = normalize_title(&title);
        let existing_page_id = page_id_by_title
            .get(&normalized_title)
            .copied()
            .or_else(|| {
                legacy_page_title_aliases(&title)
                    .iter()
                    .find_map(|legacy| page_id_by_title.get(&normalize_title(legacy)).copied())
            });

        if let Some(existing_id) = existing_page_id {
            match update_page_content(
                &wp_url,
                &wp_username,
                &wp_password,
                existing_id,
                &title,
                &content,
            )
            .await
            {
                Ok(()) => println!("   🔄 「{}」既存ページを更新 (ID: {})", title, existing_id),
                Err(e) => {
                    warning_count += 1;
                    println!("   ⚠️  「{}」更新失敗: {}", title, e);
                }
            }
            sleep(Duration::from_millis(200)).await;
            continue;
        }

        match handler
            .create_advanced_post(PostCreateParams {
                title: title.clone(),
                content: content.clone(),
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
                    if let Err(e) = update_page_content(
                        &wp_url,
                        &wp_username,
                        &wp_password,
                        page_id,
                        &title,
                        &content,
                    )
                    .await
                    {
                        warning_count += 1;
                        println!("   ⚠️  「{}」作成後のスラッグ更新失敗: {}", title, e);
                    }

                    println!("   ✅ 「{}」(ID: {})", title, page_id);
                    page_id_by_title.insert(normalized_title, page_id);
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

    let front_page_id = page_id_by_title
        .get(&normalize_title(FRONT_PAGE_TITLE))
        .copied();
    let posts_page_id = page_id_by_title
        .get(&normalize_title(POSTS_PAGE_TITLE))
        .copied();

    if let (Some(front), Some(posts)) = (front_page_id, posts_page_id) {
        print!("      └─ 🧭 フロントページ設定を適用中... ");
        io::stdout().flush()?;
        handler
            .update_settings(SettingsUpdateParams {
                title: None,
                description: None,
                timezone: None,
                show_on_front: Some("page".to_string()),
                page_on_front: Some(front),
                page_for_posts: Some(posts),
                posts_per_page: None,
                default_category: None,
                language: None,
            })
            .await?;
        println!("✅ (ホームID: {}, 投稿ページID: {})", front, posts);
    } else {
        warning_count += 1;
        println!(
            "      └─ ⚠️  フロントページ設定をスキップ ({} または {} が見つかりません)",
            FRONT_PAGE_TITLE, POSTS_PAGE_TITLE
        );
    }

    println!("[6/7] 🧭 メニューを作成中...");
    let menu_items = vec![
        MenuItemSpec {
            title: "🏠 ホーム".to_string(),
            page_title: FRONT_PAGE_TITLE.to_string(),
        },
        MenuItemSpec {
            title: "🧭 サイトガイド".to_string(),
            page_title: SITE_GUIDE_TITLE.to_string(),
        },
        MenuItemSpec {
            title: "🛠️ サービス".to_string(),
            page_title: "サービス".to_string(),
        },
        MenuItemSpec {
            title: "🧾 投稿一覧".to_string(),
            page_title: POSTS_PAGE_TITLE.to_string(),
        },
        MenuItemSpec {
            title: "ℹ️ About Us".to_string(),
            page_title: ABOUT_US_TITLE.to_string(),
        },
        MenuItemSpec {
            title: "✉️ お問い合わせ".to_string(),
            page_title: "お問い合わせ".to_string(),
        },
    ];

    match ensure_primary_menu(
        &wp_url,
        &wp_username,
        &wp_password,
        &page_id_by_title,
        PRIMARY_MENU_NAME,
        &["primary-menu", "primary-menu-side"],
        &menu_items,
    )
    .await
    {
        Ok(true) => println!("   ✅ メニュー作成・項目登録が完了しました"),
        Ok(false) => {
            warning_count += 1;
            println!(
                "   ⚠️  メニューAPIが利用できないためスキップしました（AFFINGER6の管理画面で手動設定してください）"
            );
        }
        Err(e) => {
            warning_count += 1;
            println!("   ⚠️  メニュー作成に失敗しました: {}", e);
        }
    }

    let footer_items = vec![
        MenuItemSpec {
            title: "About Us".to_string(),
            page_title: ABOUT_US_TITLE.to_string(),
        },
        MenuItemSpec {
            title: "サービス".to_string(),
            page_title: "サービス".to_string(),
        },
        MenuItemSpec {
            title: "Contributors募集".to_string(),
            page_title: CONTRIBUTORS_TITLE.to_string(),
        },
        MenuItemSpec {
            title: "お問い合わせ".to_string(),
            page_title: "お問い合わせ".to_string(),
        },
        MenuItemSpec {
            title: "投稿一覧".to_string(),
            page_title: POSTS_PAGE_TITLE.to_string(),
        },
    ];

    match ensure_primary_menu(
        &wp_url,
        &wp_username,
        &wp_password,
        &page_id_by_title,
        FOOTER_MENU_NAME,
        &["secondary-menu", "smartphone-footermenu"],
        &footer_items,
    )
    .await
    {
        Ok(true) => println!("   ✅ フッターメニュー作成・項目登録が完了しました"),
        Ok(false) => {
            warning_count += 1;
            println!("   ⚠️  フッターメニューAPIが利用できないためスキップしました");
        }
        Err(e) => {
            warning_count += 1;
            println!("   ⚠️  フッターメニュー作成に失敗しました: {}", e);
        }
    }

    println!("[7/7] ✍️  ブログ記事を作成中...");

    let find_cat = |name: &str| -> Option<u64> {
        category_ids
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, id)| *id)
    };
    let find_tag =
        |name: &str| -> Option<u64> { tag_ids.iter().find(|(n, _)| n == name).map(|(_, id)| *id) };

    for (title, content, cats, tags) in sample_posts(&find_cat, &find_tag) {
        let normalized_title = normalize_title(&title);
        let existing_post_id = existing_post_ids_by_title
            .get(&normalized_title)
            .copied()
            .or_else(|| {
                legacy_post_title_aliases(&title).iter().find_map(|legacy| {
                    existing_post_ids_by_title
                        .get(&normalize_title(legacy))
                        .copied()
                })
            });

        if let Some(existing_id) = existing_post_id {
            update_post_content(
                &wp_url,
                &wp_username,
                &wp_password,
                existing_id,
                &title,
                &content,
                &cats,
                &tags,
            )
            .await?;
            println!("   🔄 「{}」既存投稿を更新 (ID: {})", title, existing_id);
            sleep(Duration::from_millis(200)).await;
            continue;
        }

        if cats.is_empty() || tags.is_empty() {
            return Err(io::Error::other(format!(
                "投稿「{}」に必要なカテゴリまたはタグのIDが解決できませんでした",
                title
            ))
            .into());
        }

        let categories_payload = if cats.is_empty() {
            None
        } else {
            Some(cats.clone())
        };
        let tags_payload = if tags.is_empty() {
            None
        } else {
            Some(tags.clone())
        };
        match handler
            .create_advanced_post(PostCreateParams {
                title: title.clone(),
                content: content.clone(),
                post_type: "post".to_string(),
                status: "publish".to_string(),
                date: None,
                categories: categories_payload,
                tags: tags_payload,
                featured_media_id: None,
                meta: None,
            })
            .await
        {
            Ok(post) => {
                if let Some(post_id) = post.id {
                    if let Err(e) = update_post_content(
                        &wp_url,
                        &wp_username,
                        &wp_password,
                        post_id,
                        &title,
                        &content,
                        &cats,
                        &tags,
                    )
                    .await
                    {
                        warning_count += 1;
                        println!("   ⚠️  「{}」作成後のスラッグ更新失敗: {}", title, e);
                    }

                    println!("   ✅ 「{}」(ID: {})", title, post_id);
                    existing_post_ids_by_title.insert(normalized_title, post_id);
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
        return Err(io::Error::other(format!(
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

fn legacy_post_title_aliases(title: &str) -> &'static [&'static str] {
    match title {
        "トップページ構成案を反映しました" => {
            &["トップページ構成案（post=73）を反映しました"]
        }
        _ => &[],
    }
}

fn legacy_page_title_aliases(title: &str) -> &'static [&'static str] {
    match title {
        POSTS_PAGE_TITLE => &["ニュース"],
        ABOUT_US_TITLE => &["会社概要"],
        CONTRIBUTORS_TITLE => &["採用情報"],
        _ => &[],
    }
}

fn post_slug_for_title(title: &str) -> Option<&'static str> {
    match title {
        "redring.jp を公開しました" => Some("redring-jp-launch"),
        "トップページ構成案を反映しました" => Some("homepage-structure-update"),
        "監査ログとセキュリティ監視を強化しました" => {
            Some("security-monitoring-enhanced")
        }
        _ => None,
    }
}

fn page_slug_for_title(title: &str) -> Option<&'static str> {
    match title {
        FRONT_PAGE_TITLE => Some("home"),
        SITE_GUIDE_TITLE => Some("site-guide"),
        "サービス" => Some("service"),
        ABOUT_US_TITLE => Some("about-us"),
        CONTRIBUTORS_TITLE => Some("contributors"),
        "お問い合わせ" => Some("contact"),
        POSTS_PAGE_TITLE => Some("blog"),
        _ => None,
    }
}

fn load_local_wordpress_env() -> io::Result<Option<String>> {
    let path = env::var("WORDPRESS_ENV_FILE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| WORDPRESS_LOCAL_ENV_FILE.to_string());

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(io::Error::other(format!(
                "ローカル環境ファイルの読み込みに失敗しました ({}): {}",
                path, err
            )));
        }
    };

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key_raw, value_raw)) = line.split_once('=') else {
            continue;
        };

        let key = key_raw.trim();
        let mut value = value_raw.trim().to_string();
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            value = value[1..value.len() - 1].to_string();
        }

        if key.starts_with("WORDPRESS_") && env::var(key).is_err() {
            env::set_var(key, value);
        }
    }

    Ok(Some(path))
}

fn resolve_wp_url() -> io::Result<String> {
    if let Ok(url) = env::var("WORDPRESS_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    prompt_line("WORDPRESS_URL が未設定です。サイトURLを入力してください（例: https://redring.jp）")
}

fn resolve_wp_username() -> io::Result<String> {
    if let Ok(username) = env::var("WORDPRESS_USERNAME") {
        let trimmed = username.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    prompt_line("WORDPRESS_USERNAME が未設定です。ユーザー名を入力してください")
}

fn resolve_wp_password() -> io::Result<String> {
    if let Ok(password) = env::var("WORDPRESS_PASSWORD") {
        let trimmed = password.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Ok(path) = env::var("WORDPRESS_PASSWORD_FILE") {
        let file_path = path.trim();
        if !file_path.is_empty() {
            let password = fs::read_to_string(file_path).map_err(|e| {
                io::Error::other(format!(
                    "WORDPRESS_PASSWORD_FILE の読み込みに失敗しました ({}): {}",
                    file_path, e
                ))
            })?;
            let trimmed = password.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }

    prompt_password_hidden(
        "WORDPRESS_PASSWORD が未設定です。アプリケーションパスワードを入力してください",
    )
}

fn prompt_line(prompt: &str) -> io::Result<String> {
    print!("{}: ", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let value = input.trim().to_string();
    if value.is_empty() {
        return Err(io::Error::other("入力値が空です。"));
    }
    Ok(value)
}

fn prompt_password_hidden(prompt: &str) -> io::Result<String> {
    print!("{}: ", prompt);
    io::stdout().flush()?;

    enable_raw_mode()?;
    let mut password = String::new();

    loop {
        if let Event::Key(key_event) = read()? {
            if key_event.kind != KeyEventKind::Press {
                continue;
            }

            match key_event.code {
                KeyCode::Enter => break,
                KeyCode::Backspace => {
                    if !password.is_empty() {
                        password.pop();
                    }
                }
                KeyCode::Char(c) => {
                    password.push(c);
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    println!();

    let trimmed = password.trim().to_string();
    if trimmed.is_empty() {
        return Err(io::Error::other("パスワードが空です。"));
    }
    Ok(trimmed)
}

async fn fetch_all_categories(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<Vec<WordPressCategory>, Box<dyn std::error::Error>> {
    let client = Client::new();
    let mut all_categories: Vec<WordPressCategory> = Vec::new();
    let mut page: u32 = 1;

    loop {
        let url = format!("{}/wp-json/wp/v2/categories", base_url);
        let response = client
            .get(&url)
            .basic_auth(username, Some(password))
            .query(&[("per_page", "100"), ("page", &page.to_string())])
            .send()
            .await?;

        if response.status() == StatusCode::BAD_REQUEST && page > 1 {
            break;
        }

        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "カテゴリ一覧の取得に失敗しました (status: {})",
                response.status()
            ))
            .into());
        }

        let page_items: Vec<WordPressCategory> = response.json().await?;
        if page_items.is_empty() {
            break;
        }

        all_categories.extend(page_items);
        page += 1;
    }

    Ok(all_categories)
}

async fn fetch_all_tags(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<Vec<WordPressTag>, Box<dyn std::error::Error>> {
    let client = Client::new();
    let mut all_tags: Vec<WordPressTag> = Vec::new();
    let mut page: u32 = 1;

    loop {
        let url = format!("{}/wp-json/wp/v2/tags", base_url);
        let response = client
            .get(&url)
            .basic_auth(username, Some(password))
            .query(&[("per_page", "100"), ("page", &page.to_string())])
            .send()
            .await?;

        if response.status() == StatusCode::BAD_REQUEST && page > 1 {
            break;
        }

        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "タグ一覧の取得に失敗しました (status: {})",
                response.status()
            ))
            .into());
        }

        let page_items: Vec<WordPressTag> = response.json().await?;
        if page_items.is_empty() {
            break;
        }

        all_tags.extend(page_items);
        page += 1;
    }

    Ok(all_tags)
}

async fn update_page_content(
    base_url: &str,
    username: &str,
    password: &str,
    page_id: u64,
    title: &str,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = format!("{}/wp-json/wp/v2/pages/{}", base_url, page_id);
    let mut body = serde_json::Map::new();
    body.insert("title".to_string(), json!(title));
    body.insert("content".to_string(), json!(content));
    body.insert("status".to_string(), json!("publish"));

    if let Some(slug) = page_slug_for_title(title) {
        body.insert("slug".to_string(), json!(slug));
    }

    let resp = client
        .post(url)
        .basic_auth(username, Some(password))
        .json(&Value::Object(body))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(io::Error::other(format!(
            "固定ページ更新に失敗しました (ID: {}, status: {})",
            page_id,
            resp.status()
        ))
        .into());
    }

    Ok(())
}

async fn update_post_content(
    base_url: &str,
    username: &str,
    password: &str,
    post_id: u64,
    title: &str,
    content: &str,
    categories: &[u64],
    tags: &[u64],
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = format!("{}/wp-json/wp/v2/posts/{}", base_url, post_id);
    let mut body = serde_json::Map::new();
    body.insert("title".to_string(), json!(title));
    body.insert("content".to_string(), json!(content));
    body.insert("status".to_string(), json!("publish"));
    body.insert("categories".to_string(), json!(categories));
    body.insert("tags".to_string(), json!(tags));

    if let Some(slug) = post_slug_for_title(title) {
        body.insert("slug".to_string(), json!(slug));
    }

    let resp = client
        .post(url)
        .basic_auth(username, Some(password))
        .json(&Value::Object(body))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(io::Error::other(format!(
            "投稿更新に失敗しました (ID: {}, status: {})",
            post_id,
            resp.status()
        ))
        .into());
    }

    Ok(())
}

async fn direct_rest_api_probe(base_url: &str) -> bool {
    let client = Client::new();
    let url = format!("{}/wp-json/wp/v2", base_url);
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn ensure_primary_menu(
    base_url: &str,
    username: &str,
    password: &str,
    page_id_by_title: &HashMap<String, u64>,
    menu_name: &str,
    location_candidates: &[&str],
    menu_items: &[MenuItemSpec],
) -> Result<bool, Box<dyn std::error::Error>> {
    let client = Client::new();
    let menus_url = format!("{}/wp-json/wp/v2/menus", base_url);

    let list_resp = client
        .get(&menus_url)
        .basic_auth(username, Some(password))
        .query(&[("per_page", "100")])
        .send()
        .await?;

    if list_resp.status() == StatusCode::NOT_FOUND {
        return Ok(false);
    }

    if !list_resp.status().is_success() {
        return Err(io::Error::other(format!(
            "メニュー一覧取得に失敗しました (status: {})",
            list_resp.status()
        ))
        .into());
    }

    let menus: Vec<Value> = list_resp.json().await?;
    let menu_id = if let Some(existing) = menus.iter().find(|m| {
        m.get("name")
            .and_then(|v| v.as_str())
            .map(|name| name == menu_name)
            .unwrap_or(false)
    }) {
        existing
            .get("id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| io::Error::other("既存メニューのIDが取得できません"))?
    } else {
        let create_resp = client
            .post(&menus_url)
            .basic_auth(username, Some(password))
            .json(&json!({"name": menu_name}))
            .send()
            .await?;

        if create_resp.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }

        if !create_resp.status().is_success() {
            return Err(io::Error::other(format!(
                "メニュー作成に失敗しました (status: {})",
                create_resp.status()
            ))
            .into());
        }

        let body: Value = create_resp.json().await?;
        body.get("id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| io::Error::other("作成したメニューのIDが取得できません"))?
    };

    let assign_resp = client
        .post(format!("{}/wp-json/wp/v2/menus/{}", base_url, menu_id))
        .basic_auth(username, Some(password))
        .json(&json!({ "locations": location_candidates }))
        .send()
        .await?;

    let assigned_any_location = assign_resp.status().is_success();

    if !assigned_any_location {
        let list_loc_resp = client
            .get(format!("{}/wp-json/wp/v2/menu-locations", base_url))
            .basic_auth(username, Some(password))
            .send()
            .await?;

        if !list_loc_resp.status().is_success() {
            return Ok(false);
        }
    }

    let menu_items_url = format!("{}/wp-json/wp/v2/menu-items", base_url);
    let existing_items_resp = client
        .get(&menu_items_url)
        .basic_auth(username, Some(password))
        .query(&[("menus", menu_id.to_string())])
        .send()
        .await?;

    if existing_items_resp.status() == StatusCode::NOT_FOUND {
        return Ok(false);
    }

    if !existing_items_resp.status().is_success() {
        return Err(io::Error::other(format!(
            "メニュー項目一覧取得に失敗しました (status: {})",
            existing_items_resp.status()
        ))
        .into());
    }

    let existing_items: Vec<Value> = existing_items_resp.json().await?;
    let existing_titles: HashSet<String> = existing_items
        .iter()
        .filter_map(|item| item.get("title"))
        .filter_map(|title| {
            if title.is_object() {
                title.get("rendered").and_then(|v| v.as_str())
            } else {
                title.as_str()
            }
        })
        .map(normalize_title)
        .collect();

    for (idx, spec) in menu_items.iter().enumerate() {
        if existing_titles.contains(&normalize_title(&spec.title)) {
            continue;
        }

        let page_key = normalize_title(&spec.page_title);
        let Some(page_id) = page_id_by_title.get(&page_key).copied() else {
            continue;
        };

        let create_item_resp = client
            .post(&menu_items_url)
            .basic_auth(username, Some(password))
            .json(&json!({
                "menus": menu_id,
                "title": spec.title,
                "object": "page",
                "object_id": page_id,
                "type": "post_type",
                "status": "publish",
                "menu_order": (idx as u64) + 1
            }))
            .send()
            .await?;

        if create_item_resp.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }

        if !create_item_resp.status().is_success() {
            return Err(io::Error::other(format!(
                "メニュー項目「{}」の作成に失敗しました (status: {})",
                spec.title,
                create_item_resp.status()
            ))
            .into());
        }
    }

    Ok(true)
}

fn sample_pages() -> Vec<(String, String)> {
    vec![
        (
            "ホーム".to_string(),
            r#"<!-- wp:html -->
<style>
body.page-id-86 #side {display: none !important;}
body.page-id-86 #content,
body.page-id-86 #contentInner,
body.page-id-86 #wrapper,
body.page-id-86 main {
    width: 100% !important;
    max-width: 1280px !important;
    margin-left: auto !important;
    margin-right: auto !important;
    float: none !important;
}
.rr-cad-wrap {background: radial-gradient(circle at 15% -10%, #1f2a36 0%, #121820 55%, #0b1016 100%); padding: 48px 20px; border-radius: 18px;}
.rr-grid {max-width: 1180px; margin: 0 auto; display: grid; gap: 18px;}
.rr-layout {position: relative; overflow: hidden; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 18px; align-items: start; max-width: 1040px; margin: 0 auto; padding: 18px; border: 1px solid rgba(151, 179, 209, 0.24); border-radius: 20px; background: linear-gradient(180deg, rgba(18, 24, 32, 0.9) 0%, rgba(23, 31, 41, 0.96) 100%), radial-gradient(circle at 12% 14%, rgba(86, 129, 180, 0.18) 0, rgba(86, 129, 180, 0.18) 12%, transparent 13%), radial-gradient(circle at 88% 10%, rgba(118, 91, 163, 0.14) 0, rgba(118, 91, 163, 0.14) 9%, transparent 10%), repeating-linear-gradient(135deg, rgba(255, 255, 255, 0.03) 0 1px, transparent 1px 22px); box-shadow: 0 18px 42px rgba(0, 0, 0, 0.26);}
.rr-layout::before {content: ""; position: absolute; inset: auto -8% 10% auto; width: 180px; height: 180px; border: 1px solid rgba(130, 169, 210, 0.24); border-radius: 50%; transform: rotate(18deg); pointer-events: none;}
.rr-layout::after {content: ""; position: absolute; inset: 12px 12px auto auto; width: 120px; height: 120px; background: linear-gradient(135deg, rgba(80, 123, 173, 0.26), rgba(80, 123, 173, 0.04)); clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%); opacity: 0.8; pointer-events: none;}
.rr-layout > * {position: relative; z-index: 1;}
.rr-span-2 {grid-column: 1 / -1;}
.rr-hero {position: relative; overflow: hidden; border-radius: 20px; background: linear-gradient(145deg, #182230 0%, #243449 50%, #2c4058 100%); color: #f3f7ff; padding: 44px 28px; border: 1px solid rgba(116, 146, 183, 0.35); box-shadow: 0 24px 54px rgba(0, 0, 0, 0.35);}
.rr-hero-main {position: relative; z-index: 3; max-width: 760px;}
.rr-kicker {font-size: 12px; letter-spacing: 0.22em; text-transform: uppercase; color: #9cb7d3;}
.rr-title {font-size: clamp(34px, 5.2vw, 58px); line-height: 1.08; margin: 12px 0 12px; font-weight: 800;}
.rr-sub {font-size: 16px; line-height: 1.8; color: #d4e1f2;}
.rr-actions {margin-top: 20px; display: flex; flex-wrap: wrap; gap: 10px;}
.rr-btn {display: inline-block; text-decoration: none; border-radius: 999px; padding: 11px 18px; font-weight: 700;}
.rr-btn-primary {background: #007acc; color: #ffffff;}
.rr-btn-ghost {border: 1px solid rgba(180, 204, 232, 0.6); color: #dce9f7;}
.rr-viewport {position: absolute; right: 18px; bottom: 10px; width: min(44%, 420px); opacity: 0.95; z-index: 2;}
.rr-wire {stroke: #79a8d8; stroke-width: 1.2; fill: none;}
.rr-wire-strong {stroke: #9fd4ff; stroke-width: 1.8; fill: rgba(78, 121, 172, 0.12);}
.rr-section {background: rgba(16, 22, 30, 0.58); border: 1px solid rgba(151, 179, 209, 0.28); border-radius: 18px; padding: 18px; height: 100%; box-sizing: border-box; backdrop-filter: blur(10px); box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05);}
.rr-section h2 {margin: 0 0 10px; font-size: 22px; color: #dbe7f6;}
.rr-section ul {margin: 6px 0 0; padding-left: 20px; color: #c3d2e3; line-height: 1.6;}
.rr-section p {margin: 0; color: #c3d2e3;}
.rr-links {display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 10px; margin-top: 14px;}
.rr-link {display: block; background: rgba(31, 43, 57, 0.7); border: 1px solid rgba(166, 191, 217, 0.38); border-radius: 10px; padding: 10px 12px; color: #e6f0fa; text-decoration: none; font-weight: 700;}
.rr-link:hover {background: rgba(43, 58, 77, 0.9);}
.rr-post-grid {display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 12px;}
.rr-post {display: block; background: rgba(24, 34, 46, 0.72); border: 1px solid rgba(166, 191, 217, 0.32); border-radius: 12px; padding: 14px; color: #e6f0fa; text-decoration: none; min-height: 100%; box-sizing: border-box;}
.rr-post:hover {background: rgba(39, 54, 72, 0.92);}
.rr-post-title {font-weight: 800; margin-bottom: 6px;}
.rr-tag-map {display: flex; flex-wrap: wrap; gap: 8px;}
.rr-tag {display: inline-block; background: rgba(41, 58, 78, 0.78); border: 1px solid rgba(183, 205, 229, 0.34); border-radius: 999px; padding: 7px 12px; color: #e6f0fa; text-decoration: none; font-weight: 700; font-size: 13px;}
.rr-techline {display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; margin-top: 14px;}
.rr-chip {border: 1px solid #c8d2e1; border-radius: 999px; padding: 10px 14px; font-size: 13px; background: #f7fafc; color: #253449;}
@keyframes rr-spin {from {transform: rotate(0deg);} to {transform: rotate(360deg);}}
@media (max-width: 1080px) {.rr-layout {max-width: none;}}
@media (max-width: 920px) {.rr-layout {grid-template-columns: 1fr;}}
@media (max-width: 920px) {.rr-viewport {position: static; width: 100%; margin-top: 18px;}}
@media (max-width: 760px) {.rr-hero {padding: 34px 18px;}}
</style>

<div class="rr-cad-wrap">
  <div class="rr-grid">
    <section class="rr-hero">
      <div class="rr-hero-main">
        <p class="rr-kicker">RedRing CAD-Inspired Experience</p>
        <h1 class="rr-title">CAD設計を、正確に、速く、再利用しやすく。</h1>
                <p class="rr-sub">RedRing は、CAD作業の手戻りを減らし、設計を再利用しやすくすることを目指しています。実務で使える改善を積み重ね、継続的に品質と運用性を高めます。</p>
        <div class="rr-actions">
                    <a class="rr-btn rr-btn-primary" href="/service/">🛠️ 提供サービスを見る</a>
                    <a class="rr-btn rr-btn-ghost" href="/category/redring%E6%80%9D%E6%83%B3/">📐 設計思想を見る</a>
        </div>
      </div>

            <svg class="rr-viewport" viewBox="0 0 420 280" role="img" aria-label="CAD style wireframe">
                <defs>
                    <pattern id="rr-grid-p" width="18" height="18" patternUnits="userSpaceOnUse">
                        <path d="M 18 0 L 0 0 0 18" fill="none" stroke="rgba(143,176,211,0.22)" stroke-width="1"/>
                    </pattern>
                </defs>
                <rect x="0" y="0" width="420" height="280" fill="url(#rr-grid-p)" rx="12"/>
                <polyline class="rr-wire" points="26,214 96,154 188,162 266,126 362,136"/>
                <polyline class="rr-wire" points="26,240 98,176 194,186 274,148 374,164"/>
                <polygon class="rr-wire-strong" points="120,146 192,110 270,150 196,186"/>
                <polygon class="rr-wire" points="192,110 192,62 270,102 270,150"/>
                <polygon class="rr-wire" points="120,146 120,98 192,62 192,110"/>
                <circle cx="300" cy="82" r="20" class="rr-wire"/>
                <line x1="300" y1="62" x2="300" y2="102" class="rr-wire"/>
                <line x1="280" y1="82" x2="320" y2="82" class="rr-wire"/>
            </svg>
    </section>

    <div class="rr-layout">
        <section class="rr-section">
            <h2>カテゴリナビゲーション</h2>
            <ul>
                <li><a href="/category/%e5%ae%9f%e8%a3%85%e8%a8%98%e9%8c%b2/" style="color: #17324f; text-decoration: none; font-weight: 700;">📝 実装記録</a> - 3DCAD・幾何処理の技術ブログ</li>
                <li><a href="/category/redring%E6%80%9D%E6%83%B3/" style="color: #17324f; text-decoration: none; font-weight: 700;">💡 RedRing思想</a> - 設計方針・アーキテクチャ・理念</li>
                <li><a href="/category/%e3%81%8a%e7%9f%a5%e3%82%89%e3%81%9b/" style="color: #17324f; text-decoration: none; font-weight: 700;">📢 お知らせ</a> - サイト更新・リリース情報</li>
            </ul>
        </section>

        <section class="rr-section">
            <h2>RedRing思想</h2>
            <p style="color: #20374f; line-height: 1.8;">Geometry Must Be Explicit. Architecture Must Be Modular.（幾何は明示的に、アーキテクチャはモジュール性を）という設計思想のもと、3DCAD・幾何処理・システム設計の知見を公開しています。</p>
            <div style="margin-top: 14px;">
                <a class="rr-link" href="/category/redring%E6%80%9D%E6%83%B3/" style="display: inline-block;">📐 RedRing思想カテゴリへ</a>
            </div>
        </section>

        <section class="rr-section">
            <h2>タグ・分類マップ</h2>
            <div class="rr-tag-map">
                <a class="rr-tag" href="/tag/mcp/">#MCP</a>
                <a class="rr-tag" href="/tag/rust/">#Rust</a>
                <a class="rr-tag" href="/tag/wordpress/">#WordPress</a>
                <a class="rr-tag" href="/tag/%e3%82%bb%e3%82%ad%e3%83%a5%e3%83%aa%e3%83%86%e3%82%a3/">#セキュリティ</a>
                <a class="rr-tag" href="/tag/%e7%94%9f%e6%88%90ai/">#生成AI</a>
                <a class="rr-tag" href="/tag/%e9%81%8b%e7%94%a8%e8%87%aa%e5%8b%95%e5%8c%96/">#運用自動化</a>
                <a class="rr-tag" href="/tag/%e6%a7%8b%e9%80%a0%e7%be%8e/">#構造美</a>
                <a class="rr-tag" href="/tag/github/">#GitHub</a>
            </div>
        </section>

        <section class="rr-section rr-span-2">
            <h2>GitHub・開発リンク</h2>
            <div class="rr-links">
                <a class="rr-link" href="https://github.com/RedRing2020/RedRing" target="_blank" rel="noopener noreferrer">🔷 RedRing Repository</a>
                <a class="rr-link" href="https://github.com/n-takatsu/mcp-rs" target="_blank" rel="noopener noreferrer">⚙️ mcp-rs（サイト構築基盤）</a>
                <a class="rr-link" href="https://redring.jp/category/%e5%ae%9f%e8%a3%85%e8%a8%98%e9%8c%b2/" target="_blank" rel="noopener noreferrer">🧾 実装記録カテゴリー</a>
            </div>
        </section>
    </div>

  </div>
</div>
<!-- /wp:html -->"#
                .to_string(),
        ),
        (
            SITE_GUIDE_TITLE.to_string(),
            r#"<!-- wp:paragraph -->
<p>このページは、redring.jp を初めて訪れる方に向けた「サイトの使い方ガイド」です。3DCAD 開発を主軸にしつつ、周辺の自動化や実装基盤も含めて、サイトで得られる価値・主要導線・更新方針を短く案内します。</p>
<!-- /wp:paragraph -->

<!-- wp:heading {"level":3} -->
<h3>1. このサイトで得られる価値</h3>
<!-- /wp:heading -->

<!-- wp:list -->
<ul>
    <li>3DCAD、幾何処理、設計自動化を中心に、実装と運用の知見を実例ベースで公開します。</li>
    <li>主軸は 3DCAD 開発ですが、関連する実装基盤として mcp-rs や WordPress 連携も紹介します。</li>
</ul>
<!-- /wp:list -->

<!-- wp:heading {"level":3} -->
<h3>2. 主要コンテンツへの入口</h3>
<!-- /wp:heading -->

<!-- wp:list -->
<ul>
    <li>サイト全体の主要導線は、グローバルメニューから利用してください。</li>
    <li>最新情報は「投稿一覧」、3DCAD を含む開発記録は「実装記録」カテゴリから確認できます。</li>
</ul>
<!-- /wp:list -->

<!-- wp:heading {"level":3} -->
<h3>3. 更新方針</h3>
<!-- /wp:heading -->

<!-- wp:list -->
<ul>
    <li>小さく公開し、検証しながら継続的に改善します。</li>
    <li>内容が固まるまでは、要点を優先して段階的に追記します。</li>
</ul>
<!-- /wp:list -->

<!-- wp:paragraph -->
<p>詳細な実装背景や設計メモは、必要に応じて個別の記事・ドキュメントで公開します。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
        ),
        (
            "サービス".to_string(),
            r#"<!-- wp:heading {"level":2} -->
<h2>提供サービス</h2>
<!-- /wp:heading -->

<!-- wp:list -->
<ul>
    <li>📐 3DCAD アプリケーション設計・開発支援</li>
    <li>🧩 幾何処理・設計自動化ワークフローの実装支援</li>
    <li>⚙️ 関連基盤としての mcp-rs / WordPress 連携・運用支援</li>
</ul>
<!-- /wp:list -->"#
                .to_string(),
        ),
        (
            ABOUT_US_TITLE.to_string(),
            r#"<!-- wp:paragraph -->
<p>RedRing の背景、取り組み方、公開方針をまとめています。現時点では法人紹介ではなく、3DCAD・幾何処理・設計自動化に取り組む活動の概要ページとして運用します。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
        ),
        (
            CONTRIBUTORS_TITLE.to_string(),
            r#"<!-- wp:paragraph -->
<p>RedRing では、GitHub 上で実装・検証・ドキュメント整備に参加してくれる contributors を歓迎しています。興味のある方は、リポジトリの Issue・Pull Request・ドキュメント改善から参加してください。</p>
<!-- /wp:paragraph -->

<!-- wp:list -->
<ul>
    <li><a href="https://github.com/RedRing2020/RedRing" target="_blank" rel="noreferrer noopener">RedRing Repository</a></li>
    <li><a href="https://github.com/n-takatsu/mcp-rs" target="_blank" rel="noreferrer noopener">mcp-rs Repository</a></li>
    <li>まずは README、Issue、既存ドキュメントの改善提案からでも歓迎します。</li>
</ul>
<!-- /wp:list -->"#
                .to_string(),
        ),
        (
            "お問い合わせ".to_string(),
            r#"<!-- wp:paragraph -->
<p>案件相談やお見積り、協業のご相談はフォームからお問い合わせください。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
        ),
        (
            POSTS_PAGE_TITLE.to_string(),
            r#"<!-- wp:paragraph -->
    <p>このページでは、redring.jp のすべての投稿・ブログ記事が時系列で一覧表示されます。最新情報から過去の記事まで、カテゴリやタグから検索・フィルタリングも可能です。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
        ),
    ]
}

fn sample_posts<F, G>(find_cat: &F, find_tag: &G) -> Vec<(String, String, Vec<u64>, Vec<u64>)>
where
    F: Fn(&str) -> Option<u64>,
    G: Fn(&str) -> Option<u64>,
{
    vec![
        (
            "redring.jp を公開しました".to_string(),
            r#"<!-- wp:paragraph -->
<p>redring.jp を公開しました。3DCAD 開発を主軸に、関連する設計・実装の記録を段階的に公開していきます。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
            vec![find_cat("ウェブサイト")].into_iter().flatten().collect(),
            vec![
                find_tag("RedRing"),
                find_tag("WordPress"),
                find_tag("サイト運用"),
                find_tag("構造美"),
            ]
                .into_iter()
                .flatten()
                .collect(),
        ),
        (
            "トップページ構成案を反映しました".to_string(),
            r#"<!-- wp:paragraph -->
<p>トップページ構成案をもとに、初回公開後の実運用で見えてきた課題を反映した更新を行いました。今回の目的は、訪問直後に「このサイトで何が得られるか」を短時間で把握できる状態をつくることです。</p>
<!-- /wp:paragraph -->

<!-- wp:heading {"level":3} -->
<h3>今回の更新背景</h3>
<!-- /wp:heading -->

<!-- wp:paragraph -->
<p>公開直後の構成では、導線の重複やメニュー項目の混在により、初見ユーザーが主要コンテンツへ到達するまでに迷いやすい箇所がありました。特に、ヘッダー導線とサイドバー導線の役割分担を明確にする必要がありました。</p>
<!-- /wp:paragraph -->

<!-- wp:heading {"level":3} -->
<h3>主な変更内容</h3>
<!-- /wp:heading -->

<!-- wp:list -->
<ul>
    <li>ヒーロー領域のメッセージを中心に、サイトの提供価値を先頭で明確化。</li>
    <li>カテゴリ導線とタグ分類マップを整理し、技術記事への到達経路を短縮。</li>
    <li>GitHubリンクを独立セクション化し、実装情報へのアクセスを強化。</li>
    <li>メニュー重複を解消し、主要導線をホーム／サイトガイド／サービス／投稿一覧／About Us／お問い合わせへ統一。</li>
    <li>投稿一覧ページは本文説明と実記事表示の役割を整理し、閲覧時のノイズを削減。</li>
</ul>
<!-- /wp:list -->

<!-- wp:heading {"level":3} -->
<h3>更新後に期待する効果</h3>
<!-- /wp:heading -->

<!-- wp:paragraph -->
<p>サイト内回遊の起点が明確になり、初見ユーザーは「どこを見ればよいか」を短時間で判断しやすくなります。また、運用側としても導線の責務が整理され、今後の改修時に影響範囲を限定しやすくなります。</p>
<!-- /wp:paragraph -->

<!-- wp:heading {"level":3} -->
<h3>今後の方針</h3>
<!-- /wp:heading -->

<!-- wp:paragraph -->
<p>今後は、アクセス状況と閲覧行動を見ながら、カテゴリごとの導線改善と記事アーカイブの見つけやすさを段階的に強化します。大きな改修は避けつつ、実運用に合わせた小さな改善を継続していきます。</p>
<!-- /wp:paragraph -->
"#
                .to_string(),
            vec![find_cat("ウェブサイト")].into_iter().flatten().collect(),
            vec![
                find_tag("WordPress"),
                find_tag("サイト運用"),
                find_tag("UI改善"),
                find_tag("導線整理"),
            ]
                .into_iter()
                .flatten()
                .collect(),
        ),
        (
            "監査ログとセキュリティ監視を強化しました".to_string(),
            r#"<!-- wp:paragraph -->
<p>監査ログとセキュリティ監視の運用を強化しました。この記事は、MCPサーバー経由でWordPressを更新・確認する運用担当者を主な対象にしています。単に「記録する」だけでなく、あとから追跡できる粒度で変更内容を残し、異常に気づける状態を優先しています。</p>
<!-- /wp:paragraph -->

<!-- wp:paragraph -->
<p>この投稿は、mcp-rs の WordPress デモ構築部分を実例として見せるために公開しています。redring.jp をどのように立ち上げ、どのように運用情報を残すかを示すことで、コード側のデモと実際のサイト運用をつなげる狙いがあります。詳しくは <a href="https://github.com/n-takatsu/mcp-rs" target="_blank" rel="noreferrer noopener">mcp-rs のリポジトリ</a> と <a href="https://redring.jp/" target="_blank" rel="noreferrer noopener">redring.jp</a> を参照してください。</p>
<!-- /wp:paragraph -->

<!-- wp:heading {"level":3} -->
<h3>対象読者</h3>
<!-- /wp:heading -->

<!-- wp:paragraph -->
<p>この内容は、MCPサーバーを使ってWordPressの更新や点検を自動化している人、あるいは人手での更新と自動処理を併用しながらサイト運用を進めている人を想定しています。サイト公開後の変更履歴を残したい、障害時に原因をすばやく追いたい、という運用寄りの課題に向けた話です。</p>
<!-- /wp:paragraph -->

<!-- wp:heading {"level":3} -->
<h3>見直しの背景</h3>
<!-- /wp:heading -->

<!-- wp:paragraph -->
<p>サイト公開後は、コンテンツ更新や設定変更が少しずつ増えていきます。そのため、どの変更が、いつ、誰によって行われ、結果として何が変わったのかを後から把握できる形にしておく必要がありました。</p>
<!-- /wp:paragraph -->

<!-- wp:heading {"level":3} -->
<h3>監査ログの粒度</h3>
<!-- /wp:heading -->

<!-- wp:list -->
<ul>
    <li>設定更新は、変更前後の差分が追える粒度で記録します。</li>
    <li>投稿や固定ページの更新は、公開状態と更新対象が分かる形で残します。</li>
    <li>カテゴリやタグの再分類は、導線設計への影響を確認できるように扱います。</li>
    <li>メニューやフロントページ設定の変更は、ユーザー体験に直結するため重点的に追跡します。</li>
</ul>
<!-- /wp:list -->

<!-- wp:heading {"level":3} -->
<h3>監視の観点</h3>
<!-- /wp:heading -->

<!-- wp:list -->
<ul>
    <li>REST API の失敗や権限エラーを早めに検知します。</li>
    <li>想定外の更新や差分が出たときに原因を追跡しやすくします。</li>
    <li>公開後の運用で継続的に確認すべき項目を絞り込みます。</li>
</ul>
<!-- /wp:list -->

<!-- wp:heading {"level":3} -->
<h3>運用上の効果</h3>
<!-- /wp:heading -->

<!-- wp:paragraph -->
<p>この見直しによって、変更時の確認コストを下げながら、問題が起きたときの切り分けを速くできます。公開直後の段階でも、将来の自動アラートや監査運用へつなげやすい土台になります。</p>
<!-- /wp:paragraph -->"#
                .to_string(),
            vec![find_cat("技術信頼性")].into_iter().flatten().collect(),
            vec![
                find_tag("セキュリティ"),
                find_tag("監査ログ"),
                find_tag("監視"),
                find_tag("運用自動化"),
                find_tag("WordPress"),
            ]
                .into_iter()
                .flatten()
                .collect(),
        ),
    ]
}
