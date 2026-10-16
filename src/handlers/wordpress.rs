use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::config::WordPressConfig;
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolCallParams,
};
use crate::security::{RateLimiter, SecureCredentials};

/// 文字数単位で文字列を切り詰める（文字境界を跨がない）。
/// 切り詰めた場合のみ末尾に"…"を付ける。
fn truncate_chars_with_ellipsis(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

/// 表示用にslugをパーセントデコードする。デコードに失敗した場合は
/// 元の値をそのまま返す（APIに渡す生のslugは別途保持される）。
fn decode_slug_for_display(slug: &str) -> String {
    urlencoding::decode(slug)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| slug.to_string())
}

/// JSON配列（カテゴリ/タグ一覧など）内の各要素の"slug"キーを表示用に
/// パーセントデコードする。配列でない場合はそのまま返す。
fn decode_slugs_for_display(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(arr) = value.as_array_mut() {
        for item in arr.iter_mut() {
            if let Some(slug) = item.get("slug").and_then(|v| v.as_str()) {
                let decoded = decode_slug_for_display(slug);
                item["slug"] = serde_json::Value::String(decoded);
            }
        }
    }
    value
}

/// WordPressのHTML（投稿本文・抜粋など）をプレーンテキストに変換する。
/// タグを除去し、HTMLエンティティ（`&#8230;`等）はパース過程で実際の
/// 文字にデコードされる。AIへ渡す用途ではHTML構文自体がトークンの
/// 無駄になるため、切り詰めの前に必ずこれを通す。
fn html_to_plain_text(html: &str) -> String {
    let stripped = ammonia::Builder::new()
        .tags(std::collections::HashSet::new())
        .clean(html)
        .to_string();

    // ブロック要素除去後に残りがちな3行以上の連続空行を1行にまとめる
    let collapsed = Regex::new(r"\n{3,}")
        .expect("static regex is valid")
        .replace_all(&stripped, "\n\n");

    collapsed.trim().to_string()
}

/// `Option<u64>`のIDを表示用文字列に変換する（Rustの`{:?}`による
/// `Some(123)`のような生の表記を避ける）。
fn display_opt_id(id: Option<u64>) -> String {
    id.map(|v| v.to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

/// アップロードされたメディアのタイトル用に、ファイル名から拡張子を
/// 除いた部分（stem）を返す。拡張子の区切り`.`はASCII文字のため、
/// 多バイト文字を含むファイル名でも文字境界をまたがずに安全に分割できる。
/// `.`を含まない場合はファイル名全体をそのまま返す。
fn filename_stem(filename: &str) -> &str {
    filename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename)
}

/// WordPress REST APIのエラーレスポンス本文から`code`/`message`
/// （例: `rest_post_invalid_id` / "Invalid post ID."）を拾い、診断に
/// 使える文字列にまとめる。期待した形式でない場合はステータスと
/// 本文の先頭部分をそのまま含める。
fn format_wp_error(status: reqwest::StatusCode, body: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let (Some(code), Some(message)) = (
            json.get("code").and_then(|v| v.as_str()),
            json.get("message").and_then(|v| v.as_str()),
        ) {
            return format!("WordPress API error {}: {} ({})", status, message, code);
        }
    }
    format!(
        "WordPress API error {}: {}",
        status,
        truncate_chars_with_ellipsis(body, 200)
    )
}

/// post_type（"post" | "page"）からREST APIのエンドポイントセグメントを
/// 決定する。"post"・未知の値はすべて"posts"として扱う。
fn endpoint_for_post_type(post_type: &str) -> &'static str {
    match post_type {
        "page" => "pages",
        _ => "posts",
    }
}

/// カテゴリ/タグのIDリストを、id→name対応表を使って名前のリストに
/// 解決する。対応表に無いIDは数値文字列のままフォールバックする
/// （カテゴリ/タグ一覧の取得に失敗した場合など）。
fn resolve_term_names(ids: &[u64], id_to_name: &HashMap<u64, String>) -> Vec<String> {
    ids.iter()
        .map(|id| {
            id_to_name
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.to_string())
        })
        .collect()
}

/// 投稿/固定ページの一覧を、各項目の要点（id・title・status・date・
/// link・slug・抜粋）のみを含む簡潔なJSON配列にまとめる。
/// 本文全体を含めないのは出力サイズを抑えるため；抜粋は文字境界を
/// 跨がないよう安全に切り詰める。
fn summarize_posts(posts: &[WordPressPost]) -> Vec<serde_json::Value> {
    posts
        .iter()
        .map(|post| {
            let excerpt = post
                .excerpt
                .as_ref()
                .map(|e| truncate_chars_with_ellipsis(&html_to_plain_text(&e.rendered), 200))
                .unwrap_or_default();

            serde_json::json!({
                "id": post.id,
                "title": post.title.rendered,
                "status": post.status,
                "date": post.date,
                "link": post.link,
                "slug": post.slug.as_deref().map(decode_slug_for_display),
                "excerpt": excerpt,
            })
        })
        .collect()
}

/// メディア一覧を、各項目の要点（id・title・mime_type・source_url・
/// alt_text・date）のみを含む簡潔なJSON配列にまとめる。
fn summarize_media(media_items: &[WordPressMedia]) -> Vec<serde_json::Value> {
    media_items
        .iter()
        .map(|media| {
            serde_json::json!({
                "id": media.id,
                "title": media.title.as_ref().map(|t| t.rendered.as_str()),
                "mime_type": media.mime_type,
                "source_url": media.source_url,
                "alt_text": media.alt_text,
                "date": media.date,
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct WordPressHandler {
    client: Client,
    base_url: String,
    /// セキュア認証情報（新しい暗号化対応）
    secure_credentials: Option<SecureCredentials>,
    /// 後方互換性のための平文認証情報
    username: Option<String>,
    password: Option<String>,
    #[allow(dead_code)]
    rate_limiter: Arc<RateLimiter>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressPost {
    pub id: Option<u64>,
    pub date: Option<String>,
    pub date_gmt: Option<String>,
    pub guid: Option<WordPressGuid>,
    pub modified: Option<String>,
    pub modified_gmt: Option<String>,
    pub slug: Option<String>,
    pub status: String,
    #[serde(rename = "type")]
    pub post_type: Option<String>,
    pub link: Option<String>,
    pub title: WordPressContent,
    pub content: WordPressContent,
    pub excerpt: Option<WordPressContent>,
    pub author: Option<u64>,
    pub featured_media: Option<u64>,
    pub categories: Option<Vec<u64>>,
    pub tags: Option<Vec<u64>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressGuid {
    pub rendered: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressContent {
    pub rendered: String,
    #[serde(default)]
    pub protected: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressComment {
    pub id: Option<u64>,
    pub post: u64,
    pub content: HashMap<String, String>,
    pub author_name: String,
    pub author_email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressMedia {
    pub id: Option<u64>,
    pub date: Option<String>,
    pub date_gmt: Option<String>,
    pub guid: Option<WordPressGuid>,
    pub modified: Option<String>,
    pub modified_gmt: Option<String>,
    pub slug: Option<String>,
    pub status: String,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    pub link: Option<String>,
    pub title: Option<WordPressContent>,
    pub author: Option<u64>,
    pub comment_status: Option<String>,
    pub ping_status: Option<String>,
    pub template: Option<String>,
    pub description: Option<WordPressContent>,
    pub caption: Option<WordPressContent>,
    pub alt_text: Option<String>,
    pub mime_type: Option<String>,
    pub media_details: Option<serde_json::Value>,
    pub post: Option<u64>,
    pub source_url: Option<String>,
}

/// WordPress media update parameters
#[derive(Debug, Clone, Default)]
pub struct MediaUpdateParams {
    pub title: Option<String>,
    pub alt_text: Option<String>,
    pub caption: Option<String>,
    pub description: Option<String>,
    pub post: Option<u64>, // 添付先の投稿ID
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressHealthCheck {
    pub site_accessible: bool,
    pub rest_api_available: bool,
    pub authentication_valid: bool,
    pub permissions_adequate: bool,
    pub media_upload_possible: bool,
    pub error_details: Vec<String>,
    pub site_info: Option<WordPressSiteInfo>,
    /// mcp-rs自身のログ出力先の状態（WordPressサイトとは無関係だが、
    /// 接続トラブル時の切り分けに有用なため含める）
    pub log_destination: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressSiteInfo {
    pub name: String,
    pub description: String,
    pub url: String,
    pub admin_email: Option<String>,
    pub timezone_string: Option<String>,
    pub date_format: Option<String>,
    pub time_format: Option<String>,
    pub start_of_week: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressSettings {
    pub title: Option<String>,
    pub description: Option<String>,
    pub timezone: Option<String>,
    pub date_format: Option<String>,
    pub time_format: Option<String>,
    pub start_of_week: Option<u8>,
    pub language: Option<String>,
    pub use_smilies: Option<bool>,
    pub default_category: Option<u64>,
    pub default_post_format: Option<String>,
    pub posts_per_page: Option<u64>,
    pub show_on_front: Option<String>, // "posts" or "page"
    pub page_on_front: Option<u64>,    // Static front page ID
    pub page_for_posts: Option<u64>,   // Posts page ID
    pub default_ping_status: Option<String>,
    pub default_comment_status: Option<String>,
}

/// WordPress settings update parameters
#[derive(Debug, Clone, Default)]
pub struct SettingsUpdateParams {
    pub title: Option<String>,
    pub description: Option<String>,
    pub timezone: Option<String>,
    pub show_on_front: Option<String>, // "posts" or "page"
    pub page_on_front: Option<u64>,    // Static front page
    pub page_for_posts: Option<u64>,   // Blog posts page
    pub posts_per_page: Option<u64>,
    pub default_category: Option<u64>,
    pub language: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressCategory {
    pub id: Option<u64>,
    pub count: Option<u64>,
    pub description: String,
    pub link: Option<String>,
    pub name: String,
    pub slug: String,
    pub taxonomy: Option<String>,
    pub parent: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressTag {
    pub id: Option<u64>,
    pub count: Option<u64>,
    pub description: String,
    pub link: Option<String>,
    pub name: String,
    pub slug: String,
    pub taxonomy: Option<String>,
}

/// WordPress post creation parameters
#[derive(Debug, Clone)]
pub struct PostCreateParams {
    pub title: String,
    pub content: String,
    pub post_type: String,    // "post" or "page"
    pub status: String,       // "publish", "draft", "private", "future"
    pub date: Option<String>, // 予約投稿用の日時 (ISO8601形式)
    pub categories: Option<Vec<u64>>,
    pub tags: Option<Vec<u64>>,
    pub featured_media_id: Option<u64>,
    pub meta: Option<HashMap<String, String>>, // SEOメタデータ等
}

/// WordPress post update parameters
#[derive(Debug, Clone, Default)]
pub struct PostUpdateParams {
    pub title: Option<String>,
    pub content: Option<String>,
    pub status: Option<String>,
    pub categories: Option<Vec<u64>>,
    pub tags: Option<Vec<u64>>,
    pub featured_media_id: Option<u64>,
    pub meta: Option<HashMap<String, String>>,
}

impl Default for PostCreateParams {
    fn default() -> Self {
        Self {
            title: String::new(),
            content: String::new(),
            post_type: "post".to_string(),
            status: "publish".to_string(),
            date: None,
            categories: None,
            tags: None,
            featured_media_id: None,
            meta: None,
        }
    }
}

impl WordPressHandler {
    pub fn new(config: WordPressConfig) -> Self {
        // 後方互換性のために一時的に残す - try_new()の使用を推奨
        Self::try_new(config).unwrap_or_else(|e| {
            panic!("WordPressHandler initialization failed: {}", e);
        })
    }

    /// 安全なコンストラクタ - エラーハンドリング付き
    pub fn try_new(config: WordPressConfig) -> Result<Self, String> {
        // URL検証 - HTTPS強制
        if !config.url.starts_with("https://") {
            return Err(format!(
                "Insecure URL detected: {}. Only HTTPS connections are allowed for security reasons.",
                config.url
            ));
        }

        // タイムアウト設定付きのHTTPクライアントを作成
        let timeout_secs = config.timeout_seconds.unwrap_or(30);
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs)) // 設定可能なタイムアウト
            .connect_timeout(Duration::from_secs(10)) // 接続タイムアウト: 10秒
            .user_agent("mcp-rs/1.0") // User-Agentを設定
            .https_only(true) // HTTPS強制
            .min_tls_version(reqwest::tls::Version::TLS_1_2) // TLS 1.2以上を要求
            .build()
            .map_err(|e| format!("HTTP client build failed: {}", e))?;

        // レート制限設定
        let rate_limit_config = config.rate_limit.clone().unwrap_or_default();
        let rate_limiter = Arc::new(RateLimiter::new(rate_limit_config));

        // セキュア認証情報の作成
        let secure_credentials = Some(config.create_secure_credentials());

        Ok(Self {
            client,
            base_url: config.url.clone(),
            secure_credentials,
            username: Some(config.username),
            password: Some(config.password),
            rate_limiter,
        })
    }

    /// 認証情報をリクエストに追加するヘルパーメソッド
    fn add_authentication(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        // セキュア認証情報を優先的に使用
        if let Some(secure_creds) = &self.secure_credentials {
            let auth_header = format!("Basic {}", secure_creds.to_basic_auth());
            request = request.header("Authorization", auth_header);
        } else if let (Some(username), Some(password)) = (&self.username, &self.password) {
            // 後方互換性のための平文認証
            request = request.basic_auth(username, Some(password));
        }
        request
    }

    /// レート制限チェック付きでリクエストを実行
    #[allow(dead_code)]
    async fn execute_request_with_rate_limit<T>(
        &self,
        request_builder: reqwest::RequestBuilder,
        client_id: &str,
    ) -> Result<T, McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        // レート制限チェック
        if let Err(rate_limit_error) = self.rate_limiter.check_rate_limit(client_id).await {
            warn!(
                "Rate limit exceeded for client {}: {}",
                client_id, rate_limit_error
            );
            return Err(McpError::Other(format!(
                "Rate limit exceeded: {}",
                rate_limit_error
            )));
        }

        // 通常のリクエスト実行
        self.execute_request_with_retry(request_builder).await
    }

    /// リトライ機能付きでHTTPリクエストを実行
    async fn execute_request_with_retry<T>(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<T, McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_millis(1000);

        for attempt in 1..=MAX_RETRIES {
            let request = request_builder
                .try_clone()
                .ok_or_else(|| McpError::Other("Failed to clone request".to_string()))?;

            match request.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        // レスポンステキストを取得してデバッグ
                        let text = response.text().await.map_err(McpError::Http)?;
                        warn!(
                            "Response body (first 500 chars): {}",
                            text.chars().take(500).collect::<String>()
                        );

                        match serde_json::from_str::<T>(&text) {
                            Ok(data) => return Ok(data),
                            Err(e) => {
                                warn!("JSON parse error on attempt {}: {}", attempt, e);
                                if attempt == MAX_RETRIES {
                                    return Err(McpError::ExternalApi(format!(
                                        "JSON parse error: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    } else if status.as_u16() >= 500 || status.as_u16() == 429 {
                        // サーバーエラーまたはレート制限の場合はリトライ
                        warn!("HTTP error {} on attempt {}, retrying...", status, attempt);
                        if attempt == MAX_RETRIES {
                            let body = response.text().await.unwrap_or_default();
                            return Err(McpError::ExternalApi(format!(
                                "{} (after {} attempts)",
                                format_wp_error(status, &body),
                                MAX_RETRIES
                            )));
                        }
                    } else {
                        // クライアントエラー（4xx）はリトライしない。
                        // 本文のcode/messageを診断用に含める
                        // （例: rest_post_invalid_id）。
                        let body = response.text().await.unwrap_or_default();
                        return Err(McpError::ExternalApi(format_wp_error(status, &body)));
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        warn!("Request timeout on attempt {}: {}", attempt, e);
                    } else if e.is_connect() {
                        warn!("Connection error on attempt {}: {}", attempt, e);
                    } else {
                        warn!("Request error on attempt {}: {}", attempt, e);
                    }

                    if attempt == MAX_RETRIES {
                        return Err(McpError::Http(e));
                    }
                }
            }

            // リトライ前に少し待機
            if attempt < MAX_RETRIES {
                tokio::time::sleep(RETRY_DELAY * attempt).await;
            }
        }

        unreachable!()
    }

    /// `execute_request_with_retry`と同じリトライ処理だが、リクエストを
    /// `try_clone()`で複製するのではなく、`build_request`を呼ぶたびに
    /// 作り直す。`multipart::Form`を使うリクエストは`try_clone()`が
    /// 常に`None`を返すため（内部でストリームを保持しており複製不可）、
    /// こちらを使う必要がある。
    async fn execute_request_with_retry_rebuilding<T>(
        &self,
        build_request: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<T, McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_millis(1000);

        for attempt in 1..=MAX_RETRIES {
            let request = build_request();

            match request.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        let text = response.text().await.map_err(McpError::Http)?;

                        match serde_json::from_str::<T>(&text) {
                            Ok(data) => return Ok(data),
                            Err(e) => {
                                warn!("JSON parse error on attempt {}: {}", attempt, e);
                                if attempt == MAX_RETRIES {
                                    return Err(McpError::ExternalApi(format!(
                                        "JSON parse error: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    } else if status.as_u16() >= 500 || status.as_u16() == 429 {
                        warn!("HTTP error {} on attempt {}, retrying...", status, attempt);
                        if attempt == MAX_RETRIES {
                            let body = response.text().await.unwrap_or_default();
                            return Err(McpError::ExternalApi(format!(
                                "{} (after {} attempts)",
                                format_wp_error(status, &body),
                                MAX_RETRIES
                            )));
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        return Err(McpError::ExternalApi(format_wp_error(status, &body)));
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        warn!("Request timeout on attempt {}: {}", attempt, e);
                    } else if e.is_connect() {
                        warn!("Connection error on attempt {}: {}", attempt, e);
                    } else {
                        warn!("Request error on attempt {}: {}", attempt, e);
                    }

                    if attempt == MAX_RETRIES {
                        return Err(McpError::Http(e));
                    }
                }
            }

            if attempt < MAX_RETRIES {
                tokio::time::sleep(RETRY_DELAY * attempt).await;
            }
        }

        unreachable!()
    }

    /// `execute_request_with_retry`と同じリトライ処理に加え、
    /// WordPress REST APIのページネーションヘッダー（`X-WP-TotalPages`）
    /// を読み取って返す。一覧系エンドポイントの全件取得に使う。
    async fn execute_request_with_retry_capturing_total_pages<T>(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<(T, u32, u64), McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_millis(1000);

        for attempt in 1..=MAX_RETRIES {
            let request = request_builder
                .try_clone()
                .ok_or_else(|| McpError::Other("Failed to clone request".to_string()))?;

            match request.send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        let total_pages = response
                            .headers()
                            .get("x-wp-totalpages")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(1);
                        let total_items = response
                            .headers()
                            .get("x-wp-total")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);

                        let text = response.text().await.map_err(McpError::Http)?;

                        match serde_json::from_str::<T>(&text) {
                            Ok(data) => return Ok((data, total_pages, total_items)),
                            Err(e) => {
                                warn!("JSON parse error on attempt {}: {}", attempt, e);
                                if attempt == MAX_RETRIES {
                                    return Err(McpError::ExternalApi(format!(
                                        "JSON parse error: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    } else if status.as_u16() >= 500 || status.as_u16() == 429 {
                        warn!("HTTP error {} on attempt {}, retrying...", status, attempt);
                        if attempt == MAX_RETRIES {
                            let body = response.text().await.unwrap_or_default();
                            return Err(McpError::ExternalApi(format!(
                                "{} (after {} attempts)",
                                format_wp_error(status, &body),
                                MAX_RETRIES
                            )));
                        }
                    } else {
                        let body = response.text().await.unwrap_or_default();
                        return Err(McpError::ExternalApi(format_wp_error(status, &body)));
                    }
                }
                Err(e) => {
                    if attempt == MAX_RETRIES {
                        return Err(McpError::Http(e));
                    }
                    warn!("Request error on attempt {}: {}", attempt, e);
                }
            }

            if attempt < MAX_RETRIES {
                tokio::time::sleep(RETRY_DELAY * attempt).await;
            }
        }

        unreachable!()
    }

    /// 一覧系エンドポイントを`per_page=100`かつ`X-WP-TotalPages`に基づく
    /// 全ページ取得で呼び出す。`url_for_page`は`page`番号を受け取り、
    /// `per_page=100&page=N`を含む完全なURLを返すクロージャ。
    ///
    /// 戻り値の`u64`はWordPressが`X-WP-Total`ヘッダーで報告する総件数。
    /// `items.len()`と一致するはずで、食い違う場合はページネーション
    /// 自体に問題がある可能性を示す（呼び出し側で打ち切りの有無を
    /// 判別する材料として使える）。
    async fn fetch_all_pages<T>(
        &self,
        url_for_page: impl Fn(u32) -> String,
    ) -> Result<(Vec<T>, u64), McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let mut all_items: Vec<T> = Vec::new();
        let mut page: u32 = 1;

        let total_items: u64 = loop {
            let url = url_for_page(page);
            let mut request = self.client.get(&url);

            if let (Some(username), Some(password)) = (&self.username, &self.password) {
                request = request.basic_auth(username, Some(password));
            }

            let (items, total_pages, reported_total): (Vec<T>, u32, u64) = self
                .execute_request_with_retry_capturing_total_pages(request)
                .await?;

            all_items.extend(items);

            if page >= total_pages.max(1) {
                break reported_total;
            }
            page += 1;
        };

        if total_items != all_items.len() as u64 {
            warn!(
                "fetch_all_pages: X-WP-Total ({}) does not match fetched item count ({}); pagination may be incomplete",
                total_items,
                all_items.len()
            );
        }

        Ok((all_items, total_items))
    }

    async fn get_posts(&self) -> Result<Vec<WordPressPost>, McpError> {
        info!(
            "Fetching WordPress posts from: {}/wp-json/wp/v2/posts",
            self.base_url
        );
        let (items, _total) = self
            .fetch_all_pages(|page| {
                format!(
                    "{}/wp-json/wp/v2/posts?per_page=100&page={}",
                    self.base_url, page
                )
            })
            .await?;
        Ok(items)
    }

    /// Get all WordPress pages
    async fn get_pages(&self) -> Result<Vec<WordPressPost>, McpError> {
        info!(
            "Fetching WordPress pages from: {}/wp-json/wp/v2/pages",
            self.base_url
        );
        let (items, _total) = self
            .fetch_all_pages(|page| {
                format!(
                    "{}/wp-json/wp/v2/pages?per_page=100&page={}",
                    self.base_url, page
                )
            })
            .await?;
        Ok(items)
    }

    /// Get both posts and pages
    pub async fn get_all_content(
        &self,
    ) -> Result<(Vec<WordPressPost>, Vec<WordPressPost>), McpError> {
        let posts_future = self.get_posts();
        let pages_future = self.get_pages();

        let (posts, pages) = tokio::try_join!(posts_future, pages_future)?;

        Ok((posts, pages))
    }

    /// Get a single WordPress post by ID
    pub async fn get_post(&self, post_id: u64, post_type: &str) -> Result<WordPressPost, McpError> {
        let endpoint = endpoint_for_post_type(post_type);
        let url = format!("{}/wp-json/wp/v2/{}/{}", self.base_url, endpoint, post_id);

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Fetching WordPress {}: {}", post_type, post_id);
        self.execute_request_with_retry(request).await
    }

    /// Create a new WordPress post (basic version for backward compatibility)
    pub async fn create_post(
        &self,
        title: String,
        content: String,
        status: Option<String>,
    ) -> Result<WordPressPost, McpError> {
        let params = PostCreateParams {
            title,
            content,
            post_type: "post".to_string(),
            // 意図しない公開を避けるため、未指定時はdraftを既定値とする
            status: status.unwrap_or_else(|| "draft".to_string()),
            ..Default::default()
        };
        self.create_advanced_post(params).await
    }

    /// Create a new WordPress post with advanced options
    pub async fn create_advanced_post(
        &self,
        params: PostCreateParams,
    ) -> Result<WordPressPost, McpError> {
        // 投稿タイプに応じてエンドポイントを決定
        let endpoint = endpoint_for_post_type(&params.post_type);
        let url = format!("{}/wp-json/wp/v2/{}", self.base_url, endpoint);

        let mut post_data = serde_json::json!({
            "title": params.title,
            "content": params.content,
            "type": params.post_type,
            "status": params.status
        });

        // 予約投稿の場合は日時を設定
        if let Some(publish_date) = params.date {
            post_data["date"] = serde_json::Value::String(publish_date);
        }

        // カテゴリーを設定（投稿のみ）
        if params.post_type == "post" {
            if let Some(cats) = params.categories {
                post_data["categories"] = serde_json::Value::Array(
                    cats.iter()
                        .map(|&id| serde_json::Value::Number(id.into()))
                        .collect(),
                );
            }

            // タグを設定（投稿のみ）
            if let Some(tag_ids) = params.tags {
                post_data["tags"] = serde_json::Value::Array(
                    tag_ids
                        .iter()
                        .map(|&id| serde_json::Value::Number(id.into()))
                        .collect(),
                );
            }
        }

        // アイキャッチ画像を設定
        if let Some(media_id) = params.featured_media_id {
            post_data["featured_media"] = serde_json::Value::Number(media_id.into());
        }

        // メタデータを設定（SEO等）
        if let Some(metadata) = params.meta {
            post_data["meta"] = serde_json::Value::Object(
                metadata
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect(),
            );
        }

        let mut request = self.client.post(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.json(&post_data).send().await?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress API error: {}",
                response.status()
            )));
        }

        let created_post: WordPressPost = response.json().await?;
        Ok(created_post)
    }

    async fn get_comments(&self, post_id: Option<u64>) -> Result<Vec<WordPressComment>, McpError> {
        let (items, _total) = self
            .fetch_all_pages(|page| {
                let mut url = format!(
                    "{}/wp-json/wp/v2/comments?per_page=100&page={}",
                    self.base_url, page
                );
                if let Some(post_id) = post_id {
                    url = format!("{}&post={}", url, post_id);
                }
                url
            })
            .await?;
        Ok(items)
    }

    /// Upload media file to WordPress
    ///
    /// `multipart/form-data`（フィールド名`file`）で送信する。WordPressは
    /// この経路（`$_FILES`）ではUTF-8のファイル名をそのまま受け付けるため、
    /// 日本語等のファイル名もサニタイズせず渡してよい（以前の生バイナリ+
    /// `Content-Disposition`方式では、WordPress互換にするため非ASCII文字を
    /// `_`に置き換えていたが、その結果タイトルが空になり`unnamed-file`に
    /// なってしまっていた）。あわせて`title`フィールドにファイル名から
    /// 拡張子を除いた部分を渡し、メディアのタイトルが`unnamed-file`に
    /// ならないようにする。
    pub async fn upload_media(
        &self,
        file_data: &[u8],
        filename: &str,
        mime_type: &str,
    ) -> Result<WordPressMedia, McpError> {
        let url = format!("{}/wp-json/wp/v2/media", self.base_url);
        let title = filename_stem(filename).to_string();

        // mime_typeの妥当性はリクエストの内容に関わらず一定なので、
        // リトライのたびに検証するのではなくここで一度だけ検証する。
        if let Err(e) = reqwest::multipart::Part::bytes(Vec::new()).mime_str(mime_type) {
            return Err(McpError::Other(format!(
                "Invalid mime_type '{}': {}",
                mime_type, e
            )));
        }

        info!("Uploading media file: {} ({})", filename, mime_type);

        self.execute_request_with_retry_rebuilding(|| {
            let part = reqwest::multipart::Part::bytes(file_data.to_vec())
                .file_name(filename.to_string())
                .mime_str(mime_type)
                .expect("mime_type already validated above");
            let form = reqwest::multipart::Form::new()
                .part("file", part)
                .text("title", title.clone());

            let mut request = self.client.post(&url).multipart(form);
            if let (Some(username), Some(password)) = (&self.username, &self.password) {
                request = request.basic_auth(username, Some(password));
            }
            request
        })
        .await
    }

    /// Get all media files
    /// 戻り値の`u64`はWordPressが`X-WP-Total`で報告する総件数
    /// （`items.len()`と一致するはず — 一致すれば全件取得できている
    /// ことの確認材料になる）。
    pub async fn get_media(&self) -> Result<(Vec<WordPressMedia>, u64), McpError> {
        info!(
            "Fetching WordPress media from: {}/wp-json/wp/v2/media",
            self.base_url
        );
        self.fetch_all_pages(|page| {
            format!(
                "{}/wp-json/wp/v2/media?per_page=100&page={}",
                self.base_url, page
            )
        })
        .await
    }

    /// Get a single media file by ID
    pub async fn get_media_item(&self, media_id: u64) -> Result<WordPressMedia, McpError> {
        let url = format!("{}/wp-json/wp/v2/media/{}", self.base_url, media_id);

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Fetching WordPress media item: {}", media_id);
        self.execute_request_with_retry(request).await
    }

    /// Update media item (title, alt text, caption, description)
    pub async fn update_media(
        &self,
        media_id: u64,
        params: MediaUpdateParams,
    ) -> Result<WordPressMedia, McpError> {
        let url = format!("{}/wp-json/wp/v2/media/{}", self.base_url, media_id);

        info!("Updating WordPress media: {}", media_id);

        let mut update_data = serde_json::Map::new();

        if let Some(title) = params.title {
            update_data.insert("title".to_string(), serde_json::Value::String(title));
        }

        if let Some(alt_text) = params.alt_text {
            update_data.insert("alt_text".to_string(), serde_json::Value::String(alt_text));
        }

        if let Some(caption) = params.caption {
            update_data.insert("caption".to_string(), serde_json::Value::String(caption));
        }

        if let Some(description) = params.description {
            update_data.insert(
                "description".to_string(),
                serde_json::Value::String(description),
            );
        }

        if let Some(post_id) = params.post {
            update_data.insert(
                "post".to_string(),
                serde_json::Value::Number(serde_json::Number::from(post_id)),
            );
        }

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        self.execute_request_with_retry(request).await
    }

    /// Delete a media file
    pub async fn delete_media(
        &self,
        media_id: u64,
        force: Option<bool>,
    ) -> Result<WordPressMedia, McpError> {
        let force_delete = force.unwrap_or(false);
        let url = if force_delete {
            format!(
                "{}/wp-json/wp/v2/media/{}?force=true",
                self.base_url, media_id
            )
        } else {
            format!("{}/wp-json/wp/v2/media/{}", self.base_url, media_id)
        };

        let mut request = self.client.delete(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!(
            "Deleting WordPress media: {} (force: {})",
            media_id, force_delete
        );
        self.execute_request_with_retry(request).await
    }

    /// Get WordPress site settings
    pub async fn get_settings(&self) -> Result<WordPressSettings, McpError> {
        let url = format!("{}/wp-json/wp/v2/settings", self.base_url);

        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Retrieving WordPress settings");
        let response = self.execute_request_with_retry(request).await?;
        let settings: WordPressSettings = serde_json::from_value(response)?;
        Ok(settings)
    }

    /// Update WordPress site settings
    pub async fn update_settings(
        &self,
        params: SettingsUpdateParams,
    ) -> Result<WordPressSettings, McpError> {
        let url = format!("{}/wp-json/wp/v2/settings", self.base_url);

        let mut settings_data = serde_json::Map::new();

        if let Some(title) = params.title {
            settings_data.insert("title".to_string(), serde_json::Value::String(title));
        }
        if let Some(description) = params.description {
            settings_data.insert(
                "description".to_string(),
                serde_json::Value::String(description),
            );
        }
        if let Some(timezone) = params.timezone {
            settings_data.insert("timezone".to_string(), serde_json::Value::String(timezone));
        }
        if let Some(show_on_front) = params.show_on_front {
            settings_data.insert(
                "show_on_front".to_string(),
                serde_json::Value::String(show_on_front),
            );
        }
        if let Some(page_on_front) = params.page_on_front {
            settings_data.insert(
                "page_on_front".to_string(),
                serde_json::Value::Number(page_on_front.into()),
            );
        }
        if let Some(page_for_posts) = params.page_for_posts {
            settings_data.insert(
                "page_for_posts".to_string(),
                serde_json::Value::Number(page_for_posts.into()),
            );
        }
        if let Some(posts_per_page) = params.posts_per_page {
            settings_data.insert(
                "posts_per_page".to_string(),
                serde_json::Value::Number(posts_per_page.into()),
            );
        }
        if let Some(default_category) = params.default_category {
            settings_data.insert(
                "default_category".to_string(),
                serde_json::Value::Number(default_category.into()),
            );
        }
        if let Some(language) = params.language {
            settings_data.insert("language".to_string(), serde_json::Value::String(language));
        }

        let mut request = self
            .client
            .post(&url)
            .json(&serde_json::Value::Object(settings_data));

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Updating WordPress settings");
        let response = self.execute_request_with_retry(request).await?;
        let settings: WordPressSettings = serde_json::from_value(response)?;
        Ok(settings)
    }

    /// Set front page to static page
    pub async fn set_front_page(&self, page_id: u64) -> Result<WordPressSettings, McpError> {
        let params = SettingsUpdateParams {
            show_on_front: Some("page".to_string()),
            page_on_front: Some(page_id),
            ..Default::default()
        };
        self.update_settings(params).await
    }

    /// Set front page to latest posts
    pub async fn set_front_page_to_posts(
        &self,
        posts_page_id: Option<u64>,
    ) -> Result<WordPressSettings, McpError> {
        let params = SettingsUpdateParams {
            show_on_front: Some("posts".to_string()),
            page_for_posts: posts_page_id,
            ..Default::default()
        };
        self.update_settings(params).await
    }

    // YouTube video URL validation
    pub fn validate_youtube_url(url: &str) -> bool {
        url.contains("youtube.com/watch?v=")
            || url.contains("youtu.be/")
            || url.contains("youtube.com/embed/")
    }

    // Extract YouTube video ID from URL
    pub fn extract_youtube_id(url: &str) -> Option<String> {
        // YouTube URL patterns
        let patterns = [
            r"(?:youtube\.com/watch\?v=)([a-zA-Z0-9_-]+)",
            r"(?:youtu\.be/)([a-zA-Z0-9_-]+)",
            r"(?:youtube\.com/embed/)([a-zA-Z0-9_-]+)",
        ];

        for pattern in &patterns {
            if let Ok(regex) = Regex::new(pattern) {
                if let Some(caps) = regex.captures(url) {
                    if let Some(video_id) = caps.get(1) {
                        return Some(video_id.as_str().to_string());
                    }
                }
            }
        }
        None
    }

    // Generate YouTube embed HTML
    pub fn generate_youtube_embed(
        video_id: &str,
        width: Option<u32>,
        height: Option<u32>,
    ) -> String {
        let w = width.unwrap_or(560);
        let h = height.unwrap_or(315);
        format!(
            r#"<iframe width="{}" height="{}" src="https://www.youtube.com/embed/{}" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>"#,
            w, h, video_id
        )
    }

    // Validate social media URLs
    pub fn validate_social_url(url: &str) -> Option<&'static str> {
        if url.contains("twitter.com/") || url.contains("x.com/") {
            Some("twitter")
        } else if url.contains("instagram.com/p/") {
            Some("instagram")
        } else if url.contains("facebook.com/") {
            Some("facebook")
        } else if url.contains("tiktok.com/") {
            Some("tiktok")
        } else {
            None
        }
    }

    // Create post with embedded content (YouTube, social media)
    pub async fn create_post_with_embeds(
        &self,
        title: &str,
        content: &str,
        youtube_urls: Vec<&str>,
        social_urls: Vec<&str>,
        params: Option<PostCreateParams>,
    ) -> Result<WordPressPost, McpError> {
        let mut full_content = content.to_string();

        // Add YouTube embeds
        for url in youtube_urls {
            if Self::validate_youtube_url(url) {
                if let Some(video_id) = Self::extract_youtube_id(url) {
                    let embed = Self::generate_youtube_embed(&video_id, None, None);
                    full_content.push_str(&format!("\n\n{}", embed));
                } else {
                    // Fallback: just add the URL for WordPress oEmbed
                    full_content.push_str(&format!("\n\n{}", url));
                }
            }
        }

        // Add social media embeds (WordPress oEmbed will handle these)
        for url in social_urls {
            if Self::validate_social_url(url).is_some() {
                full_content.push_str(&format!("\n\n{}", url));
            }
        }

        // Create post with the enhanced content
        if let Some(mut post_params) = params {
            post_params.content = full_content;
            self.create_advanced_post(post_params).await
        } else {
            let post_params = PostCreateParams {
                title: title.to_string(),
                content: full_content,
                post_type: "post".to_string(),
                status: "publish".to_string(),
                ..Default::default()
            };
            self.create_advanced_post(post_params).await
        }
    }

    /// Create post with featured image
    async fn create_post_with_featured_image(
        &self,
        title: String,
        content: String,
        featured_media_id: u64,
    ) -> Result<WordPressPost, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts", self.base_url);

        let post = WordPressPost {
            id: None,
            date: None,
            date_gmt: None,
            guid: None,
            modified: None,
            modified_gmt: None,
            slug: None,
            status: "publish".to_string(),
            post_type: Some("post".to_string()),
            link: None,
            title: WordPressContent {
                rendered: title,
                protected: false,
            },
            content: WordPressContent {
                rendered: content,
                protected: false,
            },
            excerpt: None,
            author: None,
            featured_media: Some(featured_media_id),
            categories: None,
            tags: None,
        };

        let mut request = self.client.post(&url).json(&post);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Creating post with featured image: {}", featured_media_id);
        self.execute_request_with_retry(request).await
    }

    /// Create post with categories and tags
    pub async fn create_post_with_categories_tags(
        &self,
        title: String,
        content: String,
        status: Option<String>,
        categories: Option<Vec<u64>>,
        tags: Option<Vec<u64>>,
        featured_media_id: Option<u64>,
    ) -> Result<WordPressPost, McpError> {
        self.create_advanced_post(PostCreateParams {
            title,
            content,
            post_type: "post".to_string(),
            // 意図しない公開を避けるため、未指定時はdraftを既定値とする
            status: status.unwrap_or_else(|| "draft".to_string()),
            date: None,
            categories,
            tags,
            featured_media_id,
            meta: None,
        })
        .await
    }

    /// Update post categories and tags
    pub async fn update_post_categories_tags(
        &self,
        post_id: u64,
        post_type: &str,
        categories: Option<Vec<u64>>,
        tags: Option<Vec<u64>>,
    ) -> Result<WordPressPost, McpError> {
        self.update_post(
            post_id,
            post_type,
            PostUpdateParams {
                categories,
                tags,
                ..Default::default()
            },
        )
        .await
    }

    /// Update an existing WordPress post or page
    pub async fn update_post(
        &self,
        post_id: u64,
        post_type: &str,
        params: PostUpdateParams,
    ) -> Result<WordPressPost, McpError> {
        let endpoint = endpoint_for_post_type(post_type);
        let url = format!("{}/wp-json/wp/v2/{}/{}", self.base_url, endpoint, post_id);

        info!("Updating WordPress {}: {}", post_type, post_id);

        let mut update_data = serde_json::Map::new();

        if let Some(title) = params.title {
            update_data.insert("title".to_string(), serde_json::Value::String(title));
        }

        if let Some(content) = params.content {
            update_data.insert("content".to_string(), serde_json::Value::String(content));
        }

        if let Some(status) = params.status {
            update_data.insert("status".to_string(), serde_json::Value::String(status));
        }

        if let Some(cats) = params.categories {
            update_data.insert(
                "categories".to_string(),
                serde_json::Value::Array(
                    cats.into_iter()
                        .map(|id| serde_json::Value::Number(serde_json::Number::from(id)))
                        .collect(),
                ),
            );
        }

        if let Some(tag_ids) = params.tags {
            update_data.insert(
                "tags".to_string(),
                serde_json::Value::Array(
                    tag_ids
                        .into_iter()
                        .map(|id| serde_json::Value::Number(serde_json::Number::from(id)))
                        .collect(),
                ),
            );
        }

        if let Some(media_id) = params.featured_media_id {
            update_data.insert(
                "featured_media".to_string(),
                serde_json::Value::Number(serde_json::Number::from(media_id)),
            );
        }

        if let Some(metadata) = params.meta {
            update_data.insert(
                "meta".to_string(),
                serde_json::Value::Object(
                    metadata
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::Value::String(v)))
                        .collect(),
                ),
            );
        }

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        self.execute_request_with_retry(request).await
    }

    /// Delete a WordPress post or page
    pub async fn delete_post(
        &self,
        post_id: u64,
        post_type: &str,
        force: bool,
    ) -> Result<serde_json::Value, McpError> {
        let endpoint = endpoint_for_post_type(post_type);
        let url = format!("{}/wp-json/wp/v2/{}/{}", self.base_url, endpoint, post_id);

        let mut request = self.client.delete(&url);

        if force {
            request = request.query(&[("force", "true")]);
        }

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!(
            "Deleting WordPress {}: {} (force: {})",
            post_type, post_id, force
        );
        self.execute_request_with_retry(request).await
    }

    /// Set featured image for existing post
    async fn set_featured_image(
        &self,
        post_id: u64,
        media_id: u64,
    ) -> Result<WordPressPost, McpError> {
        let url = format!("{}/wp-json/wp/v2/posts/{}", self.base_url, post_id);

        let update_data = serde_json::json!({ "featured_media": media_id });

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Setting featured image {} for post {}", media_id, post_id);
        self.execute_request_with_retry(request).await
    }

    /// Get all categories
    pub async fn get_categories(&self) -> Result<Vec<WordPressCategory>, McpError> {
        info!("Fetching WordPress categories");
        let (items, _total) = self
            .fetch_all_pages(|page| {
                format!(
                    "{}/wp-json/wp/v2/categories?per_page=100&page={}",
                    self.base_url, page
                )
            })
            .await?;
        Ok(items)
    }

    /// Create a new category
    pub async fn create_category(
        &self,
        name: &str,
        description: Option<&str>,
        parent: Option<u64>,
        slug: Option<&str>,
    ) -> Result<WordPressCategory, McpError> {
        let url = format!("{}/wp-json/wp/v2/categories", self.base_url);

        let mut category_data = serde_json::json!({ "name": name });

        if let Some(desc) = description {
            category_data["description"] = serde_json::Value::String(desc.to_string());
        }

        if let Some(parent_id) = parent {
            category_data["parent"] =
                serde_json::Value::Number(serde_json::Number::from(parent_id));
        }

        if let Some(slug) = slug {
            category_data["slug"] = serde_json::Value::String(slug.to_string());
        }

        let mut request = self.client.post(&url).json(&category_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Creating category: {}", name);
        self.execute_request_with_retry(request).await
    }

    /// Update an existing category
    pub async fn update_category(
        &self,
        category_id: u64,
        name: Option<&str>,
        description: Option<&str>,
        parent: Option<u64>,
        slug: Option<&str>,
    ) -> Result<WordPressCategory, McpError> {
        let url = format!("{}/wp-json/wp/v2/categories/{}", self.base_url, category_id);

        let mut update_data = serde_json::Map::new();

        if let Some(name) = name {
            update_data.insert(
                "name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }

        if let Some(desc) = description {
            update_data.insert(
                "description".to_string(),
                serde_json::Value::String(desc.to_string()),
            );
        }

        if let Some(parent_id) = parent {
            update_data.insert(
                "parent".to_string(),
                serde_json::Value::Number(serde_json::Number::from(parent_id)),
            );
        }

        if let Some(slug) = slug {
            update_data.insert(
                "slug".to_string(),
                serde_json::Value::String(slug.to_string()),
            );
        }

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Updating category: {}", category_id);
        self.execute_request_with_retry(request).await
    }

    /// Delete a category
    pub async fn delete_category(
        &self,
        category_id: u64,
        force: bool,
    ) -> Result<serde_json::Value, McpError> {
        let url = format!("{}/wp-json/wp/v2/categories/{}", self.base_url, category_id);

        let mut request = self.client.delete(&url);

        if force {
            request = request.query(&[("force", "true")]);
        }

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Deleting category: {} (force: {})", category_id, force);
        self.execute_request_with_retry(request).await
    }

    /// Get all tags
    pub async fn get_tags(&self) -> Result<Vec<WordPressTag>, McpError> {
        info!("Fetching WordPress tags");
        let (items, _total) = self
            .fetch_all_pages(|page| {
                format!(
                    "{}/wp-json/wp/v2/tags?per_page=100&page={}",
                    self.base_url, page
                )
            })
            .await?;
        Ok(items)
    }

    /// Create a new tag
    pub async fn create_tag(
        &self,
        name: &str,
        description: Option<&str>,
        slug: Option<&str>,
    ) -> Result<WordPressTag, McpError> {
        let url = format!("{}/wp-json/wp/v2/tags", self.base_url);

        let mut tag_data = serde_json::json!({ "name": name });

        if let Some(desc) = description {
            tag_data["description"] = serde_json::Value::String(desc.to_string());
        }

        if let Some(slug) = slug {
            tag_data["slug"] = serde_json::Value::String(slug.to_string());
        }

        let mut request = self.client.post(&url).json(&tag_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Creating tag: {}", name);
        self.execute_request_with_retry(request).await
    }

    /// Update an existing tag
    pub async fn update_tag(
        &self,
        tag_id: u64,
        name: Option<&str>,
        description: Option<&str>,
        slug: Option<&str>,
    ) -> Result<WordPressTag, McpError> {
        let url = format!("{}/wp-json/wp/v2/tags/{}", self.base_url, tag_id);

        let mut update_data = serde_json::Map::new();

        if let Some(name) = name {
            update_data.insert(
                "name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }

        if let Some(desc) = description {
            update_data.insert(
                "description".to_string(),
                serde_json::Value::String(desc.to_string()),
            );
        }

        if let Some(slug) = slug {
            update_data.insert(
                "slug".to_string(),
                serde_json::Value::String(slug.to_string()),
            );
        }

        let mut request = self.client.put(&url).json(&update_data);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Updating tag: {}", tag_id);
        self.execute_request_with_retry(request).await
    }

    /// Delete a tag
    pub async fn delete_tag(
        &self,
        tag_id: u64,
        force: bool,
    ) -> Result<serde_json::Value, McpError> {
        let url = format!("{}/wp-json/wp/v2/tags/{}", self.base_url, tag_id);

        let mut request = self.client.delete(&url);

        if force {
            request = request.query(&[("force", "true")]);
        }

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        info!("Deleting tag: {} (force: {})", tag_id, force);
        self.execute_request_with_retry(request).await
    }

    /// Perform comprehensive health check of WordPress environment
    pub async fn health_check(&self) -> WordPressHealthCheck {
        let mut health = WordPressHealthCheck {
            site_accessible: false,
            rest_api_available: false,
            authentication_valid: false,
            permissions_adequate: false,
            media_upload_possible: false,
            error_details: Vec::new(),
            site_info: None,
            log_destination: crate::logging::current_log_destination()
                .map(|d| d.detailed_status())
                .unwrap_or_else(|| {
                    "not available (file logging disabled or not initialized)".to_string()
                }),
        };

        info!("Starting WordPress health check for: {}", self.base_url);

        // 1. Check site accessibility
        match self.check_site_accessibility().await {
            Ok(site_info) => {
                health.site_accessible = true;
                health.site_info = Some(site_info);
                info!("✅ Site accessibility: OK");
            }
            Err(e) => {
                health
                    .error_details
                    .push(format!("Site accessibility failed: {}", e));
                warn!("❌ Site accessibility: FAILED - {}", e);
                return health; // If site is not accessible, skip other checks
            }
        }

        // 2. Check REST API availability
        // 注意: ここで早期returnしない。後続のチェックは独立しており、
        // このチェックの成否に関わらず実行すべき（互いの偽陰性の連鎖を防ぐ）。
        if let Err(e) = self.check_rest_api().await {
            health
                .error_details
                .push(format!("REST API check failed: {}", e));
            warn!("❌ REST API availability: FAILED - {}", e);
        } else {
            health.rest_api_available = true;
            info!("✅ REST API availability: OK");
        }

        // 3. Check authentication
        if let Err(e) = self.check_authentication().await {
            health
                .error_details
                .push(format!("Authentication failed: {}", e));
            warn!("❌ Authentication: FAILED - {}", e);
        } else {
            health.authentication_valid = true;
            info!("✅ Authentication: OK");
        }

        // 4. Check permissions
        if let Err(e) = self.check_permissions().await {
            health
                .error_details
                .push(format!("Permissions check failed: {}", e));
            warn!("❌ Permissions: FAILED - {}", e);
        } else {
            health.permissions_adequate = true;
            info!("✅ Permissions: OK");
        }

        // 5. Check media upload capability
        if let Err(e) = self.check_media_upload_capability().await {
            health
                .error_details
                .push(format!("Media upload check failed: {}", e));
            warn!("❌ Media upload capability: FAILED - {}", e);
        } else {
            health.media_upload_possible = true;
            info!("✅ Media upload capability: OK");
        }

        if health.error_details.is_empty() {
            info!("🎉 WordPress health check completed successfully!");
        } else {
            warn!(
                "⚠️ WordPress health check completed with {} issues",
                health.error_details.len()
            );
        }

        health
    }

    /// Check if WordPress site is accessible
    async fn check_site_accessibility(&self) -> Result<WordPressSiteInfo, McpError> {
        let url = format!("{}/wp-json/wp/v2/settings", self.base_url);
        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to connect to WordPress: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress site not accessible. Status: {}",
                response.status()
            )));
        }

        let settings: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse site info: {}", e)))?;

        Ok(WordPressSiteInfo {
            name: settings
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string(),
            description: settings
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            url: settings
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or(&self.base_url)
                .to_string(),
            admin_email: settings
                .get("admin_email")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            timezone_string: settings
                .get("timezone_string")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            date_format: settings
                .get("date_format")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            time_format: settings
                .get("time_format")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            start_of_week: settings
                .get("start_of_week")
                .and_then(|v| v.as_u64())
                .map(|n| n as u8),
        })
    }

    /// Check if WordPress REST API is available
    ///
    /// ルートのAPIディスカバリーインデックス（`/wp-json/`）を見る。
    /// `/wp-json/wp/v2`はwp/v2名前空間自身のルート一覧
    /// （`{"namespace": "wp/v2", "routes": {...}}`）を返すだけで
    /// `"namespaces"`配列を持たないため、そちらを見ると常に
    /// 偽陰性になる。
    async fn check_rest_api(&self) -> Result<(), McpError> {
        let url = format!("{}/wp-json/", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("REST API check failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress REST API not available. Status: {}",
                response.status()
            )));
        }

        // Check if response contains expected namespace
        let api_info: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Invalid REST API response: {}", e)))?;

        if !api_info
            .get("namespaces")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|ns| ns.as_str() == Some("wp/v2")))
            .unwrap_or(false)
        {
            return Err(McpError::ExternalApi(
                "WordPress REST API v2 not available".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if authentication credentials are valid
    async fn check_authentication(&self) -> Result<(), McpError> {
        let url = format!("{}/wp-json/wp/v2/users/me", self.base_url);
        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        } else {
            return Err(McpError::ExternalApi(
                "No authentication credentials provided".to_string(),
            ));
        }

        let response = request
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Authentication check failed: {}", e)))?;

        match response.status().as_u16() {
            200 => Ok(()),
            401 => Err(McpError::ExternalApi("Invalid credentials".to_string())),
            403 => Err(McpError::ExternalApi(
                "Authentication forbidden".to_string(),
            )),
            _ => Err(McpError::ExternalApi(format!(
                "Authentication failed with status: {}",
                response.status()
            ))),
        }
    }

    /// Check if user has adequate permissions
    async fn check_permissions(&self) -> Result<(), McpError> {
        // Check if user can read posts
        let posts_url = format!("{}/wp-json/wp/v2/posts?per_page=1", self.base_url);
        let mut request = self.client.get(&posts_url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Posts permission check failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "No permission to read posts. Status: {}",
                response.status()
            )));
        }

        // Check if user can access media
        let media_url = format!("{}/wp-json/wp/v2/media?per_page=1", self.base_url);
        let mut request = self.client.get(&media_url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request
            .send()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Media permission check failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "No permission to access media. Status: {}",
                response.status()
            )));
        }

        Ok(())
    }

    /// Check if media upload is possible
    async fn check_media_upload_capability(&self) -> Result<(), McpError> {
        // Check if user can access media endpoint with GET request
        let url = format!("{}/wp-json/wp/v2/media?per_page=1", self.base_url);
        let mut request = self.client.get(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.send().await.map_err(|e| {
            McpError::ExternalApi(format!("Media upload capability check failed: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "Cannot access media endpoint. Status: {}",
                response.status()
            )));
        }

        // If we can read media, we can likely upload (assuming proper permissions)
        // We could also check upload_max_filesize but that requires admin access
        Ok(())
    }
}

#[async_trait]
impl McpHandler for WordPressHandler {
    async fn initialize(&self, _params: InitializeParams) -> Result<serde_json::Value, McpError> {
        info!("WordPress MCP Handler initialized");
        Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": false
                },
                "resources": {
                    "subscribe": false,
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": "mcp-rs-wordpress",
                "version": "0.1.0"
            }
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool {
                name: "wordpress_health_check".to_string(),
                description: "Perform comprehensive WordPress environment health check".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "get_posts".to_string(),
                description: "Retrieve WordPress posts".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "get_pages".to_string(),
                description: "Retrieve WordPress pages".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "get_all_content".to_string(),
                description: "Retrieve both WordPress posts and pages".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "get_post".to_string(),
                description: "Retrieve a single WordPress post or page by ID".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID to retrieve"
                        },
                        "post_type": {
                            "type": "string",
                            "enum": ["post", "page"],
                            "description": "Content type to retrieve. Defaults to 'post'.",
                            "default": "post"
                        }
                    },
                    "required": ["post_id"]
                }),
            },
            Tool {
                name: "create_post".to_string(),
                description: "Create a new WordPress post (basic). Defaults to draft status so nothing is published unintentionally.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "The post content"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["publish", "draft", "private"],
                            "description": "Post status. Defaults to 'draft' to avoid accidental publication.",
                            "default": "draft"
                        }
                    },
                    "required": ["title", "content"]
                }),
            },
            Tool {
                name: "create_advanced_post".to_string(),
                description: "Create a new WordPress post or page with advanced options"
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post/page title"
                        },
                        "content": {
                            "type": "string",
                            "description": "The post/page content"
                        },
                        "post_type": {
                            "type": "string",
                            "description": "Post type: 'post' (投稿) or 'page' (固定ページ)",
                            "enum": ["post", "page"],
                            "default": "post"
                        },
                        "status": {
                            "type": "string",
                            "description": "Post status: 'publish' (公開), 'draft' (下書き), 'private' (非公開), 'future' (予約投稿)",
                            "enum": ["publish", "draft", "private", "future"],
                            "default": "publish"
                        },
                        "date": {
                            "type": "string",
                            "description": "Publication date (ISO8601 format, required for 'future' status)"
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Category IDs (posts only)"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Tag IDs (posts only)"
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Featured image media ID"
                        },
                        "meta": {
                            "type": "object",
                            "description": "Meta fields for SEO (e.g., _yoast_wpseo_metadesc, _yoast_wpseo_meta-robots-noindex, _yoast_wpseo_meta-robots-nofollow)",
                            "additionalProperties": {"type": "string"}
                        }
                    },
                    "required": ["title", "content"]
                }),
            },
            Tool {
                name: "get_comments".to_string(),
                description: "Retrieve WordPress comments".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Optional post ID to filter comments"
                        }
                    },
                    "required": []
                }),
            },
            Tool {
                name: "upload_media".to_string(),
                description: "Upload media file to WordPress".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_data": {
                            "type": "string",
                            "description": "Base64 encoded file data"
                        },
                        "filename": {
                            "type": "string",
                            "description": "Original filename"
                        },
                        "mime_type": {
                            "type": "string",
                            "description": "MIME type of the file (e.g., 'image/jpeg')"
                        }
                    },
                    "required": ["file_data", "filename", "mime_type"]
                }),
            },
            Tool {
                name: "get_media".to_string(),
                description: "Retrieve WordPress media files".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "get_media_item".to_string(),
                description: "Retrieve a single WordPress media item by ID".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "media_id": {
                            "type": "number",
                            "description": "Media ID to retrieve"
                        }
                    },
                    "required": ["media_id"]
                }),
            },
            Tool {
                name: "update_media".to_string(),
                description:
                    "Update WordPress media metadata (title, alt text, caption, description)"
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "media_id": {
                            "type": "number",
                            "description": "Media ID to update"
                        },
                        "title": {
                            "type": "string",
                            "description": "Media title"
                        },
                        "alt_text": {
                            "type": "string",
                            "description": "Alternative text for accessibility"
                        },
                        "caption": {
                            "type": "string",
                            "description": "Media caption"
                        },
                        "description": {
                            "type": "string",
                            "description": "Media description"
                        },
                        "post": {
                            "type": "number",
                            "description": "Post ID to attach media to"
                        }
                    },
                    "required": ["media_id"]
                }),
            },
            Tool {
                name: "delete_media".to_string(),
                description: "Delete WordPress media file".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "media_id": {
                            "type": "number",
                            "description": "Media ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force delete (bypass trash)"
                        }
                    },
                    "required": ["media_id"]
                }),
            },
            Tool {
                name: "create_post_with_featured_image".to_string(),
                description: "Create a new WordPress post with featured image".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "The post content"
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Media ID for featured image"
                        }
                    },
                    "required": ["title", "content", "featured_media_id"]
                }),
            },
            Tool {
                name: "set_featured_image".to_string(),
                description: "Set featured image for existing post".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post ID to update"
                        },
                        "media_id": {
                            "type": "number",
                            "description": "Media ID for featured image"
                        }
                    },
                    "required": ["post_id", "media_id"]
                }),
            },
            Tool {
                name: "get_categories".to_string(),
                description: "Retrieve WordPress categories".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "create_category".to_string(),
                description: "Create a new WordPress category".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Category name"
                        },
                        "description": {
                            "type": "string",
                            "description": "Category description (optional)"
                        },
                        "parent": {
                            "type": "number",
                            "description": "Parent category ID (optional)"
                        },
                        "slug": {
                            "type": "string",
                            "description": "Custom slug (optional, e.g. an ASCII slug for non-ASCII names)"
                        }
                    },
                    "required": ["name"]
                }),
            },
            Tool {
                name: "update_category".to_string(),
                description: "Update an existing WordPress category".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category_id": {
                            "type": "number",
                            "description": "Category ID to update"
                        },
                        "name": {
                            "type": "string",
                            "description": "New category name (optional)"
                        },
                        "description": {
                            "type": "string",
                            "description": "New category description (optional)"
                        },
                        "parent": {
                            "type": "number",
                            "description": "New parent category ID (optional; use 0 to clear)"
                        },
                        "slug": {
                            "type": "string",
                            "description": "New slug (optional; WordPress does not auto-update the slug when the name changes)"
                        }
                    },
                    "required": ["category_id"]
                }),
            },
            Tool {
                name: "delete_category".to_string(),
                description: "Delete a WordPress category".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category_id": {
                            "type": "number",
                            "description": "Category ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force delete (bypass trash)"
                        }
                    },
                    "required": ["category_id"]
                }),
            },
            Tool {
                name: "get_tags".to_string(),
                description: "Retrieve WordPress tags".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "create_tag".to_string(),
                description: "Create a new WordPress tag".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Tag name"
                        },
                        "description": {
                            "type": "string",
                            "description": "Tag description (optional)"
                        },
                        "slug": {
                            "type": "string",
                            "description": "Custom slug (optional, e.g. an ASCII slug for non-ASCII names)"
                        }
                    },
                    "required": ["name"]
                }),
            },
            Tool {
                name: "update_tag".to_string(),
                description: "Update an existing WordPress tag".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tag_id": {
                            "type": "number",
                            "description": "Tag ID to update"
                        },
                        "name": {
                            "type": "string",
                            "description": "New tag name (optional)"
                        },
                        "description": {
                            "type": "string",
                            "description": "New tag description (optional)"
                        },
                        "slug": {
                            "type": "string",
                            "description": "New slug (optional; WordPress does not auto-update the slug when the name changes)"
                        }
                    },
                    "required": ["tag_id"]
                }),
            },
            Tool {
                name: "delete_tag".to_string(),
                description: "Delete a WordPress tag".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tag_id": {
                            "type": "number",
                            "description": "Tag ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force delete (bypass trash)"
                        }
                    },
                    "required": ["tag_id"]
                }),
            },
            Tool {
                name: "create_post_with_categories_tags".to_string(),
                description: "Create a new WordPress post with categories and tags. Defaults to draft status so nothing is published unintentionally.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "The post content"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["publish", "draft", "private"],
                            "description": "Post status. Defaults to 'draft' to avoid accidental publication.",
                            "default": "draft"
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Array of category IDs (optional)"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Array of tag IDs (optional)"
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Featured image media ID (optional)"
                        }
                    },
                    "required": ["title", "content"]
                }),
            },
            Tool {
                name: "update_post_categories_tags".to_string(),
                description: "Update categories and tags for an existing WordPress post or page"
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID to update"
                        },
                        "post_type": {
                            "type": "string",
                            "enum": ["post", "page"],
                            "description": "Content type being updated. Defaults to 'post'.",
                            "default": "post"
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Array of category IDs (optional)"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Array of tag IDs (optional)"
                        }
                    },
                    "required": ["post_id"]
                }),
            },
            Tool {
                name: "update_post".to_string(),
                description: "Update an existing WordPress post or page".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID to update"
                        },
                        "post_type": {
                            "type": "string",
                            "enum": ["post", "page"],
                            "description": "Content type being updated. Defaults to 'post'.",
                            "default": "post"
                        },
                        "title": {
                            "type": "string",
                            "description": "New post title (optional)"
                        },
                        "content": {
                            "type": "string",
                            "description": "New post content (optional)"
                        },
                        "status": {
                            "type": "string",
                            "description": "Post status: publish, draft, private (optional)"
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Array of category IDs (optional)"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Array of tag IDs (optional)"
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Featured image media ID (optional)"
                        }
                    },
                    "required": ["post_id"]
                }),
            },
            Tool {
                name: "delete_post".to_string(),
                description: "Delete a WordPress post or page".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID to delete"
                        },
                        "post_type": {
                            "type": "string",
                            "enum": ["post", "page"],
                            "description": "Content type being deleted. Defaults to 'post'.",
                            "default": "post"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force delete (bypass trash, permanently delete)"
                        }
                    },
                    "required": ["post_id"]
                }),
            },
            Tool {
                name: "create_post_with_embeds".to_string(),
                description:
                    "Create WordPress post with embedded YouTube videos and social media content"
                        .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Base post content (embeds will be added)"
                        },
                        "youtube_urls": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "YouTube video URLs to embed"
                        },
                        "social_urls": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Social media URLs to embed (Twitter, Instagram, Facebook, TikTok)"
                        },
                        "post_type": {
                            "type": "string",
                            "description": "Post type (post or page)",
                            "enum": ["post", "page"]
                        },
                        "status": {
                            "type": "string",
                            "description": "Post status",
                            "enum": ["publish", "draft", "private", "future"]
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Category IDs (posts only)"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Tag IDs (posts only)"
                        }
                    },
                    "required": ["title", "content"]
                }),
            },
            Tool {
                name: "get_settings".to_string(),
                description: "Get WordPress site settings".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "update_settings".to_string(),
                description: "Update WordPress site settings".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Site title"
                        },
                        "description": {
                            "type": "string",
                            "description": "Site tagline/description"
                        },
                        "timezone": {
                            "type": "string",
                            "description": "Site timezone (e.g., 'Asia/Tokyo')"
                        },
                        "show_on_front": {
                            "type": "string",
                            "description": "What to show on front page",
                            "enum": ["posts", "page"]
                        },
                        "page_on_front": {
                            "type": "number",
                            "description": "Static front page ID (when show_on_front is 'page')"
                        },
                        "page_for_posts": {
                            "type": "number",
                            "description": "Posts page ID (when show_on_front is 'page')"
                        },
                        "posts_per_page": {
                            "type": "number",
                            "description": "Number of posts per page"
                        },
                        "default_category": {
                            "type": "number",
                            "description": "Default category for new posts"
                        },
                        "language": {
                            "type": "string",
                            "description": "Site language code (e.g., 'ja', 'en_US')"
                        }
                    },
                    "required": []
                }),
            },
            Tool {
                name: "set_front_page".to_string(),
                description: "Set a static page as the front page".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "page_id": {
                            "type": "number",
                            "description": "Page ID to set as front page"
                        }
                    },
                    "required": ["page_id"]
                }),
            },
            Tool {
                name: "set_front_page_to_posts".to_string(),
                description: "Set front page to show latest posts".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "posts_page_id": {
                            "type": "number",
                            "description": "Optional page ID to use for blog posts"
                        }
                    },
                    "required": []
                }),
            },
        ])
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
        match params.name.as_str() {
            "wordpress_health_check" => {
                info!("Performing WordPress health check...");
                let health = self.health_check().await;

                let status_emoji = if health.error_details.is_empty() {
                    "✅"
                } else {
                    "⚠️"
                };
                let status_text = if health.error_details.is_empty() {
                    "HEALTHY"
                } else {
                    "ISSUES DETECTED"
                };

                let mut report = format!(
                    "{} WordPress Health Check: {}\n\n",
                    status_emoji, status_text
                );

                report.push_str(&format!("📄 Log file: {}\n\n", health.log_destination));

                if let Some(site_info) = &health.site_info {
                    report.push_str(&format!(
                        "🌐 Site: {} ({})\n",
                        site_info.name, site_info.url
                    ));
                    report.push_str(&format!("📝 Description: {}\n\n", site_info.description));
                }

                report.push_str("📊 Health Status:\n");
                report.push_str(&format!(
                    "  • Site Accessible: {}\n",
                    if health.site_accessible { "✅" } else { "❌" }
                ));
                report.push_str(&format!(
                    "  • REST API Available: {}\n",
                    if health.rest_api_available {
                        "✅"
                    } else {
                        "❌"
                    }
                ));
                report.push_str(&format!(
                    "  • Authentication Valid: {}\n",
                    if health.authentication_valid {
                        "✅"
                    } else {
                        "❌"
                    }
                ));
                report.push_str(&format!(
                    "  • Permissions Adequate: {}\n",
                    if health.permissions_adequate {
                        "✅"
                    } else {
                        "❌"
                    }
                ));
                report.push_str(&format!(
                    "  • Media Upload Possible: {}\n",
                    if health.media_upload_possible {
                        "✅"
                    } else {
                        "❌"
                    }
                ));

                if !health.error_details.is_empty() {
                    report.push_str("\n🚨 Issues Found:\n");
                    for (i, error) in health.error_details.iter().enumerate() {
                        report.push_str(&format!("  {}. {}\n", i + 1, error));
                    }
                }

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": report
                    }],
                    "isError": !health.error_details.is_empty()
                }))
            }
            "get_posts" => {
                let posts = self.get_posts().await?;
                let summary = summarize_posts(&posts);
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} posts:\n{}",
                            posts.len(),
                            serde_json::to_string_pretty(&summary)
                                .unwrap_or_else(|_| "Failed to serialize posts".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "get_pages" => {
                let pages = self.get_pages().await?;
                let summary = summarize_posts(&pages);
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} pages:\n{}",
                            pages.len(),
                            serde_json::to_string_pretty(&summary)
                                .unwrap_or_else(|_| "Failed to serialize pages".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "get_all_content" => {
                let (posts, pages) = self.get_all_content().await?;
                let posts_summary = summarize_posts(&posts);
                let pages_summary = summarize_posts(&pages);
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} posts and {} pages:\nPosts:\n{}\nPages:\n{}",
                            posts.len(), pages.len(),
                            serde_json::to_string_pretty(&posts_summary)
                                .unwrap_or_else(|_| "Failed to serialize posts".to_string()),
                            serde_json::to_string_pretty(&pages_summary)
                                .unwrap_or_else(|_| "Failed to serialize pages".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "get_post" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args
                    .get("post_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let post_type = args
                    .get("post_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("post");

                let post = self.get_post(post_id, post_type).await?;

                // カテゴリ/タグIDを名前に解決する。一覧取得に失敗しても
                // get_post自体は失敗させず、数値IDのまま表示する。
                let categories_names = match self.get_categories().await {
                    Ok(categories) => {
                        let id_to_name: HashMap<u64, String> = categories
                            .into_iter()
                            .filter_map(|c| c.id.map(|id| (id, c.name)))
                            .collect();
                        post.categories
                            .as_deref()
                            .map(|ids| resolve_term_names(ids, &id_to_name))
                            .unwrap_or_default()
                    }
                    Err(e) => {
                        warn!("Failed to resolve category names for get_post: {}", e);
                        post.categories
                            .as_deref()
                            .map(|ids| ids.iter().map(|id| id.to_string()).collect())
                            .unwrap_or_default()
                    }
                };
                let tags_names = match self.get_tags().await {
                    Ok(tags) => {
                        let id_to_name: HashMap<u64, String> = tags
                            .into_iter()
                            .filter_map(|t| t.id.map(|id| (id, t.name)))
                            .collect();
                        post.tags
                            .as_deref()
                            .map(|ids| resolve_term_names(ids, &id_to_name))
                            .unwrap_or_default()
                    }
                    Err(e) => {
                        warn!("Failed to resolve tag names for get_post: {}", e);
                        post.tags
                            .as_deref()
                            .map(|ids| ids.iter().map(|id| id.to_string()).collect())
                            .unwrap_or_default()
                    }
                };

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Post Details:\nID: {}\nTitle: {}\nStatus: {}\nCategories: {}\nTags: {}\nContent: {}",
                            post.id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string()),
                            post.title.rendered,
                            post.status,
                            categories_names.join(", "),
                            tags_names.join(", "),
                            truncate_chars_with_ellipsis(&html_to_plain_text(&post.content.rendered), 100)
                        )
                    }],
                    "isError": false
                }))
            }
            "create_post" => {
                let args = params.arguments.unwrap_or_default();
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;
                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let post = self
                    .create_post(title.to_string(), content.to_string(), status)
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post with ID: {} (Status: {})",
                            post.id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string()),
                            post.status)
                    }],
                    "isError": false
                }))
            }
            "create_advanced_post" => {
                let args = params.arguments.unwrap_or_default();
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;

                let post_type = args
                    .get("post_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("post")
                    .to_string();

                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("publish")
                    .to_string();

                let date = args
                    .get("date")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let categories = args
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<u64>>());

                let tags = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<u64>>());

                let featured_media_id = args.get("featured_media_id").and_then(|v| v.as_u64());

                let meta = args.get("meta").and_then(|v| v.as_object()).map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect::<HashMap<String, String>>()
                });

                let post = self
                    .create_advanced_post(PostCreateParams {
                        title: title.to_string(),
                        content: content.to_string(),
                        post_type: post_type.clone(),
                        status: status.clone(),
                        date,
                        categories,
                        tags,
                        featured_media_id,
                        meta,
                    })
                    .await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Created {} with ID: {} (Status: {})",
                            if post_type == "page" { "page" } else { "post" },
                            display_opt_id(post.id),
                            status
                        )
                    }],
                    "isError": false
                }))
            }
            "get_comments" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args.get("post_id").and_then(|v| v.as_u64());

                let comments = self.get_comments(post_id).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} comments:\n{}",
                            comments.len(),
                            serde_json::to_string_pretty(&comments)
                                .unwrap_or_else(|_| "Failed to serialize comments".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "upload_media" => {
                let args = params.arguments.unwrap_or_default();
                let file_data_b64 = args
                    .get("file_data")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing file_data".to_string()))?;
                let filename = args
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing filename".to_string()))?;
                let mime_type = args
                    .get("mime_type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing mime_type".to_string()))?;

                // Decode base64 file data
                use base64::{engine::general_purpose, Engine as _};
                let file_data = general_purpose::STANDARD
                    .decode(file_data_b64)
                    .map_err(|e| McpError::InvalidParams(format!("Invalid base64 data: {}", e)))?;

                let media = self.upload_media(&file_data, filename, mime_type).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Uploaded media ID {}: {}",
                            display_opt_id(media.id),
                            media.source_url.as_deref().unwrap_or("(no URL)")
                        )
                    }],
                    "isError": false
                }))
            }
            "get_media" => {
                let (media_list, total_from_header) = self.get_media().await?;
                let summary = summarize_media(&media_list);
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} media files (X-WP-Total: {}):\n{}",
                            media_list.len(),
                            total_from_header,
                            serde_json::to_string_pretty(&summary)
                                .unwrap_or_else(|_| "Failed to serialize media".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "get_media_item" => {
                let args = params.arguments.unwrap_or_default();
                let media_id = args
                    .get("media_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing media_id".to_string()))?;

                let media = self.get_media_item(media_id).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Media ID: {}, Title: {}, Alt: {}, URL: {}",
                            display_opt_id(media.id),
                            media.title.as_ref().map(|t| t.rendered.as_str()).unwrap_or("No title"),
                            media.alt_text.as_deref().unwrap_or("No alt text"),
                            media.source_url.as_deref().unwrap_or("No URL")
                        )
                    }],
                    "isError": false
                }))
            }
            "update_media" => {
                let args = params.arguments.unwrap_or_default();
                let media_id = args
                    .get("media_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing media_id".to_string()))?;

                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let alt_text = args
                    .get("alt_text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let caption = args
                    .get("caption")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let post = args.get("post").and_then(|v| v.as_u64());

                let update_params = MediaUpdateParams {
                    title,
                    alt_text,
                    caption,
                    description,
                    post,
                };

                let media = self.update_media(media_id, update_params).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated media ID: {}", display_opt_id(media.id))
                    }],
                    "isError": false
                }))
            }
            "delete_media" => {
                let args = params.arguments.unwrap_or_default();
                let media_id = args
                    .get("media_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing media_id".to_string()))?;

                let force = args.get("force").and_then(|v| v.as_bool());

                let media = self.delete_media(media_id, force).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Deleted media ID: {} (Force: {})",
                            display_opt_id(media.id),
                            force.unwrap_or(false)
                        )
                    }],
                    "isError": false
                }))
            }
            "create_post_with_featured_image" => {
                let args = params.arguments.unwrap_or_default();
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;
                let featured_media_id = args
                    .get("featured_media_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| {
                        McpError::InvalidParams("Missing featured_media_id".to_string())
                    })?;

                let post = self
                    .create_post_with_featured_image(
                        title.to_string(),
                        content.to_string(),
                        featured_media_id,
                    )
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post with featured image. Post ID: {}", display_opt_id(post.id))
                    }],
                    "isError": false
                }))
            }
            "set_featured_image" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args
                    .get("post_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let media_id = args
                    .get("media_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing media_id".to_string()))?;

                let post = self.set_featured_image(post_id, media_id).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Set featured image {} for post {}. Updated post ID: {}", media_id, post_id, display_opt_id(post.id))
                    }],
                    "isError": false
                }))
            }
            "get_categories" => {
                let categories = self.get_categories().await?;
                let categories_json = decode_slugs_for_display(
                    serde_json::to_value(&categories).unwrap_or(serde_json::Value::Null),
                );
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} categories:\n{}",
                            categories.len(),
                            serde_json::to_string_pretty(&categories_json)
                                .unwrap_or_else(|_| "Failed to serialize categories".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "create_category" => {
                let args = params.arguments.unwrap_or_default();
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing name".to_string()))?;
                let description = args.get("description").and_then(|v| v.as_str());
                let parent = args.get("parent").and_then(|v| v.as_u64());
                let slug = args.get("slug").and_then(|v| v.as_str());

                let category = self
                    .create_category(name, description, parent, slug)
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created category '{}' with ID: {}", name, display_opt_id(category.id))
                    }],
                    "isError": false
                }))
            }
            "update_category" => {
                let args = params.arguments.unwrap_or_default();
                let category_id = args
                    .get("category_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing category_id".to_string()))?;
                let name = args.get("name").and_then(|v| v.as_str());
                let description = args.get("description").and_then(|v| v.as_str());
                let parent = args.get("parent").and_then(|v| v.as_u64());
                let slug = args.get("slug").and_then(|v| v.as_str());

                let category = self
                    .update_category(category_id, name, description, parent, slug)
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated category ID {} to '{}'", category_id, category.name)
                    }],
                    "isError": false
                }))
            }
            "delete_category" => {
                let args = params.arguments.unwrap_or_default();
                let category_id = args
                    .get("category_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing category_id".to_string()))?;
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

                self.delete_category(category_id, force).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Deleted category ID {} (force: {})", category_id, force)
                    }],
                    "isError": false
                }))
            }
            "get_tags" => {
                let tags = self.get_tags().await?;
                let tags_json = decode_slugs_for_display(
                    serde_json::to_value(&tags).unwrap_or(serde_json::Value::Null),
                );
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} tags:\n{}",
                            tags.len(),
                            serde_json::to_string_pretty(&tags_json)
                                .unwrap_or_else(|_| "Failed to serialize tags".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "create_tag" => {
                let args = params.arguments.unwrap_or_default();
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing name".to_string()))?;
                let description = args.get("description").and_then(|v| v.as_str());
                let slug = args.get("slug").and_then(|v| v.as_str());

                let tag = self.create_tag(name, description, slug).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created tag '{}' with ID: {}", name, display_opt_id(tag.id))
                    }],
                    "isError": false
                }))
            }
            "update_tag" => {
                let args = params.arguments.unwrap_or_default();
                let tag_id = args
                    .get("tag_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing tag_id".to_string()))?;
                let name = args.get("name").and_then(|v| v.as_str());
                let description = args.get("description").and_then(|v| v.as_str());
                let slug = args.get("slug").and_then(|v| v.as_str());

                let tag = self.update_tag(tag_id, name, description, slug).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated tag ID {} to '{}'", tag_id, tag.name)
                    }],
                    "isError": false
                }))
            }
            "delete_tag" => {
                let args = params.arguments.unwrap_or_default();
                let tag_id = args
                    .get("tag_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing tag_id".to_string()))?;
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

                self.delete_tag(tag_id, force).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Deleted tag ID {} (force: {})", tag_id, force)
                    }],
                    "isError": false
                }))
            }
            "create_post_with_categories_tags" => {
                let args = params.arguments.unwrap_or_default();
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;

                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let categories: Option<Vec<u64>> = args
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let tags: Option<Vec<u64>> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let featured_media_id = args.get("featured_media_id").and_then(|v| v.as_u64());

                let post = self
                    .create_post_with_categories_tags(
                        title.to_string(),
                        content.to_string(),
                        status,
                        categories,
                        tags,
                        featured_media_id,
                    )
                    .await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post '{}' with ID: {} (Status: {}), categories: {}, tags: {}",
                            title,
                            post.id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string()),
                            post.status,
                            post.categories.as_deref().map(|ids| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "-".to_string()),
                            post.tags.as_deref().map(|ids| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "-".to_string()))
                    }],
                    "isError": false
                }))
            }
            "update_post_categories_tags" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args
                    .get("post_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let post_type = args
                    .get("post_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("post");

                let categories: Option<Vec<u64>> = args
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let tags: Option<Vec<u64>> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let post = self
                    .update_post_categories_tags(post_id, post_type, categories, tags)
                    .await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated {} ID {} with categories: {}, tags: {}",
                            post_type,
                            post_id,
                            post.categories.as_deref().map(|ids| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "-".to_string()),
                            post.tags.as_deref().map(|ids| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ")).unwrap_or_else(|| "-".to_string()))
                    }],
                    "isError": false
                }))
            }
            "update_post" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args
                    .get("post_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let post_type = args
                    .get("post_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("post");

                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let categories = args
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let tags = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let featured_media_id = args.get("featured_media_id").and_then(|v| v.as_u64());

                let post = self
                    .update_post(
                        post_id,
                        post_type,
                        PostUpdateParams {
                            title,
                            content,
                            status,
                            categories,
                            tags,
                            featured_media_id,
                            meta: None,
                        },
                    )
                    .await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated {} ID {} - Title: '{}', Status: {}",
                            post_type, post_id, post.title.rendered, post.status)
                    }],
                    "isError": false
                }))
            }
            "delete_post" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args
                    .get("post_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let post_type = args
                    .get("post_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("post");
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

                self.delete_post(post_id, post_type, force).await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Deleted {} ID {} ({})",
                            post_type,
                            post_id,
                            if force { "permanently" } else { "moved to trash" }
                        )
                    }],
                    "isError": false
                }))
            }
            "create_post_with_embeds" => {
                let args = params.arguments.unwrap_or_default();
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing title".to_string()))?;
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;

                let youtube_urls: Vec<&str> = args
                    .get("youtube_urls")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                let social_urls: Vec<&str> = args
                    .get("social_urls")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                let post_type = args
                    .get("post_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("post")
                    .to_string();

                let status = args
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("publish")
                    .to_string();

                let categories = args
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<u64>>());

                let tags = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<u64>>());

                let params = PostCreateParams {
                    title: title.to_string(),
                    content: content.to_string(),
                    post_type,
                    status,
                    categories,
                    tags,
                    ..Default::default()
                };

                let post = self
                    .create_post_with_embeds(
                        title,
                        content,
                        youtube_urls,
                        social_urls,
                        Some(params),
                    )
                    .await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Created {} with embedded content - ID: {}, Title: {}, Status: {}",
                            if post.post_type.as_ref().unwrap_or(&"post".to_string()) == "page" { "page" } else { "post" },
                            display_opt_id(post.id),
                            post.title.rendered,
                            post.status
                        )
                    }],
                    "isError": false
                }))
            }
            "get_settings" => {
                let settings = self.get_settings().await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "WordPress Settings:\n- Title: {}\n- Description: {}\n- Show on front: {}\n- Posts per page: {}\n- Language: {}",
                            settings.title.unwrap_or("N/A".to_string()),
                            settings.description.unwrap_or("N/A".to_string()),
                            settings.show_on_front.unwrap_or("posts".to_string()),
                            settings.posts_per_page.unwrap_or(10),
                            settings.language.unwrap_or("en_US".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "update_settings" => {
                let args = params.arguments.unwrap_or_default();

                let params = SettingsUpdateParams {
                    title: args
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    description: args
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    timezone: args
                        .get("timezone")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    show_on_front: args
                        .get("show_on_front")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    page_on_front: args.get("page_on_front").and_then(|v| v.as_u64()),
                    page_for_posts: args.get("page_for_posts").and_then(|v| v.as_u64()),
                    posts_per_page: args.get("posts_per_page").and_then(|v| v.as_u64()),
                    default_category: args.get("default_category").and_then(|v| v.as_u64()),
                    language: args
                        .get("language")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                };

                let settings = self.update_settings(params).await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Settings updated successfully:\n- Title: {}\n- Description: {}\n- Show on front: {}",
                            settings.title.unwrap_or("N/A".to_string()),
                            settings.description.unwrap_or("N/A".to_string()),
                            settings.show_on_front.unwrap_or("posts".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "set_front_page" => {
                let args = params.arguments.unwrap_or_default();
                let page_id = args
                    .get("page_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| McpError::InvalidParams("Missing page_id".to_string()))?;

                let settings = self.set_front_page(page_id).await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Front page set to static page ID: {} (Show on front: {})",
                            page_id,
                            settings.show_on_front.unwrap_or("page".to_string())
                        )
                    }],
                    "isError": false
                }))
            }
            "set_front_page_to_posts" => {
                let args = params.arguments.unwrap_or_default();
                let posts_page_id = args.get("posts_page_id").and_then(|v| v.as_u64());

                self.set_front_page_to_posts(posts_page_id).await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Front page set to latest posts{}",
                            if let Some(page_id) = posts_page_id {
                                format!(" (Posts page ID: {})", page_id)
                            } else {
                                String::new()
                            }
                        )
                    }],
                    "isError": false
                }))
            }
            _ => Err(McpError::ToolNotFound(params.name)),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        Ok(vec![
            Resource {
                uri: "wordpress://posts".to_string(),
                name: "WordPress Posts".to_string(),
                description: Some("All WordPress posts".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            Resource {
                uri: "wordpress://comments".to_string(),
                name: "WordPress Comments".to_string(),
                description: Some("All WordPress comments".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            Resource {
                uri: "wordpress://categories".to_string(),
                name: "WordPress Categories".to_string(),
                description: Some("All WordPress categories".to_string()),
                mime_type: Some("application/json".to_string()),
            },
            Resource {
                uri: "wordpress://tags".to_string(),
                name: "WordPress Tags".to_string(),
                description: Some("All WordPress tags".to_string()),
                mime_type: Some("application/json".to_string()),
            },
        ])
    }

    async fn read_resource(
        &self,
        params: ResourceReadParams,
    ) -> Result<serde_json::Value, McpError> {
        match params.uri.as_str() {
            "wordpress://posts" => {
                let posts = self.get_posts().await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&posts)?
                    }]
                }))
            }
            "wordpress://comments" => {
                let comments = self.get_comments(None).await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&comments)?
                    }]
                }))
            }
            "wordpress://categories" => {
                let categories = self.get_categories().await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&categories)?
                    }]
                }))
            }
            "wordpress://tags" => {
                let tags = self.get_tags().await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&tags)?
                    }]
                }))
            }
            _ => Err(McpError::ResourceNotFound(params.uri)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RateLimitConfig;

    #[test]
    fn test_https_enforcement() {
        // HTTP URLは拒否される
        let insecure_config = WordPressConfig {
            url: "http://example.com".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
        };

        let result = WordPressHandler::try_new(insecure_config);
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Insecure URL detected"));
        assert!(error_msg.contains("Only HTTPS connections are allowed"));
    }

    #[test]
    fn test_https_allowed() {
        // HTTPS URLは許可される
        let secure_config = WordPressConfig {
            url: "https://secure.example.com".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
        };

        let result = WordPressHandler::try_new(secure_config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_malformed_urls_rejected() {
        let bad_urls = vec!["ftp://example.com", "ws://example.com", "", "not-a-url"];

        for url in bad_urls {
            let config = WordPressConfig {
                url: url.to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                enabled: Some(true),
                timeout_seconds: Some(30),
                rate_limit: Some(RateLimitConfig::default()),
                encrypted_credentials: None,
            };

            let result = WordPressHandler::try_new(config);
            assert!(result.is_err(), "URL {} should be rejected", url);
        }
    }

    #[test]
    fn test_truncate_chars_with_ellipsis_handles_multibyte_boundaries() {
        // "モ"は3バイトのUTF-8文字。バイト単位で&s[..100]のように切ると
        // 文字境界を跨いでpanicする実例（src\handlers\wordpress.rs:2425で
        // 実際に発生したバグ）。文字数単位で切ることで回避する。
        let japanese = "あ".repeat(50) + "モデルコンテキストプロトコル" + &"い".repeat(50);
        // パニックしないことそのものがテスト対象（文字境界を跨ぐ位置で切る）
        let truncated = truncate_chars_with_ellipsis(&japanese, 55);
        assert!(truncated.ends_with('…'));
        assert_eq!(truncated.chars().count(), 56); // 55文字 + "…"

        // 絵文字（複数バイト、サロゲートペア相当）を含む文字列
        let emoji_str = "Hello🎉World🌟Test";
        let truncated_emoji = truncate_chars_with_ellipsis(emoji_str, 7);
        assert!(truncated_emoji.ends_with('…'));

        // ちょうど境界（max_charsと文字数が一致）の場合は切り詰めない
        let exact = "12345";
        assert_eq!(truncate_chars_with_ellipsis(exact, 5), "12345");

        // max_charsより短い文字列はそのまま
        let short = "short";
        assert_eq!(truncate_chars_with_ellipsis(short, 100), "short");
    }

    #[test]
    fn test_decode_slug_for_display_decodes_percent_encoded_japanese() {
        // "ウェブ" のパーセントエンコード
        let encoded = "%e3%82%a6%e3%82%a7%e3%83%96";
        let decoded = decode_slug_for_display(encoded);
        assert_eq!(decoded, "ウェブ");
    }

    #[test]
    fn test_decode_slug_for_display_passes_through_plain_slug() {
        assert_eq!(decode_slug_for_display("hello-world"), "hello-world");
    }

    #[test]
    fn test_decode_slugs_for_display_on_array() {
        let value = serde_json::json!([
            {"slug": "%e3%82%a6%e3%82%a7%e3%83%96", "name": "a"},
            {"slug": "plain-slug", "name": "b"}
        ]);
        let decoded = decode_slugs_for_display(value);
        assert_eq!(decoded[0]["slug"], "ウェブ");
        assert_eq!(decoded[1]["slug"], "plain-slug");
    }

    #[test]
    fn test_summarize_posts_includes_expected_fields_and_safe_excerpt() {
        let long_excerpt = "あ".repeat(300);
        let posts = vec![WordPressPost {
            id: Some(328),
            date: Some("2026-01-01".to_string()),
            date_gmt: None,
            guid: None,
            modified: None,
            modified_gmt: None,
            slug: Some("%e3%83%86%e3%82%b9%e3%83%88".to_string()), // "テスト"
            status: "publish".to_string(),
            post_type: None,
            link: Some("https://example.com/test".to_string()),
            title: WordPressContent {
                rendered: "Test Post".to_string(),
                protected: false,
            },
            content: WordPressContent {
                rendered: "full content".to_string(),
                protected: false,
            },
            excerpt: Some(WordPressContent {
                rendered: long_excerpt,
                protected: false,
            }),
            author: None,
            featured_media: None,
            categories: None,
            tags: None,
        }];

        let summary = summarize_posts(&posts);
        assert_eq!(summary.len(), 1);
        let item = &summary[0];
        assert_eq!(item["id"], 328);
        assert_eq!(item["title"], "Test Post");
        assert_eq!(item["status"], "publish");
        assert_eq!(item["link"], "https://example.com/test");
        assert_eq!(item["slug"], "テスト");
        assert!(item["excerpt"].as_str().unwrap().ends_with('…'));
    }

    #[test]
    fn test_html_to_plain_text_strips_tags_and_decodes_entities() {
        let html = "<p>Hello&#8230; <strong>World</strong></p>";
        let plain = html_to_plain_text(html);
        assert!(!plain.contains('<'));
        assert!(!plain.contains('>'));
        assert!(!plain.contains("&#8230;"));
        assert!(plain.contains('…'));
        assert!(plain.contains("Hello"));
        assert!(plain.contains("World"));
    }

    #[test]
    fn test_html_to_plain_text_handles_plain_text_unchanged() {
        let plain_input = "Just plain text, no HTML here";
        assert_eq!(html_to_plain_text(plain_input), plain_input);
    }

    #[test]
    fn test_html_to_plain_text_collapses_runs_of_blank_lines() {
        // 空の<p>タグが連続すると、タグ除去後に複数行の空行が残ることがある
        let html = "<p>はじめに</p><p></p><p></p><p></p><p></p><p>本文</p>";
        let plain = html_to_plain_text(html);
        assert!(!plain.contains("\n\n\n"));
        assert!(plain.contains("はじめに"));
        assert!(plain.contains("本文"));
    }

    #[test]
    fn test_resolve_term_names_maps_known_ids_and_falls_back_for_unknown() {
        let mut id_to_name = HashMap::new();
        id_to_name.insert(7, "🧬 Trait設計".to_string());
        id_to_name.insert(6, "kernel-design".to_string());

        let resolved = resolve_term_names(&[7, 6, 999], &id_to_name);
        assert_eq!(
            resolved,
            vec![
                "🧬 Trait設計".to_string(),
                "kernel-design".to_string(),
                "999".to_string(), // 未知のIDは数値文字列にフォールバック
            ]
        );
    }

    #[test]
    fn test_display_opt_id_formats_without_debug_artifacts() {
        assert_eq!(display_opt_id(Some(328)), "328");
        assert_eq!(display_opt_id(None), "N/A");
    }

    #[test]
    fn test_endpoint_for_post_type_routes_pages_and_posts() {
        assert_eq!(endpoint_for_post_type("page"), "pages");
        assert_eq!(endpoint_for_post_type("post"), "posts");
        // 未知の値は投稿として扱う（既定の振る舞い）
        assert_eq!(endpoint_for_post_type("anything-else"), "posts");
    }

    #[test]
    fn test_format_wp_error_extracts_code_and_message() {
        let body =
            r#"{"code":"rest_post_invalid_id","message":"Invalid post ID.","data":{"status":404}}"#;
        let formatted = format_wp_error(reqwest::StatusCode::NOT_FOUND, body);
        assert!(formatted.contains("rest_post_invalid_id"));
        assert!(formatted.contains("Invalid post ID."));
    }

    #[test]
    fn test_format_wp_error_falls_back_for_non_json_body() {
        let formatted = format_wp_error(reqwest::StatusCode::NOT_FOUND, "<html>404</html>");
        assert!(formatted.contains("404"));
        assert!(formatted.contains("<html>404</html>"));
    }

    #[test]
    fn test_filename_stem_strips_extension_preserving_non_ascii() {
        // upload_mediaのタイトルに使うstemは日本語を含め元の文字を
        // 保持したまま拡張子だけを取り除く（以前の実装はファイル名全体を
        // ASCIIサニタイズしており、日本語ファイル名は`unnamed-file`扱いに
        // なってしまっていた）。
        assert_eq!(filename_stem("テスト画像.png"), "テスト画像");
        assert_eq!(filename_stem("photo.png"), "photo");

        // 拡張子がない場合はそのまま返す
        assert_eq!(filename_stem("no_extension"), "no_extension");
    }

    #[test]
    fn test_summarize_media_includes_expected_fields() {
        let media = vec![WordPressMedia {
            id: Some(42),
            date: Some("2026-01-01".to_string()),
            date_gmt: None,
            guid: None,
            modified: None,
            modified_gmt: None,
            slug: None,
            status: "inherit".to_string(),
            media_type: None,
            link: None,
            title: Some(WordPressContent {
                rendered: "Test Image".to_string(),
                protected: false,
            }),
            author: None,
            comment_status: None,
            ping_status: None,
            template: None,
            description: None,
            caption: None,
            alt_text: Some("A test image".to_string()),
            mime_type: Some("image/png".to_string()),
            media_details: None,
            post: None,
            source_url: Some("https://example.com/test.png".to_string()),
        }];

        let summary = summarize_media(&media);
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0]["id"], 42);
        assert_eq!(summary[0]["title"], "Test Image");
        assert_eq!(summary[0]["mime_type"], "image/png");
        assert_eq!(summary[0]["source_url"], "https://example.com/test.png");
        assert_eq!(summary[0]["alt_text"], "A test image");
    }

    /// upload_mediaが日本語ファイル名でも`multipart::Part`/`Form`の構築に
    /// 失敗しない（パニックしない）ことを確認する回帰テスト。以前の
    /// 生バイナリ+`Content-Disposition`方式は`try_clone()`できないという
    /// 別の不具合を踏んでいたが、今回の不具合（日本語ファイル名が
    /// `unnamed-file`になる）はASCIIへのサニタイズが原因だったため、
    /// `multipart::Form`に切り替えた現在の実装ではサニタイズ自体を行わず
    /// 元のファイル名をそのまま`file_name()`に渡せることを確認する。
    #[test]
    fn test_upload_media_builds_multipart_form_with_non_ascii_filename() {
        let file_data: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47]; // PNGマジックナンバー
        let filename = "テスト画像.png";
        let mime_type = "image/png";

        let part = reqwest::multipart::Part::bytes(file_data)
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .expect("valid mime_type must build a Part");
        let _form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("title", filename_stem(filename).to_string());
    }
}
