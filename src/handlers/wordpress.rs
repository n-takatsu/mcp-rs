use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::WordPressConfig;
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolAnnotations,
    ToolCallParams,
};
use crate::security::{
    AuditCategory, AuditLevel, AuditLogEntry, AuditLogger, RateLimiter, SecureCredentials,
};

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

/// `filename_stem`と同じ規則（最後の`.`で分割）で拡張子を返す。
/// `std::path::Path::extension`はOSによって区切り文字の扱いが異なるため
/// 使わない。`.`を含まない場合は`None`。
fn filename_extension(filename: &str) -> Option<&str> {
    filename.rsplit_once('.').map(|(_, ext)| ext)
}

/// 再送するHTTPステータスか。一時的な失敗（5xx・429）は再送するが、
/// 501 Not Implementedは再送しても結果が変わらないため除く（例: メディアの
/// ゴミ箱移動に対応していないサイトでの`rest_trash_not_supported`）。
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status.is_server_error() && status != reqwest::StatusCode::NOT_IMPLEMENTED)
}

/// タイムアウトや5xxのとき、再送しても結果が変わらないメソッドか。
/// POSTは作成が重複し、DELETEは1回目が処理済みなら2回目が404になる
/// ため含めない（#388）。
fn is_safe_to_resend(method: &reqwest::Method) -> bool {
    matches!(
        *method,
        reqwest::Method::GET
            | reqwest::Method::HEAD
            | reqwest::Method::OPTIONS
            | reqwest::Method::PUT
    )
}

/// WordPressの1つの応答の大きさの上限（#598）。乗っ取られたサイト等が、
/// 大きな応答でメモリーを使い切らせないように。1ページ100件の投稿（本文
/// 付き）でも、ふつうは数MB
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// エラーの応答の本文は、診断に使う分（WordPressの`code`・`message`）だけ読む
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// 一覧の全件取得（`fetch_all_pages`）で読むページ数の上限（#598）。1ページ
/// 100件なので、10,000件。`X-WP-TotalPages`をそのまま信じると、大量の
/// ページを読み続けさせられる
const MAX_LIST_PAGES: u32 = 100;

/// 一覧の全件取得で読む、応答の合計の大きさの上限（#598）
const MAX_LIST_BYTES: usize = 64 * 1024 * 1024;

/// リビジョンの書いた人の名前を読むのに、全体で待つ長さ（#684）。過ぎたら
/// 読めなかった人は`user {ID}`のまま出す
const AUTHOR_NAMES_TIMEOUT: Duration = Duration::from_secs(10);

/// 応答の本文を、`limit`バイトまで読む（#598）。超えたら、読むのをやめて
/// エラーにする（`Content-Length`が上限を超えていれば、読まずに）
async fn read_body_limited(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<String, McpError> {
    let too_large = || {
        McpError::ExternalApi(format!(
            "the WordPress response is larger than {} MB, the most mcp-rs reads for one request",
            limit / (1024 * 1024)
        ))
    };
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(McpError::Http)? {
        if body.len() + chunk.len() > limit {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// エラーの応答の本文を、`MAX_ERROR_BODY_BYTES`まで読む。読めなければ空。
/// 上限を超えた分は読まない（診断には先頭で足りる）
async fn read_error_body(mut response: reqwest::Response) -> String {
    let mut body = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        let room = MAX_ERROR_BODY_BYTES - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if body.len() >= MAX_ERROR_BODY_BYTES {
            break;
        }
    }
    String::from_utf8_lossy(&body).into_owned()
}

/// POST・DELETEを再送しなかったときに、エラーメッセージに添える注意。
/// MCPクライアントが結果を確かめずに同じ呼び出しを繰り返さないように。
const NOT_RESENT_NOTE: &str = ". Not retried automatically: WordPress may have already processed this request. Check whether it took effect before trying again";

/// 成功（2xx）応答の本文を解析する。失敗しても再送しない：WordPressは
/// 既にリクエストを処理済みであり、再送すると作成の重複や、削除済みの
/// 対象への2回目の削除（404）を招くため。
fn parse_success_body<T>(text: &str) -> Result<T, McpError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    serde_json::from_str::<T>(text).map_err(|e| {
        warn!(
            "JSON parse error on a successful response (not retried): {}",
            e
        );
        McpError::ExternalApi(format!(
            "WordPress processed the request, but its response could not be parsed: {}",
            e
        ))
    })
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

/// `context=edit`での取得が拒否された（401/403）エラーかどうか。編集権限が
/// 無い場合WordPressは`rest_forbidden_context`を返すが、セキュリティ
/// プラグインやWAFが別のコードやHTMLの応答で拒否することもあるため、
/// コードではなくステータスで判定する（パスワード誤りでも1回余分に
/// 取得し直すが、表示用コンテキストでも同じく失敗するだけで実害はない）。
/// `execute_request_with_retry`はWordPressの4xx応答を`format_wp_error`で
/// 整形した`ExternalApi`として返すため、その先頭のステータスで判定する。
fn is_edit_context_denied(error: &McpError) -> bool {
    matches!(
        error,
        McpError::ExternalApi(msg)
            if msg.starts_with("WordPress API error 401")
                || msg.starts_with("WordPress API error 403")
    )
}

/// `get_post`が本文をどの形で返すか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostContentFormat {
    /// 編集用の原文（取得できなければ表示用HTMLに切り替える）
    Raw,
    /// WordPressが整形した表示用HTML
    Rendered,
    /// 表示用HTMLからタグを除いたプレーンテキスト
    Text,
}

impl PostContentFormat {
    /// ツール引数の`format`を解釈する。省略・`null`は既定の`raw`とし、
    /// 文字列以外の値や未知の文字列はエラーにする。
    fn parse(value: Option<&serde_json::Value>) -> Result<Self, McpError> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(Self::Raw),
            Some(serde_json::Value::String(s)) if s == "raw" => Ok(Self::Raw),
            Some(serde_json::Value::String(s)) if s == "rendered" => Ok(Self::Rendered),
            Some(serde_json::Value::String(s)) if s == "text" => Ok(Self::Text),
            Some(other) => Err(McpError::InvalidParams(format!(
                "Invalid format {}: expected 'raw', 'rendered' or 'text'",
                truncate_chars_with_ellipsis(&other.to_string(), 100)
            ))),
        }
    }
}

/// タイトルや本文を指定の形で、切り詰めずに返す。原文を要求したが取得
/// できなかった場合は表示用HTMLを返す（その判定は`post_format_label`）。
fn post_content_for_display(content: &WordPressContent, format: PostContentFormat) -> String {
    match format {
        PostContentFormat::Raw => content
            .raw
            .clone()
            .unwrap_or_else(|| content.rendered.clone()),
        PostContentFormat::Rendered => content.rendered.clone(),
        PostContentFormat::Text => html_to_plain_text(&content.rendered),
    }
}

/// `get_post`の出力で、タイトル・本文を実際にどの形で返したかを示す
/// ラベル。原文を要求したが取得できなかった場合はその旨を含める
/// （タイトルと本文は同じ応答から取るため、どちらかだけが原文になる
/// ことはない）。
fn post_format_label(post: &WordPressPost, format: PostContentFormat) -> &'static str {
    // 本文のない投稿タイプ（#684）ではタイトルで見る
    let has_raw = post
        .content
        .as_ref()
        .map_or(post.title.raw.is_some(), |content| content.raw.is_some());
    match format {
        PostContentFormat::Raw if has_raw => "raw",
        PostContentFormat::Raw => {
            "rendered HTML (raw source unavailable: it requires credentials with edit permission)"
        }
        PostContentFormat::Rendered => "rendered HTML",
        PostContentFormat::Text => "plain text",
    }
}

/// 閲覧パスワード付きの投稿を表示用コンテキストで取得すると、WordPressは
/// 本文を空にして返す。空の本文を見たエージェントが投稿を上書きして
/// しまわないよう、その場合に添える注意書き。
fn protected_content_note(post: &WordPressPost) -> Option<&'static str> {
    let content = post.content.as_ref()?;
    (content.protected && content.raw.is_none()).then_some(
        "This post is password-protected, so its content is hidden. Do not treat the content as empty or overwrite it.",
    )
}

/// 本文のない投稿タイプ（`editor`に対応していないカスタム投稿タイプ）の
/// 投稿に添える注意書き（#684）。空の本文と取り違えて書き込まないように
const NO_CONTENT_FIELD_NOTE: &str =
    "This post type has no content field, so there is no content to show or update.";

/// `create_post_with_embeds`で動画プレーヤーにするYouTubeのホスト（#680）。
/// どれも完全に一致するものだけを受け付ける（部分一致・後ろの一致では見ない）。
/// 増やすときは、ここに足す
const YOUTUBE_HOSTS: &[&str] = &["youtube.com", "www.youtube.com", "m.youtube.com"];
/// `/embed/<ID>`だけを受け付ける、プライバシー強化モードのホスト
const YOUTUBE_NOCOOKIE_HOSTS: &[&str] = &["youtube-nocookie.com", "www.youtube-nocookie.com"];
/// `/<ID>`の短いリンクのホスト
const YOUTUBE_SHORT_LINK_HOST: &str = "youtu.be";

/// `create_post_with_embeds`で本文に足すSNSのURL（#680）。サービスの名前・
/// 受け付けるホスト（完全に一致するもの）・パスの頭。増やすときは、ここに足す
const SOCIAL_EMBED_HOSTS: &[(&str, &[&str], &str)] = &[
    (
        "twitter",
        &[
            "twitter.com",
            "www.twitter.com",
            "mobile.twitter.com",
            "x.com",
            "www.x.com",
            "mobile.x.com",
        ],
        "/",
    ),
    ("instagram", &["instagram.com", "www.instagram.com"], "/p/"),
    (
        "facebook",
        &["facebook.com", "www.facebook.com", "m.facebook.com"],
        "/",
    ),
    (
        "tiktok",
        &[
            "tiktok.com",
            "www.tiktok.com",
            "m.tiktok.com",
            "vm.tiktok.com",
            "vt.tiktok.com",
        ],
        "/",
    ),
];

/// `http`・`https`のURLだけを分解する。ホストの大文字は小文字になる
fn parse_web_url(url: &str) -> Option<url::Url> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    matches!(parsed.scheme(), "http" | "https").then_some(parsed)
}

/// YouTubeの動画のIDとして使える文字（英数字・`_`・`-`）だけなら、そのまま返す。
/// 埋め込みのHTMLに入れるので、ほかの文字は受け付けない
fn youtube_video_id(id: &str) -> Option<String> {
    (!id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
    .then(|| id.to_string())
}

/// リソースのテンプレートで読むもの（#644）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceKind {
    Post,
    Page,
    Media,
}

/// テンプレートの`{id}`か、埋めたID
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceId {
    Placeholder,
    Id(u64),
}

/// `wordpress://post/{id}`・`wordpress://post/123`等を読む（#644）。IDは
/// 正の整数だけ（先頭の0・符号・空白等は受けない）
fn resource_template_target(uri: &str) -> Option<(ResourceKind, ResourceId)> {
    let rest = uri.strip_prefix("wordpress://")?;
    let (kind, id) = rest.split_once('/')?;
    let kind = match kind {
        "post" => ResourceKind::Post,
        "page" => ResourceKind::Page,
        "media" => ResourceKind::Media,
        _ => return None,
    };
    let id = if id == "{id}" {
        ResourceId::Placeholder
    } else if !id.is_empty()
        && id.len() <= 20
        && !id.starts_with('0')
        && id.chars().all(|c| c.is_ascii_digit())
    {
        ResourceId::Id(id.parse().ok()?)
    } else {
        return None;
    };
    Some((kind, id))
}

/// WordPressが「無い」（404）と答えたら、リソースが無い（`-32002`）にする（#644）
fn not_found_as_resource(error: McpError, uri: &str) -> McpError {
    match &error {
        McpError::ExternalApi(message) if message.contains(" 404 ") => {
            McpError::ResourceNotFound(uri.to_string())
        }
        _ => error,
    }
}

/// WordPressの投稿タイプ・タクソノミーの名前の形か（#642）。URLに入れるので、
/// 英小文字・数字・`_`・`-`だけを受ける
fn is_slug(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
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

/// 投稿のアイキャッチ画像（`featured_media`）を`get_post`の表示用に
/// まとめる。WordPressは未設定を`0`で返すため、`0`と欠落はどちらも
/// 未設定として扱う。画像のURLが取れなかった場合（メディアの取得に
/// 失敗した等）はIDだけを表示する。
fn featured_media_label(media_id: Option<u64>, source_url: Option<&str>) -> String {
    match (media_id, source_url) {
        (None | Some(0), _) => "none".to_string(),
        (Some(id), Some(url)) if !url.is_empty() => format!("{} ({})", id, url),
        (Some(id), _) => id.to_string(),
    }
}

/// `get_post`・`update_post`に出す公開日時。予約済みなら、そう添える
/// （#625）。WordPressの`date`は、サイトのタイムゾーンの時刻
fn post_date_label(post: &WordPressPost) -> String {
    match (post.date.as_deref(), post.status.as_str()) {
        (Some(date), "future") => format!("{} (site time, scheduled)", date),
        (Some(date), _) => format!("{} (site time)", date),
        (None, _) => "N/A".to_string(),
    }
}

/// `get_posts`の`status`引数（#625）。WordPressの投稿の状態の名前
/// （英小文字・`_`・`-`）か`any`を、カンマ区切りで受ける。URLに入れる
/// ので、ほかの文字は断る
fn post_status_filter_arg(
    args: &HashMap<String, serde_json::Value>,
) -> Result<Option<String>, McpError> {
    let Some(value) = args.get("status") else {
        return Ok(None);
    };
    let Some(status) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(McpError::InvalidParams(
            "status must be a non-empty string, e.g. 'future' or 'future,draft'".to_string(),
        ));
    };
    let valid = status.split(',').all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c == '-')
    });
    if !valid {
        return Err(McpError::InvalidParams(format!(
            "status '{}' is not valid: use post statuses such as 'publish', 'future', 'draft', 'pending', 'private' or 'any', separated by commas",
            status
        )));
    }
    Ok(Some(status.to_string()))
}

/// 検索・絞り込み・ページを指定したときに、1ページで返す件数の既定（#637）
const DEFAULT_LIST_PER_PAGE: u32 = 20;

/// `get_posts`・`get_pages`の検索・絞り込み・ページ（#637）。値はURLに入れる
/// ので、`post_list_query_arg`で形を確かめてから作る
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct PostListQuery {
    /// 投稿の状態（#625）
    status: Option<String>,
    search: Option<String>,
    categories: Vec<u64>,
    tags: Vec<u64>,
    author: Option<u64>,
    /// この日時より後（サイトのタイムゾーン、ISO8601）
    after: Option<String>,
    /// この日時より前
    before: Option<String>,
    orderby: Option<String>,
    order: Option<String>,
    per_page: Option<u32>,
    page: Option<u32>,
}

impl PostListQuery {
    /// 検索・絞り込み・ページのどれかを指定したか。指定したら、全件ではなく、
    /// そのページだけを読む。`status`だけなら、今までどおり全件（#625）
    fn is_single_page(&self) -> bool {
        self.search.is_some()
            || !self.categories.is_empty()
            || !self.tags.is_empty()
            || self.author.is_some()
            || self.after.is_some()
            || self.before.is_some()
            || self.orderby.is_some()
            || self.order.is_some()
            || self.per_page.is_some()
            || self.page.is_some()
    }

    /// 一覧のURLに付ける問い合わせ（`per_page`・`page`を除く）。値は
    /// URLの形に符号化する
    fn query_string(&self) -> String {
        let join = |ids: &[u64]| ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        let pairs = [
            ("status", self.status.clone()),
            ("search", self.search.clone()),
            (
                "categories",
                (!self.categories.is_empty()).then(|| join(&self.categories)),
            ),
            ("tags", (!self.tags.is_empty()).then(|| join(&self.tags))),
            ("author", self.author.map(|a| a.to_string())),
            ("after", self.after.clone()),
            ("before", self.before.clone()),
            ("orderby", self.orderby.clone()),
            ("order", self.order.clone()),
        ];
        for (key, value) in pairs {
            if let Some(value) = value {
                query.append_pair(key, &value);
            }
        }
        query.finish()
    }
}

/// 1ページだけを読んだときの、ページの情報（#637）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListPage {
    page: u32,
    per_page: u32,
    total_pages: u32,
    total: u64,
}

/// `get_posts`・`get_pages`の引数から、検索・絞り込み・ページを読む（#637）。
/// `with_taxonomies`が偽（固定ページ）なら、`categories`・`tags`を断る
fn post_list_query_arg(
    args: &HashMap<String, serde_json::Value>,
    with_taxonomies: bool,
) -> Result<PostListQuery, McpError> {
    let invalid = |message: String| McpError::InvalidParams(message);
    let positive = |name: &str| -> Result<Option<u64>, McpError> {
        match args.get(name) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(value) => value
                .as_u64()
                .filter(|n| *n > 0)
                .map(Some)
                .ok_or_else(|| invalid(format!("{name} must be a positive integer"))),
        }
    };
    let ids = |name: &str| -> Result<Vec<u64>, McpError> {
        let Some(value) = args.get(name) else {
            return Ok(Vec::new());
        };
        if !with_taxonomies {
            return Err(invalid(format!("pages have no {name}")));
        }
        let values = match value {
            serde_json::Value::Array(values) => values.clone(),
            other => vec![other.clone()],
        };
        if values.is_empty() || values.len() > 100 {
            return Err(invalid(format!("{name} must list 1 to 100 IDs")));
        }
        values
            .iter()
            .map(|v| {
                v.as_u64()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| invalid(format!("{name} must be positive integer IDs")))
            })
            .collect()
    };
    let text = |name: &str| -> Result<Option<String>, McpError> {
        match args.get(name) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(value) => value
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Some(s.to_string()))
                .ok_or_else(|| invalid(format!("{name} must be a non-empty string"))),
        }
    };
    // 日時（ISO8601）。日付だけなら、その日の0時にする
    let datetime = |name: &str| -> Result<Option<String>, McpError> {
        let Some(value) = text(name)? else {
            return Ok(None);
        };
        let looks_like_datetime = value.len() >= 10
            && value.len() <= 35
            && value.as_bytes()[..10].iter().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    *b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
            && value
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | 'T' | '+' | 'Z' | '.'));
        if !looks_like_datetime {
            return Err(invalid(format!(
                "{name} must be a date or date-time in ISO8601, e.g. '2026-11-01' or '2026-11-01T09:00:00'"
            )));
        }
        Ok(Some(if value.len() == 10 {
            format!("{value}T00:00:00")
        } else {
            value
        }))
    };

    let search = text("search")?;
    if search.as_ref().is_some_and(|s| s.chars().count() > 200) {
        return Err(invalid("search must be at most 200 characters".to_string()));
    }
    let orderby = text("orderby")?;
    if let Some(orderby) = &orderby {
        let mut allowed = vec![
            "date",
            "modified",
            "title",
            "id",
            "slug",
            "author",
            "relevance",
        ];
        if !with_taxonomies {
            allowed.push("menu_order");
        }
        if !allowed.contains(&orderby.as_str()) {
            return Err(invalid(format!(
                "orderby must be one of: {}",
                allowed.join(", ")
            )));
        }
        if orderby == "relevance" && search.is_none() {
            return Err(invalid("orderby 'relevance' needs search".to_string()));
        }
    }
    let order = text("order")?;
    if order.as_deref().is_some_and(|o| o != "asc" && o != "desc") {
        return Err(invalid("order must be 'asc' or 'desc'".to_string()));
    }
    let per_page = positive("per_page")?;
    if per_page.is_some_and(|n| n > 100) {
        return Err(invalid("per_page must be 1 to 100".to_string()));
    }
    let page = positive("page")?;
    if page.is_some_and(|n| n > u64::from(u32::MAX)) {
        return Err(invalid("page is too large".to_string()));
    }
    Ok(PostListQuery {
        status: post_status_filter_arg(args)?,
        search,
        categories: ids("categories")?,
        tags: ids("tags")?,
        author: positive("author")?,
        after: datetime("after")?,
        before: datetime("before")?,
        orderby,
        order,
        per_page: per_page.map(|n| n as u32),
        page: page.map(|n| n as u32),
    })
}

/// 一覧の結果の文（#637）。1ページだけを読んだときは、ページと総件数を添える
fn list_result_text(kind: &str, items: &[WordPressPost], page: Option<ListPage>) -> String {
    let summary = serde_json::to_string_pretty(&summarize_posts(items))
        .unwrap_or_else(|_| format!("Failed to serialize {kind}"));
    match page {
        None => format!("Found {} {}:\n{}", items.len(), kind, summary),
        Some(page) => format!(
            "Found {} {} (page {} of {}, {} per page; {} in total):\n{}",
            items.len(),
            kind,
            page.page,
            page.total_pages.max(1),
            page.per_page,
            page.total,
            summary
        ),
    }
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

/// `call_tool`の全ツール呼び出し共通のレート制限キー。
/// `WordPressHandler`は1インスタンスにつき1つのWordPressサイトに
/// 対応するため、クライアント単位ではなくハンドラ（サイト接続）単位で
/// 制限する。
const RATE_LIMIT_CLIENT_ID: &str = "wordpress-tool-calls";

/// ツールの性質のヒント（annotations、#417）を、ツール名の命名規則
/// （`is_mutating_tool`と同じ）から決める。クライアントは、これを見て実行前に
/// 確認するか等を決める。
/// - 変更しないツール（`get_*`等）: 読み取り専用
/// - `create_*`・`upload_*`: 新しく作る。既存のものは変えないが、繰り返すと
///   その分作られる
/// - `update_*`・`set_*`・`delete_*`・`restore_*`: 既存のものを上書き・削除する。同じ引数で
///   繰り返しても、追加の影響は無い
///
/// どのツールも、設定したWordPressサイトとだけやり取りする（開かれた世界では
/// ない）。
fn wordpress_tool_annotations(name: &str) -> ToolAnnotations {
    let base = ToolAnnotations {
        open_world_hint: Some(false),
        ..Default::default()
    };
    if !is_mutating_tool(name) {
        return ToolAnnotations {
            read_only_hint: Some(true),
            ..base
        };
    }
    let creates = name.starts_with("create_") || name.starts_with("upload_");
    ToolAnnotations {
        read_only_hint: Some(false),
        destructive_hint: Some(!creates),
        idempotent_hint: Some(!creates),
        ..base
    }
}

/// 変更系ツールのツール名接頭辞。本ファイルのツール命名規則
/// （`create_*`/`update_*`/`delete_*`/`upload_*`/`set_*`/`restore_*`）に合わせており、
/// 新規ツール追加時も個別対応なしに自動的に監査ログ対象へ含まれる。
const MUTATING_TOOL_PREFIXES: &[&str] = &[
    "create_", "update_", "delete_", "upload_", "set_", "restore_",
];

/// ツール名が変更系（WordPress側の状態を変更する）かどうかを判定する。
fn is_mutating_tool(name: &str) -> bool {
    MUTATING_TOOL_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// `upload_media`で許可する最大ファイルサイズ（10 MiB）の絶対上限。
/// 実効上限は通常これより小さく、トランスポートのメッセージサイズ上限
/// （STDIOの`max_message_size`・HTTPの`max_request_size`、既定1 MiB）に
/// base64の膨張分を加味した値になる。運用者がトランスポート側の上限を
/// 引き上げた場合でも、WordPressへのアップロードはこの値で頭打ちになる。
const MAX_UPLOAD_SIZE_BYTES: usize = 10 * 1024 * 1024;

/// `MAX_UPLOAD_SIZE_BYTES`をbase64エンコードした場合の最大長。JSON-RPC
/// メッセージ自体の読み込み・パースは既にトランスポート層で済んでいる
/// ため、ここで防げるのはデコード時の追加のメモリ確保のみ（メッセージ
/// 全体のサイズ制御はトランスポートの責務）。
const MAX_UPLOAD_BASE64_LEN: usize = MAX_UPLOAD_SIZE_BYTES.div_ceil(3) * 4;

/// PDFヘッダ（`%PDF-`）の探索範囲。PDF仕様上、ヘッダは先頭ちょうどで
/// なくとも先頭1024バイト以内にあればよく、BOMや空白を前置きする
/// ツールも存在するため。ただし前置きとして許すのはBOMと空白のみ
/// （`<html>...%PDF-`のようなHTML前置きのポリグロットは拒否する）。
const PDF_HEADER_SEARCH_WINDOW: usize = 1024;

/// 検証エラーメッセージに含めるユーザー入力（mime_type・filename）の
/// 最大文字数。巨大な文字列がそのままエラー応答やログに反射されるのを
/// 防ぐ。
const MAX_ECHOED_INPUT_CHARS: usize = 100;

/// `upload_media`で受け付けるファイル名の最大文字数（一般的なファイル
/// システムのファイル名上限に合わせる）。
const MAX_UPLOAD_FILENAME_CHARS: usize = 255;

/// ファイル名として受け付けない文字かどうか。パス区切り、制御文字
/// （改行によるログ偽装等）、双方向テキスト制御文字（U+202E等で
/// `invoice\u{202E}gpj.exe`を`invoice exe.jpg`のように見せかける拡張子
/// 偽装）を拒否する。
fn is_forbidden_filename_char(c: char) -> bool {
    c == '/'
        || c == '\\'
        || c.is_control()
        || matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// エラーメッセージやツールスキーマで示す、許可MIMEタイプの一覧。
fn allowed_upload_mime_list() -> String {
    ALLOWED_UPLOADS
        .iter()
        .map(|(m, _)| *m)
        .collect::<Vec<_>>()
        .join(", ")
}

/// PDFのヘッダが（BOM・空白の前置きを許容しつつ）先頭1024バイト以内に
/// あるかを判定する。
fn has_pdf_header(data: &[u8]) -> bool {
    let window = &data[..data.len().min(PDF_HEADER_SEARCH_WINDOW)];
    let without_bom = window.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(window);
    let start = without_bom
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(without_bom.len());
    without_bom[start..].starts_with(b"%PDF-")
}

/// `upload_media`で許可するMIMEタイプと、それぞれに対応する許可拡張子。
/// WordPressは保存形式をMIMEではなく拡張子で判定する
/// （`wp_check_filetype_and_ext`）ため、拡張子も併せて検証しないと
/// 「`image/png`と宣言しつつ`x.html`として保存させる」ことができてしまう。
///
/// SVGは`<script>`を埋め込めるXMLベースの形式であり、WordPress自身も
/// 既定で無効化しているため含めない。`text/plain`等マジックナンバーを
/// 持たない自由形式も、内容を検証できない（任意のバイト列を素通し
/// させてしまう）ため含めない。同様に、ZIPベースのOffice文書（docx等）は
/// 「ZIPであること」までしか判定できず、QuickTime（mov）は先頭のatom
/// 順序が一定しないため含めない。全エントリが`file_signature_matches`で
/// 検証可能な形式であることを前提としている。
const ALLOWED_UPLOADS: &[(&str, &[&str])] = &[
    ("image/jpeg", &["jpg", "jpeg", "jpe"]),
    ("image/png", &["png"]),
    ("image/gif", &["gif"]),
    ("image/webp", &["webp"]),
    ("application/pdf", &["pdf"]),
    ("video/mp4", &["mp4", "m4v"]),
    ("video/webm", &["webm"]),
    ("video/ogg", &["ogv"]),
    ("audio/mpeg", &["mp3"]),
    ("audio/mp4", &["m4a"]),
    ("audio/wav", &["wav"]),
    ("audio/ogg", &["ogg", "oga"]),
];

/// ISO Base Media File Format（MP4/M4A等）は先頭4バイトがボックス長、
/// 続く4バイトが`ftyp`ボックスの識別子になる。
fn is_iso_bmff(data: &[u8]) -> bool {
    data.len() >= 8 && &data[4..8] == b"ftyp"
}

/// MIMEタイプを比較用に正規化する。RFC 2045によりMIMEタイプは大文字
/// 小文字を区別しないため小文字化し、`; charset=...`等のパラメータを
/// 除去する。ブラウザやツールが報告する非標準の別名は正規の名前に寄せる。
fn normalize_mime_type(mime_type: &str) -> String {
    let base = mime_type
        .split_once(';')
        .map_or(mime_type, |(base, _)| base)
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "image/jpg" | "image/pjpeg" => "image/jpeg".to_string(),
        "image/x-png" => "image/png".to_string(),
        "audio/mp3" => "audio/mpeg".to_string(),
        "audio/x-m4a" => "audio/mp4".to_string(),
        "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => "audio/wav".to_string(),
        _ => base,
    }
}

/// ファイル先頭バイト（マジックナンバー）が宣言されたMIMEタイプと
/// 一致するか検証する。先頭数バイトのみを見るヒューリスティックであり、
/// 正しいヘッダの後ろに別形式の内容を連結したポリグロットファイルまでは
/// 検出できない。目的はあくまで明白なMIME偽装（実行ファイルやHTMLを
/// 画像と称する等）を弾くことにある。シグネチャ未定義の形式は拒否する
/// （fail-closed）。
fn file_signature_matches(mime_type: &str, data: &[u8]) -> bool {
    match mime_type {
        "image/jpeg" => data.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/png" => data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
        "image/gif" => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
        "image/webp" => data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP",
        "application/pdf" => has_pdf_header(data),
        "video/mp4" | "audio/mp4" => is_iso_bmff(data),
        "video/webm" => data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]),
        "video/ogg" | "audio/ogg" => data.starts_with(b"OggS"),
        // ID3タグ付き、またはMPEGフレーム同期ワード（先頭11ビットが1）
        "audio/mpeg" => {
            data.starts_with(b"ID3")
                || (data.len() >= 2 && data[0] == 0xFF && data[1] & 0xE0 == 0xE0)
        }
        "audio/wav" => data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE",
        _ => false,
    }
}

/// `upload_media`の入力を検証し、WordPressへ送る正規化済みMIMEタイプを
/// 返す。サイズ上限・MIME許可リスト・拡張子とMIMEの整合・マジック
/// ナンバーの順に検証する。
fn validate_upload(
    file_data: &[u8],
    filename: &str,
    mime_type: &str,
) -> Result<&'static str, McpError> {
    // 中身の検証（マジックナンバー）より前に判定し、原因が分かるエラーにする
    if file_data.is_empty() {
        return Err(McpError::InvalidParams("File is empty".to_string()));
    }

    if file_data.len() > MAX_UPLOAD_SIZE_BYTES {
        return Err(McpError::InvalidParams(format!(
            "File too large: {} bytes (max {} bytes)",
            file_data.len(),
            MAX_UPLOAD_SIZE_BYTES
        )));
    }

    let normalized = normalize_mime_type(mime_type);
    let Some(&(canonical, allowed_extensions)) =
        ALLOWED_UPLOADS.iter().find(|(m, _)| *m == normalized)
    else {
        return Err(McpError::InvalidParams(format!(
            "mime_type '{}' is not allowed. Allowed types: {}",
            truncate_chars_with_ellipsis(mime_type, MAX_ECHOED_INPUT_CHARS),
            allowed_upload_mime_list()
        )));
    };

    if filename.chars().count() > MAX_UPLOAD_FILENAME_CHARS {
        return Err(McpError::InvalidParams(format!(
            "filename too long (max {} characters)",
            MAX_UPLOAD_FILENAME_CHARS
        )));
    }

    // WordPress側のサニタイズに頼らずここで拒否する
    if filename.chars().any(is_forbidden_filename_char) {
        return Err(McpError::InvalidParams(format!(
            "filename {:?} must not contain path separators or control characters",
            truncate_chars_with_ellipsis(filename, MAX_ECHOED_INPUT_CHARS)
        )));
    }

    let extension = filename_extension(filename).map(|ext| ext.to_ascii_lowercase());
    if !extension.is_some_and(|e| allowed_extensions.contains(&e.as_str())) {
        return Err(McpError::InvalidParams(format!(
            "filename '{}' extension does not match mime_type '{}' (allowed: {})",
            truncate_chars_with_ellipsis(filename, MAX_ECHOED_INPUT_CHARS),
            canonical,
            allowed_extensions.join(", ")
        )));
    }

    if !file_signature_matches(canonical, file_data) {
        return Err(McpError::InvalidParams(format!(
            "File content does not match declared mime_type '{}'",
            canonical
        )));
    }

    Ok(canonical)
}

/// 設定の`upload_allowed_dirs`を正規化する。存在しないフォルダや
/// フォルダでないパスが指定されていれば起動時にエラーにする（設定の
/// 書き間違いで`file_path`が黙って使えなくなるのを避けるため）。
fn canonicalize_upload_dirs(dirs: &[String]) -> Result<Vec<PathBuf>, String> {
    dirs.iter()
        .map(|dir| {
            std::fs::canonicalize(dir)
                .ok()
                .filter(|path| path.is_dir())
                .ok_or_else(|| {
                    format!(
                        "upload_allowed_dirs entry '{}' is not an existing directory",
                        dir
                    )
                })
        })
        .collect()
}

/// `upload_media`の`file_path`を、許可フォルダ内の実在するファイルの
/// 正規化済みパスに解決する。
///
/// 判定は正規化（`..`・シンボリックリンクの解決）後のパスで行うため、
/// `..`やリンクで許可フォルダの外に出ることはできない。許可フォルダの外を
/// 指す場合と、ファイルが存在しない場合は同じエラーにする（許可フォルダ
/// 外のファイルの有無が分からないように）。
fn resolve_upload_path(file_path: &str, allowed_dirs: &[PathBuf]) -> Result<PathBuf, McpError> {
    if allowed_dirs.is_empty() {
        return Err(McpError::InvalidParams(
            "file_path is disabled. Set upload_allowed_dirs in [handlers.wordpress] to enable it, or use file_data".to_string(),
        ));
    }

    let path = Path::new(file_path);
    // 相対パスはmcp-rsの作業ディレクトリ（起動のされ方で変わる）基準に
    // なり、意図しない場所を指しうるため受け付けない
    if !path.is_absolute() {
        return Err(McpError::InvalidParams(
            "file_path must be an absolute path".to_string(),
        ));
    }

    std::fs::canonicalize(path)
        .ok()
        .filter(|canonical| {
            canonical.is_file() && allowed_dirs.iter().any(|dir| canonical.starts_with(dir))
        })
        .ok_or_else(|| {
            McpError::InvalidParams(
                "file_path must be an existing file inside one of upload_allowed_dirs".to_string(),
            )
        })
}

/// ファイル名の拡張子から、`upload_media`で許可するMIMEタイプを決める
/// （`file_path`で`mime_type`が省略された場合用）。中身の検証は
/// `validate_upload`で別途行う。
fn mime_type_for_filename(filename: &str) -> Option<&'static str> {
    let extension = filename_extension(filename)?.to_ascii_lowercase();
    ALLOWED_UPLOADS
        .iter()
        .find(|(_, extensions)| extensions.contains(&extension.as_str()))
        .map(|(mime_type, _)| *mime_type)
}

/// アップロードするファイルを読み込む。上限を超える分は読まない
/// （確認後にファイルが大きくなった場合も含め、メモリに載せる量を
/// 上限+1バイトに抑える）。
async fn read_upload_file(path: &Path) -> Result<Vec<u8>, McpError> {
    use tokio::io::AsyncReadExt as _;

    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| McpError::InvalidParams(format!("Failed to read file_path: {}", e)))?;
    let mut data = Vec::new();
    file.take(MAX_UPLOAD_SIZE_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .await
        .map_err(|e| McpError::InvalidParams(format!("Failed to read file_path: {}", e)))?;
    if data.len() > MAX_UPLOAD_SIZE_BYTES {
        return Err(McpError::InvalidParams(format!(
            "File too large (max {} bytes)",
            MAX_UPLOAD_SIZE_BYTES
        )));
    }
    Ok(data)
}

/// 引数からベストエフォートで対象リソースのIDを抽出する（監査ログの
/// 補助情報用）。ツールごとに異なるキー名（`post_id`・`media_id`等）を
/// 優先順位付きで試す。見つからない場合は`None`（例: 新規作成系ツールは
/// 呼び出し時点では対象IDを持たない）。
fn extract_target_id(arguments: &Option<HashMap<String, serde_json::Value>>) -> Option<String> {
    const ID_KEYS: &[&str] = &[
        "id",
        "post_id",
        "media_id",
        "category_id",
        "tag_id",
        "page_id",
        "comment_id",
        "posts_page_id",
    ];
    let args = arguments.as_ref()?;
    ID_KEYS.iter().find_map(|key| {
        args.get(*key).map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    })
}

/// 作成する投稿の`status`を検証する。投稿作成系のツールはすべて
/// `create_advanced_post`を経由するため、そこで1度だけ呼ぶ（ツールごとに
/// 検証すると、ツールによって検証の有無がずれる）。
///
/// `future`（予約投稿）を日付なしで送ると、WordPressは即座に公開して
/// しまうため、日付が無い（空文字を含む）場合は拒否する。過去の日付の
/// 判定はサイトのタイムゾーンに依存するため行わない。
///
/// それ以外の値は許可リストで絞らずWordPressに任せる。プラグインが独自の
/// ステータス（PublishPressの`pitch`等）を追加していることがあり、不正な
/// 値はWordPressのエラー内容（`format_wp_error`）としてそのまま返るため。
fn validate_post_status(status: &str, date: Option<&str>) -> Result<(), McpError> {
    let has_date = date.is_some_and(|d| !d.trim().is_empty());
    if status == "future" && !has_date {
        return Err(McpError::InvalidParams(
            "status 'future' (scheduled) requires a publish date; without it WordPress publishes the post immediately. Pass 'date' (ISO8601, site time) with create_advanced_post or update_post; with other tools, use 'draft' instead".to_string(),
        ));
    }
    Ok(())
}

/// 投稿作成系ツールの`status`引数。未指定時は、意図しない公開を避けるため
/// `draft`とする（`create_post`・`create_post_with_categories_tags`と同じ
/// 方針）。値の検証は`validate_post_status`で行う。
fn post_status_or_draft(args: &HashMap<String, serde_json::Value>) -> Result<String, McpError> {
    match args.get("status") {
        None | Some(serde_json::Value::Null) => Ok("draft".to_string()),
        Some(serde_json::Value::String(s)) => Ok(s.clone()),
        Some(other) => Err(McpError::InvalidParams(format!(
            "Invalid status {}: expected a string",
            truncate_chars_with_ellipsis(&other.to_string(), 100)
        ))),
    }
}

/// ツール引数の`slug`を取り出す。省略時は`None`（WordPressに送らない）。
/// 空文字列はWordPressにとって「自動生成に戻す」意味になり、意図せず
/// 既存のスラッグを消しうるため受け付けない。
fn optional_slug_arg(
    args: &HashMap<String, serde_json::Value>,
) -> Result<Option<String>, McpError> {
    match args.get("slug") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(slug)) => {
            let slug = slug.trim();
            if slug.is_empty() {
                Err(McpError::InvalidParams(
                    "slug must not be empty. Omit it to let WordPress generate one".to_string(),
                ))
            } else {
                Ok(Some(slug.to_string()))
            }
        }
        Some(_) => Err(McpError::InvalidParams("slug must be a string".to_string())),
    }
}

/// 投稿・固定ページのスラッグの表示用の文字列。日本語のスラッグは
/// パーセントエンコードされて返るためデコードする。WordPressは、
/// スラッグを指定していない下書きにはスラッグを付けない（公開時に作る）。
fn post_slug_label(post: &WordPressPost) -> String {
    match post.slug.as_deref() {
        Some(slug) if !slug.is_empty() => decode_slug_for_display(slug),
        _ => "none (WordPress generates one when the post is published)".to_string(),
    }
}

/// 投稿・固定ページ作成時の送信内容。WordPressは書き込み時にタイトル・
/// 本文を文字列（または`raw`）としてしか読まないため、読み取り用の
/// `WordPressPost`（`rendered`を持つ）をそのまま送ってはいけない。
/// 投稿作成系のツールはすべてこの関数を経由する。
fn advanced_post_body(params: PostCreateParams) -> serde_json::Value {
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

    // スラッグを設定（未指定ならWordPressがタイトルから作る）
    if let Some(slug) = params.slug {
        post_data["slug"] = serde_json::Value::String(slug);
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

    // タクソノミーの項目（#642）
    if let Some(terms) = params.terms {
        for (taxonomy, ids) in terms {
            post_data[taxonomy] = serde_json::json!(ids);
        }
    }

    post_data
}

/// `DELETE /wp/v2/media/{id}`の応答。`force=true`（完全削除）では
/// `{"deleted": true, "previous": {...}}`の形で削除前のメディアが返り、
/// ゴミ箱への移動（対応しているサイトのみ）ではメディアそのものが返る。
#[derive(Deserialize)]
#[serde(untagged)]
enum DeleteMediaResponse {
    Deleted {
        #[allow(dead_code)]
        deleted: bool,
        previous: WordPressMedia,
    },
    Trashed(WordPressMedia),
}

#[derive(Clone)]
pub struct WordPressHandler {
    client: Client,
    base_url: String,
    /// セキュア認証情報（新しい暗号化対応）
    secure_credentials: Option<SecureCredentials>,
    /// 後方互換性のための平文認証情報
    username: Option<String>,
    password: Option<String>,
    rate_limiter: Arc<RateLimiter>,
    audit_logger: Arc<AuditLogger>,
    /// `upload_media`の`file_path`で読み込みを許可するフォルダ（起動時に
    /// 正規化済み）。空なら`file_path`は使えない。
    upload_allowed_dirs: Arc<Vec<PathBuf>>,
    /// `post`・`page`のほかに使ってよいカスタム投稿タイプ（#642）
    allowed_post_types: Arc<Vec<String>>,
    /// カスタム投稿タイプのREST APIの場所（`rest_base`）。サイトに1回だけ聞く
    type_endpoints: Arc<tokio::sync::Mutex<HashMap<String, String>>>,
}

// Debugトレイトでパスワードを露出しないようにカスタム実装
// （SecureCredentialsのDebug実装と同じ方針）
impl std::fmt::Debug for WordPressHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WordPressHandler")
            .field("client", &self.client)
            .field("base_url", &self.base_url)
            .field("secure_credentials", &self.secure_credentials)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .field("rate_limiter", &self.rate_limiter)
            .field("audit_logger", &self.audit_logger)
            .field("upload_allowed_dirs", &self.upload_allowed_dirs)
            .finish()
    }
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
    /// タイトル。`title`に対応していない投稿タイプ（#642で使えるようにした
    /// カスタム投稿タイプ等）では応答にないため、空として読む（#684）
    #[serde(default)]
    pub title: WordPressContent,
    /// 本文。`editor`に対応していない投稿タイプでは応答にないため`None`
    /// （空の本文とは区別する。#684）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<WordPressContent>,
    pub excerpt: Option<WordPressContent>,
    pub author: Option<u64>,
    pub featured_media: Option<u64>,
    pub categories: Option<Vec<u64>>,
    pub tags: Option<Vec<u64>>,
    /// ほかの項目（独自のタクソノミーの項目等、#642）。読むだけで、書き出さない
    #[serde(flatten, default, skip_serializing)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressGuid {
    pub rendered: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WordPressContent {
    pub rendered: String,
    /// 編集用の原文（Gutenbergのブロック区切り等を含む）。WordPressは
    /// `context=edit`で取得した場合にのみ返す。`None`のときはシリアライズ
    /// しない（このフィールドを持たなかった頃と送信内容を変えないため）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(default)]
    pub protected: bool,
}

/// 投稿・固定ページのリビジョン（#641）
#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressRevision {
    pub id: u64,
    /// リビジョンの元の投稿のID
    #[serde(default)]
    pub parent: u64,
    /// 書いた人（ユーザーのID）
    #[serde(default)]
    pub author: u64,
    #[serde(default)]
    pub date: Option<String>,
    pub title: WordPressContent,
    pub content: WordPressContent,
    #[serde(default)]
    pub excerpt: Option<WordPressContent>,
}

/// リビジョンを1行で表す（#641）。本文は、タグを除いた文字数と、先頭の
/// 100文字
fn revision_summary(revision: &WordPressRevision, author: &str) -> String {
    let text = html_to_plain_text(&revision.content.rendered);
    let text = text.trim();
    format!(
        "ID {} (Date: {}, Author: {}): {} — {} characters: {}",
        revision.id,
        revision.date.as_deref().unwrap_or("unknown"),
        author,
        html_to_plain_text(&revision.title.rendered).trim(),
        text.chars().count(),
        truncate_chars_with_ellipsis(text, 100)
    )
}

/// WordPressのコメント。コメントした人のメールアドレス・IPアドレスは、
/// 管理（承認・スパム・返信）に要らないので、読まない・出さない（#639）
#[derive(Debug, Serialize, Deserialize)]
pub struct WordPressComment {
    pub id: Option<u64>,
    pub post: u64,
    /// 返信先のコメントのID（返信でなければ0）
    #[serde(default)]
    pub parent: u64,
    /// `approved`・`hold`・`spam`・`trash`
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    pub content: HashMap<String, String>,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub link: Option<String>,
}

/// `get_comments`の`status`で受ける、コメントの状態（#639）。`all`は、
/// 承認済みと承認待ち
const COMMENT_LIST_STATUSES: &[&str] = &["approve", "hold", "spam", "trash", "all"];

/// `create_comment`・`update_comment`の`status`で受ける、コメントの状態（#639）
const COMMENT_SET_STATUSES: &[&str] = &["approve", "hold", "spam", "trash"];

/// `create_comment`の既定の状態（#639）。WordPressのコメントには下書きが
/// 無いので、AIが書いた返信は承認待ちで作り、人が確かめてから公開する
const DEFAULT_REPLY_STATUS: &str = "hold";

/// 引数`key`の、コメントの状態を読む（#639）。`allowed`の中の値だけを受ける
fn comment_status_arg(
    args: &HashMap<String, serde_json::Value>,
    key: &str,
    allowed: &[&str],
) -> Result<Option<String>, McpError> {
    match args.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .filter(|s| allowed.contains(s))
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| {
                McpError::InvalidParams(format!("{key} must be one of: {}", allowed.join(", ")))
            }),
    }
}

/// リビジョンのツールの`post_type`（#641）。`post`か`page`だけを受ける
fn revision_post_type_arg(args: &HashMap<String, serde_json::Value>) -> Result<&str, McpError> {
    match args.get("post_type") {
        None | Some(serde_json::Value::Null) => Ok("post"),
        Some(value) => value
            .as_str()
            .ok_or_else(|| McpError::InvalidParams("post_type must be a string".to_string())),
    }
}

/// 投稿の項目として送れない名前（#642）。`terms`で、ほかの項目を上書きしない
const RESERVED_POST_FIELDS: &[&str] = &[
    "id",
    "date",
    "date_gmt",
    "slug",
    "status",
    "type",
    "password",
    "title",
    "content",
    "excerpt",
    "author",
    "featured_media",
    "comment_status",
    "ping_status",
    "format",
    "meta",
    "sticky",
    "template",
    "parent",
    "menu_order",
];

/// 引数`terms`（#642）。`{"<タクソノミーのrest_base>": [項目のID, ...]}`
fn terms_arg(
    args: &HashMap<String, serde_json::Value>,
) -> Result<Option<std::collections::BTreeMap<String, Vec<u64>>>, McpError> {
    let invalid = |message: String| McpError::InvalidParams(message);
    let Some(value) = args.get("terms").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let object = value.as_object().ok_or_else(|| {
        invalid("terms must be an object such as {\"product_cat\": [12, 15]}".to_string())
    })?;
    let mut terms = std::collections::BTreeMap::new();
    for (taxonomy, ids) in object {
        if !is_slug(taxonomy) || RESERVED_POST_FIELDS.contains(&taxonomy.as_str()) {
            return Err(invalid(format!(
                "terms: '{}' is not a taxonomy (use the rest_base from get_taxonomies)",
                truncate_chars_with_ellipsis(taxonomy, MAX_ECHOED_INPUT_CHARS)
            )));
        }
        let ids = ids
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .map(|id| id.as_u64().filter(|n| *n > 0))
                    .collect::<Option<Vec<u64>>>()
            })
            .ok_or_else(|| invalid(format!("terms.{taxonomy} must be a list of term IDs")))?;
        terms.insert(taxonomy.clone(), ids);
    }
    Ok(Some(terms))
}

/// `post_type`の引数の説明で、許したカスタム投稿タイプを並べる前の文
/// （#684。複数のサイトでは、全サイトのものに置き換える）
pub(crate) const CUSTOM_POST_TYPES_NOTE: &str = "Custom post types allowed in the config";

/// ツールの`post_type`の引数（`enum`が`post`・`page`のもの）に、設定で
/// 許したカスタム投稿タイプを足す（#642）。AIには、使える型だけを見せる
fn with_allowed_post_types(tool: &mut Tool, allowed: &[String]) {
    if allowed.is_empty() {
        return;
    }
    // get_postsは、カスタム投稿タイプを許したときだけ`post_type`を持つ
    if tool.name == "get_posts" {
        if let Some(properties) = tool
            .input_schema
            .get_mut("properties")
            .and_then(|p| p.as_object_mut())
        {
            properties.insert(
                "post_type".to_string(),
                serde_json::json!({
                    "type": "string",
                    "enum": ["post"],
                    "description": "Post type to list. Defaults to 'post'.",
                    "default": "post"
                }),
            );
        }
    }
    let Some(post_type) = tool
        .input_schema
        .get_mut("properties")
        .and_then(|p| p.get_mut("post_type"))
        .and_then(|p| p.as_object_mut())
    else {
        return;
    };
    let Some(values) = post_type.get_mut("enum").and_then(|e| e.as_array_mut()) else {
        return;
    };
    for name in allowed {
        values.push(serde_json::json!(name));
    }
    let description = post_type
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("Content type")
        .to_string();
    post_type.insert(
        "description".to_string(),
        serde_json::json!(format!(
            "{description} {CUSTOM_POST_TYPES_NOTE}: {}",
            allowed.join(", ")
        )),
    );
}

/// 引数`key`の、正の整数のIDを読む（#639）
fn positive_id_arg(
    args: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<Option<u64>, McpError> {
    match args.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|n| *n > 0)
            .map(Some)
            .ok_or_else(|| McpError::InvalidParams(format!("{key} must be a positive integer"))),
    }
}

/// コメントを1行で表す（#639）。本文は、タグを除いて200文字まで
fn comment_summary(comment: &WordPressComment) -> String {
    let content = comment
        .content
        .get("rendered")
        .map(|c| truncate_chars_with_ellipsis(html_to_plain_text(c).trim(), 200))
        .unwrap_or_default();
    format!(
        "ID {} on post {}{} (Status: {}, Author: {}, Date: {}): {}",
        display_opt_id(comment.id),
        comment.post,
        if comment.parent > 0 {
            format!(", reply to {}", comment.parent)
        } else {
            String::new()
        },
        comment.status.as_deref().unwrap_or("unknown"),
        comment.author_name,
        comment.date.as_deref().unwrap_or("unknown"),
        content
    )
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
    /// URLのスラッグ。`None`のときは送らず、WordPressがタイトルから作る
    pub slug: Option<String>,
    /// タクソノミーの項目（#642）。キーはタクソノミーの`rest_base`
    /// （例: `product_cat`）、値は項目のID
    pub terms: Option<std::collections::BTreeMap<String, Vec<u64>>>,
}

/// WordPress post update parameters
#[derive(Debug, Clone, Default)]
pub struct PostUpdateParams {
    pub title: Option<String>,
    pub content: Option<String>,
    pub status: Option<String>,
    /// 公開日時（ISO8601、サイトのタイムゾーン、#625）。未来の日時と
    /// `status: "future"`で、既存の投稿を予約投稿にする
    pub date: Option<String>,
    pub categories: Option<Vec<u64>>,
    pub tags: Option<Vec<u64>>,
    pub featured_media_id: Option<u64>,
    pub meta: Option<HashMap<String, String>>,
    /// URLのスラッグ。WordPressはタイトルを変えてもスラッグを変えない
    pub slug: Option<String>,
    /// タクソノミーの項目（#642）。`PostCreateParams::terms`と同じ。渡した
    /// タクソノミーの項目を、この一覧に置き換える
    pub terms: Option<std::collections::BTreeMap<String, Vec<u64>>>,
}

impl Default for PostCreateParams {
    fn default() -> Self {
        Self {
            title: String::new(),
            content: String::new(),
            post_type: "post".to_string(),
            // 意図しない公開を避けるため、既定は下書き
            status: "draft".to_string(),
            date: None,
            categories: None,
            tags: None,
            featured_media_id: None,
            meta: None,
            slug: None,
            terms: None,
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
    /// ツール呼び出しを記録する監査ログを差し替える。本体は、プラグイン等と
    /// 同じ監査ログ（設定の`[audit]`で書き込み先を決めたもの）を渡す（#416）。
    /// 既定はメモリにだけ残す監査ログ。
    pub fn with_audit_logger(mut self, audit_logger: Arc<AuditLogger>) -> Self {
        self.audit_logger = audit_logger;
        self
    }

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

        // 監査ログ（RateLimiterと同様、サイト接続単位でハンドラが保持する）
        let audit_logger = Arc::new(AuditLogger::with_defaults());

        // セキュア認証情報の作成
        let secure_credentials = Some(config.create_secure_credentials());

        let upload_allowed_dirs =
            canonicalize_upload_dirs(config.upload_allowed_dirs.as_deref().unwrap_or_default())?;
        if !upload_allowed_dirs.is_empty() {
            info!(
                "upload_media file_path is enabled for: {:?}",
                upload_allowed_dirs
            );
        }

        let allowed_post_types = config.post_types.clone().unwrap_or_default();
        for post_type in &allowed_post_types {
            if !is_slug(post_type) || matches!(post_type.as_str(), "post" | "page") {
                return Err(format!(
                    "post_types: '{post_type}' is not a custom post type name (lowercase letters, digits, '_' and '-'; post and page are always allowed)"
                ));
            }
        }

        Ok(Self {
            client,
            base_url: config.url.clone(),
            secure_credentials,
            username: Some(config.username),
            password: Some(config.password),
            rate_limiter,
            audit_logger,
            upload_allowed_dirs: Arc::new(upload_allowed_dirs),
            allowed_post_types: Arc::new(allowed_post_types),
            type_endpoints: Default::default(),
        })
    }

    /// ツール呼び出しの結果を監査ログに記録する。成功/失敗どちらも記録する
    /// （インシデント調査では失敗した変更の試みも重要なため）。読み取りの
    /// ツールも記録する（#55。誰が何を読んだかも分かるように）。呼び出し元と
    /// 所要時間も記録する。引数（投稿の本文等）は記録しない。
    async fn audit_tool_call(
        &self,
        tool_name: &str,
        target_id: Option<&str>,
        result: &Result<serde_json::Value, McpError>,
        started: std::time::Instant,
    ) {
        let success = result.is_ok();
        let level = if success {
            AuditLevel::Info
        } else {
            AuditLevel::Error
        };
        let message = format!(
            "WordPress tool call: {} - {}",
            tool_name,
            if success { "SUCCESS" } else { "FAILURE" }
        );

        let mut entry = AuditLogEntry::new(level, AuditCategory::DataAccess, message)
            .with_action(self.base_url.clone(), tool_name.to_string())
            .with_result(if success { "SUCCESS" } else { "FAILURE" }.to_string());

        if let Some(target_id) = target_id {
            entry = entry.add_metadata("target_id".to_string(), target_id.to_string());
        }
        if let Err(e) = result {
            entry = entry.add_metadata("error".to_string(), e.to_string());
        }
        let entry = crate::mcp::audit_tool_call_context(entry, started);

        if let Err(e) = self.audit_logger.log(entry).await {
            warn!("Failed to write audit log entry for {}: {}", tool_name, e);
        }
    }

    /// リクエストを送り、成功（2xx）の応答を返す。一時的な失敗は最大
    /// `MAX_RETRIES`回まで再送するが、再送するかどうかは失敗の種類と
    /// HTTPメソッドで決める（#388）。
    ///
    /// - 接続できなかった・429: リクエストはWordPressで処理されていない
    ///   ため、メソッドを問わず再送する
    /// - タイムアウト・送信後のエラー・5xx: WordPressで処理済みの可能性が
    ///   ある。GET・PUT等の繰り返しても結果が変わらないメソッドだけ再送し、
    ///   POST（作成の重複）・DELETE（2回目が404）は再送しない
    ///
    /// `build_request`は試行ごとに呼ばれる。`multipart::Form`を使う
    /// リクエストは`try_clone()`で複製できない（内部でストリームを保持して
    /// いる）ため、作り直せる形で受け取る。
    async fn send_with_retry(
        &self,
        build_request: impl Fn() -> Result<reqwest::RequestBuilder, McpError>,
    ) -> Result<reqwest::Response, McpError> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_millis(1000);

        for attempt in 1..=MAX_RETRIES {
            let request = build_request()?.build().map_err(McpError::Http)?;
            let method = request.method().clone();
            let resend_safe = is_safe_to_resend(&method);
            let is_last = attempt == MAX_RETRIES;

            match self.client.execute(request).await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response);
                    }

                    let transient = is_retryable_status(status);
                    let retry = status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || (transient && resend_safe);
                    if retry && !is_last {
                        warn!(
                            "HTTP error {} on attempt {} ({}), retrying...",
                            status, attempt, method
                        );
                    } else {
                        // 本文のcode/messageを診断用に含める
                        // （例: rest_post_invalid_id）
                        let body = read_error_body(response).await;
                        let mut message = format_wp_error(status, &body);
                        if attempt > 1 {
                            message.push_str(&format!(" (after {} attempts)", attempt));
                        }
                        if transient && !retry {
                            message.push_str(NOT_RESENT_NOTE);
                        }
                        return Err(McpError::ExternalApi(message));
                    }
                }
                Err(e) => {
                    // 接続できなかった場合はリクエストが届いていない
                    let retry = e.is_connect() || resend_safe;
                    if e.is_timeout() {
                        warn!("Request timeout on attempt {} ({}): {}", attempt, method, e);
                    } else if e.is_connect() {
                        warn!(
                            "Connection error on attempt {} ({}): {}",
                            attempt, method, e
                        );
                    } else {
                        warn!("Request error on attempt {} ({}): {}", attempt, method, e);
                    }

                    if !retry {
                        return Err(McpError::ExternalApi(format!(
                            "Request failed: {}{}",
                            e, NOT_RESENT_NOTE
                        )));
                    }
                    if is_last {
                        return Err(McpError::Http(e));
                    }
                }
            }

            // リトライ前に少し待機
            tokio::time::sleep(RETRY_DELAY * attempt).await;
        }

        unreachable!()
    }

    /// リトライ機能付きでHTTPリクエストを実行し、応答をJSONとして解析する
    async fn execute_request_with_retry<T>(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<T, McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let response = self
            .send_with_retry(|| {
                request_builder
                    .try_clone()
                    .ok_or_else(|| McpError::Other("Failed to clone request".to_string()))
            })
            .await?;
        // 本文はログに出さない。context=editの応答には投稿の
        // 閲覧パスワードや未公開の下書き等が含まれうるため
        let text = read_body_limited(response, MAX_RESPONSE_BYTES).await?;
        debug!("Response body: {} bytes", text.len());
        parse_success_body(&text)
    }

    /// `execute_request_with_retry`と同じだが、リクエストを`try_clone()`で
    /// 複製するのではなく、`build_request`を呼ぶたびに作り直す。
    /// `multipart::Form`を使うリクエストは`try_clone()`が常に`None`を
    /// 返すため（内部でストリームを保持しており複製不可）、こちらを使う。
    async fn execute_request_with_retry_rebuilding<T>(
        &self,
        build_request: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<T, McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let response = self.send_with_retry(|| Ok(build_request())).await?;
        let text = read_body_limited(response, MAX_RESPONSE_BYTES).await?;
        parse_success_body(&text)
    }

    /// `execute_request_with_retry`と同じリトライ処理に加え、
    /// WordPress REST APIのページネーションヘッダー（`X-WP-TotalPages`）
    /// を読み取って返す。一覧系エンドポイントの全件取得に使う。最後の値は、
    /// 読んだ本文の大きさ（バイト）
    async fn execute_request_with_retry_capturing_total_pages<T>(
        &self,
        request_builder: reqwest::RequestBuilder,
    ) -> Result<(T, u32, u64, usize), McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let response = self
            .send_with_retry(|| {
                request_builder
                    .try_clone()
                    .ok_or_else(|| McpError::Other("Failed to clone request".to_string()))
            })
            .await?;
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
        let text = read_body_limited(response, MAX_RESPONSE_BYTES).await?;
        let data = parse_success_body(&text)?;
        Ok((data, total_pages, total_items, text.len()))
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
        let mut read_bytes: usize = 0;

        let total_items: u64 = loop {
            let url = url_for_page(page);
            let mut request = self.client.get(&url);

            if let (Some(username), Some(password)) = (&self.username, &self.password) {
                request = request.basic_auth(username, Some(password));
            }

            let (items, total_pages, reported_total, bytes): (Vec<T>, u32, u64, usize) = self
                .execute_request_with_retry_capturing_total_pages(request)
                .await?;

            all_items.extend(items);
            read_bytes += bytes;
            if read_bytes > MAX_LIST_BYTES {
                return Err(McpError::ExternalApi(format!(
                    "the WordPress list is larger than {} MB in total, the most mcp-rs reads for one list",
                    MAX_LIST_BYTES / (1024 * 1024)
                )));
            }

            if page >= total_pages.max(1) {
                break reported_total;
            }
            // `X-WP-TotalPages`をそのまま信じない（#598）
            if page >= MAX_LIST_PAGES {
                warn!(
                    "fetch_all_pages: WordPress reports {} pages; read only the first {} ({} items)",
                    total_pages,
                    MAX_LIST_PAGES,
                    all_items.len()
                );
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

    /// 投稿の一覧。`status`が無ければ、公開済みの投稿だけ（WordPressの
    /// 既定）。`future`・`draft`等や`any`、カンマ区切りで、予約済み・下書き
    /// の投稿も取れる（#625）
    async fn get_posts(&self, status: Option<&str>) -> Result<Vec<WordPressPost>, McpError> {
        let query = PostListQuery {
            status: status.map(str::to_string),
            ..Default::default()
        };
        Ok(self.list("posts", &query).await?.0)
    }

    /// Get all WordPress pages
    async fn get_pages(&self) -> Result<Vec<WordPressPost>, McpError> {
        Ok(self.list("pages", &PostListQuery::default()).await?.0)
    }

    /// 投稿（`posts`）・固定ページ（`pages`）の一覧（#637）。検索・絞り込み・
    /// ページを指定していれば、そのページだけを読み、ページの情報も返す。
    /// 指定していなければ、全件を読む（`fetch_all_pages`の上限まで）
    async fn list(
        &self,
        endpoint: &str,
        query: &PostListQuery,
    ) -> Result<(Vec<WordPressPost>, Option<ListPage>), McpError> {
        info!(
            "Fetching WordPress {} from: {}/wp-json/wp/v2/{}",
            endpoint, self.base_url, endpoint
        );
        let filters = query.query_string();
        let filters = if filters.is_empty() {
            String::new()
        } else {
            format!("&{filters}")
        };
        if !query.is_single_page() {
            let (items, _total) = self
                .fetch_all_pages(|page| {
                    format!(
                        "{}/wp-json/wp/v2/{}?per_page=100&page={}{}",
                        self.base_url, endpoint, page, filters
                    )
                })
                .await?;
            return Ok((items, None));
        }
        let per_page = query.per_page.unwrap_or(DEFAULT_LIST_PER_PAGE);
        let page = query.page.unwrap_or(1);
        let url = format!(
            "{}/wp-json/wp/v2/{}?per_page={}&page={}{}",
            self.base_url, endpoint, per_page, page, filters
        );
        let mut request = self.client.get(&url);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        let (items, total_pages, total, _bytes): (Vec<WordPressPost>, u32, u64, usize) = self
            .execute_request_with_retry_capturing_total_pages(request)
            .await?;
        Ok((
            items,
            Some(ListPage {
                page,
                per_page,
                total_pages,
                total,
            }),
        ))
    }

    /// Get both posts and pages
    pub async fn get_all_content(
        &self,
    ) -> Result<(Vec<WordPressPost>, Vec<WordPressPost>), McpError> {
        let posts_future = self.get_posts(None);
        let pages_future = self.get_pages();

        let (posts, pages) = tokio::try_join!(posts_future, pages_future)?;

        Ok((posts, pages))
    }

    /// 投稿・固定ページを1件取得する。
    ///
    /// `include_raw`が真なら、編集用の原文（`raw`）を含めるため
    /// `context=edit`で取得し、拒否された場合のみ表示用コンテキストで
    /// 取り直す（その場合`raw`は`None`になる）。認証情報が無い場合は
    /// `context=edit`が必ず拒否されるため、最初から表示用コンテキストで
    /// 取得する。
    pub async fn get_post(
        &self,
        post_id: u64,
        post_type: &str,
        include_raw: bool,
    ) -> Result<WordPressPost, McpError> {
        let endpoint = self.post_type_endpoint(post_type).await?;
        let url = format!("{}/wp-json/wp/v2/{}/{}", self.base_url, endpoint, post_id);

        info!("Fetching WordPress {}: {}", post_type, post_id);
        let has_credentials = self.username.is_some() && self.password.is_some();
        if !include_raw || !has_credentials {
            return self.fetch_post(&url, None).await;
        }

        match self.fetch_post(&url, Some("edit")).await {
            Err(edit_error) if is_edit_context_denied(&edit_error) => {
                warn!(
                    "Fetching {} {} with context=edit was denied, falling back to the view context: {}",
                    post_type, post_id, edit_error
                );
                self.fetch_post(&url, None).await
            }
            result => result,
        }
    }

    async fn fetch_post(
        &self,
        url: &str,
        context: Option<&str>,
    ) -> Result<WordPressPost, McpError> {
        let mut request = self.client.get(url);
        if let Some(context) = context {
            request = request.query(&[("context", context)]);
        }
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
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
        validate_post_status(&params.status, params.date.as_deref())?;

        // 投稿タイプに応じてエンドポイントを決定
        let endpoint = self.post_type_endpoint(&params.post_type).await?;
        self.check_terms_taxonomies(&params.post_type, params.terms.as_ref())
            .await?;
        let url = format!("{}/wp-json/wp/v2/{}", self.base_url, endpoint);
        let post_data = advanced_post_body(params);

        let mut request = self.client.post(&url);

        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }

        let response = request.json(&post_data).send().await?;

        let status = response.status();
        if !status.is_success() {
            // WordPressのエラーコード（empty_content等）を含めないと、
            // 失敗の原因が分からない
            let body = read_error_body(response).await;
            return Err(McpError::ExternalApi(format_wp_error(status, &body)));
        }

        let text = read_body_limited(response, MAX_RESPONSE_BYTES).await?;
        let created_post: WordPressPost = serde_json::from_str(&text)?;
        Ok(created_post)
    }

    /// コメントの一覧。`status`を渡せば、その状態のコメント（#639）。
    /// 渡さなければ、WordPressの既定（承認済み）
    async fn get_comments(
        &self,
        post_id: Option<u64>,
        status: Option<&str>,
    ) -> Result<Vec<WordPressComment>, McpError> {
        let (items, _total) = self
            .fetch_all_pages(|page| {
                let mut url = format!(
                    "{}/wp-json/wp/v2/comments?per_page=100&page={}",
                    self.base_url, page
                );
                if let Some(post_id) = post_id {
                    url = format!("{}&post={}", url, post_id);
                }
                if let Some(status) = status {
                    url = format!("{}&status={}", url, status);
                }
                url
            })
            .await?;
        Ok(items)
    }

    /// コメントを1件読む（#639）
    async fn get_comment(&self, comment_id: u64) -> Result<WordPressComment, McpError> {
        let url = format!("{}/wp-json/wp/v2/comments/{}", self.base_url, comment_id);
        let mut request = self.client.get(&url);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        self.execute_request_with_retry(request).await
    }

    /// コメントを作る（返信は`parent`付き）（#639）。書いた人は、設定した
    /// WordPressのユーザーになる
    async fn create_comment(
        &self,
        post_id: u64,
        parent: Option<u64>,
        content: &str,
        status: &str,
    ) -> Result<WordPressComment, McpError> {
        let url = format!("{}/wp-json/wp/v2/comments", self.base_url);
        let mut body = serde_json::json!({
            "post": post_id,
            "content": content,
            "status": status,
        });
        if let Some(parent) = parent {
            body["parent"] = serde_json::json!(parent);
        }
        let mut request = self.client.post(&url).json(&body);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        info!(
            "Creating WordPress comment on post {} (parent: {:?}, status: {})",
            post_id, parent, status
        );
        self.execute_request_with_retry(request).await
    }

    /// コメントの状態・本文を変える（#639）
    async fn update_comment(
        &self,
        comment_id: u64,
        status: Option<&str>,
        content: Option<&str>,
    ) -> Result<WordPressComment, McpError> {
        let url = format!("{}/wp-json/wp/v2/comments/{}", self.base_url, comment_id);
        let mut body = serde_json::Map::new();
        if let Some(status) = status {
            body.insert("status".to_string(), serde_json::json!(status));
        }
        if let Some(content) = content {
            body.insert("content".to_string(), serde_json::json!(content));
        }
        let mut request = self.client.post(&url).json(&body);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        info!(
            "Updating WordPress comment {} (status: {:?}, content: {})",
            comment_id,
            status,
            content.is_some()
        );
        self.execute_request_with_retry(request).await
    }

    /// コメントを消す（#639）。`force`が偽ならゴミ箱へ
    async fn delete_comment(
        &self,
        comment_id: u64,
        force: bool,
    ) -> Result<serde_json::Value, McpError> {
        let url = format!("{}/wp-json/wp/v2/comments/{}", self.base_url, comment_id);
        let mut request = self.client.delete(&url);
        if force {
            request = request.query(&[("force", "true")]);
        }
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        info!(
            "Deleting WordPress comment {} (force: {})",
            comment_id, force
        );
        self.execute_request_with_retry(request).await
    }

    /// 投稿タイプのREST APIの場所（#642）。`post`・`page`はそのまま。
    /// カスタム投稿タイプは、設定の`post_types`にあるものだけを、サイトの
    /// `/wp/v2/types/<名前>`の`rest_base`で（`wp/v2`にあるものだけ）
    async fn post_type_endpoint(&self, post_type: &str) -> Result<String, McpError> {
        match post_type {
            "post" => return Ok("posts".to_string()),
            "page" => return Ok("pages".to_string()),
            _ => {}
        }
        if !is_slug(post_type) {
            return Err(McpError::InvalidParams(format!(
                "post_type '{}' is not a post type name",
                truncate_chars_with_ellipsis(post_type, MAX_ECHOED_INPUT_CHARS)
            )));
        }
        if !self.allowed_post_types.iter().any(|t| t == post_type) {
            return Err(McpError::InvalidParams(format!(
                "post_type '{post_type}' is not allowed: mcp-rs uses only post, page{}. To use it, add it to post_types in [handlers.wordpress] (see get_post_types)",
                self.allowed_post_types
                    .iter()
                    .map(|t| format!(", {t}"))
                    .collect::<String>()
            )));
        }
        if let Some(endpoint) = self.type_endpoints.lock().await.get(post_type) {
            return Ok(endpoint.clone());
        }
        let url = format!("{}/wp-json/wp/v2/types/{}", self.base_url, post_type);
        let info: serde_json::Value = self.fetch_json(&url, None).await.map_err(|e| {
            McpError::ExternalApi(format!(
                "post type '{post_type}' was not found in the site's REST API: {e}"
            ))
        })?;
        let namespace = info
            .get("rest_namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("wp/v2");
        let endpoint = info
            .get("rest_base")
            .and_then(|v| v.as_str())
            .filter(|base| is_slug(base) && namespace == "wp/v2")
            .ok_or_else(|| {
                McpError::ExternalApi(format!(
                    "post type '{post_type}' is not available under /wp/v2 of the REST API (namespace '{namespace}'), so mcp-rs cannot use it"
                ))
            })?
            .to_string();
        self.type_endpoints
            .lock()
            .await
            .insert(post_type.to_string(), endpoint.clone());
        Ok(endpoint)
    }

    /// `terms`のタクソノミーが、投稿タイプに登録されているかを確かめる（#684）。
    /// WordPressは登録されていないタクソノミーの項目を黙って無視するため、
    /// 送る前に`/wp/v2/taxonomies?type=<投稿タイプ>`で調べて、無いものは断る。
    /// `terms`が空なら調べない
    async fn check_terms_taxonomies(
        &self,
        post_type: &str,
        terms: Option<&std::collections::BTreeMap<String, Vec<u64>>>,
    ) -> Result<(), McpError> {
        let Some(terms) = terms.filter(|terms| !terms.is_empty()) else {
            return Ok(());
        };
        let url = format!(
            "{}/wp-json/wp/v2/taxonomies?type={}",
            self.base_url, post_type
        );
        let taxonomies: serde_json::Map<String, serde_json::Value> =
            self.fetch_json(&url, None).await.map_err(|e| {
                McpError::ExternalApi(format!(
                    "could not check the taxonomies of post type '{post_type}' for terms: {e}"
                ))
            })?;
        let valid: Vec<&str> = taxonomies
            .values()
            .filter_map(|info| info.get("rest_base").and_then(|b| b.as_str()))
            .collect();
        if let Some(unknown) = terms.keys().find(|key| !valid.contains(&key.as_str())) {
            return Err(McpError::InvalidParams(format!(
                "terms: '{unknown}' is not a taxonomy of post type '{post_type}'. Valid: {}",
                if valid.is_empty() {
                    "none".to_string()
                } else {
                    valid.join(", ")
                }
            )));
        }
        Ok(())
    }

    /// 投稿の、独自のタクソノミーの項目（#642）。例: `product_cat: 12, 15`。
    /// タクソノミーが読めなければ`None`
    async fn custom_terms_label(&self, post_type: &str, post: &WordPressPost) -> Option<String> {
        let url = format!(
            "{}/wp-json/wp/v2/taxonomies?type={}",
            self.base_url, post_type
        );
        let taxonomies: serde_json::Map<String, serde_json::Value> =
            self.fetch_json(&url, None).await.ok()?;
        let parts: Vec<String> = taxonomies
            .values()
            .filter_map(|info| info.get("rest_base").and_then(|b| b.as_str()))
            .filter_map(|base| {
                let ids = post.extra.get(base)?.as_array()?;
                let ids: Vec<String> = ids
                    .iter()
                    .filter_map(|id| id.as_u64())
                    .map(|id| id.to_string())
                    .collect();
                Some(format!(
                    "{base}: {}",
                    if ids.is_empty() {
                        "none".to_string()
                    } else {
                        ids.join(", ")
                    }
                ))
            })
            .collect();
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    /// タクソノミーの項目の一覧（#642）。`taxonomy`は、タクソノミーの名前か
    /// `rest_base`。サイトの`/wp/v2/taxonomies`で、`wp/v2`の場所を確かめる
    async fn get_terms(&self, taxonomy: &str) -> Result<Vec<serde_json::Value>, McpError> {
        if !is_slug(taxonomy) {
            return Err(McpError::InvalidParams(format!(
                "taxonomy '{}' is not a taxonomy name",
                truncate_chars_with_ellipsis(taxonomy, MAX_ECHOED_INPUT_CHARS)
            )));
        }
        let taxonomies: serde_json::Map<String, serde_json::Value> = self
            .fetch_json(&format!("{}/wp-json/wp/v2/taxonomies", self.base_url), None)
            .await?;
        let rest_base = taxonomies
            .iter()
            .find_map(|(slug, info)| {
                let base = info.get("rest_base").and_then(|b| b.as_str())?;
                let namespace = info
                    .get("rest_namespace")
                    .and_then(|n| n.as_str())
                    .unwrap_or("wp/v2");
                (namespace == "wp/v2" && is_slug(base) && (slug == taxonomy || base == taxonomy))
                    .then(|| base.to_string())
            })
            .ok_or_else(|| {
                McpError::InvalidParams(format!(
                    "taxonomy '{taxonomy}' was not found under /wp/v2 of the site's REST API (see get_taxonomies)"
                ))
            })?;
        let (items, _total) = self
            .fetch_all_pages(|page| {
                format!(
                    "{}/wp-json/wp/v2/{}?per_page=100&page={}",
                    self.base_url, rest_base, page
                )
            })
            .await?;
        Ok(items)
    }

    /// 投稿・固定ページのリビジョンの一覧（#641）。新しい順
    async fn get_revisions(
        &self,
        post_id: u64,
        post_type: &str,
    ) -> Result<Vec<WordPressRevision>, McpError> {
        let endpoint = self.post_type_endpoint(post_type).await?;
        let (items, _total) = self
            .fetch_all_pages(|page| {
                format!(
                    "{}/wp-json/wp/v2/{}/{}/revisions?per_page=100&page={}",
                    self.base_url, endpoint, post_id, page
                )
            })
            .await?;
        Ok(items)
    }

    /// リビジョンを1つ、編集用の原文（`context=edit`）も含めて読む（#641）
    async fn get_revision(
        &self,
        post_id: u64,
        revision_id: u64,
        post_type: &str,
    ) -> Result<WordPressRevision, McpError> {
        let endpoint = self.post_type_endpoint(post_type).await?;
        let url = format!(
            "{}/wp-json/wp/v2/{}/{}/revisions/{}",
            self.base_url, endpoint, post_id, revision_id
        );
        let revision: WordPressRevision = self.fetch_json(&url, Some("edit")).await?;
        // 別の投稿のリビジョンを、この投稿に書き戻さない
        if revision.parent != 0 && revision.parent != post_id {
            return Err(McpError::InvalidParams(format!(
                "revision {} belongs to {} {}, not {}",
                revision_id, post_type, revision.parent, post_id
            )));
        }
        Ok(revision)
    }

    /// 投稿を、リビジョンのタイトル・本文・抜粋に書き戻す（#641）。状態・
    /// 公開日時・スラッグ等は変えない。リビジョンが有効なサイトでは、
    /// WordPressが書き戻す前の内容をリビジョンとして残すので、たいていは
    /// 元に戻せる。サイトの設定（`WP_POST_REVISIONS`で無効・1件だけ等）に
    /// よっては残らないので、必ず残したいときは先に`get_post`で読んでおく（#684）
    async fn restore_revision(
        &self,
        post_id: u64,
        revision_id: u64,
        post_type: &str,
    ) -> Result<(WordPressRevision, WordPressPost), McpError> {
        let revision = self.get_revision(post_id, revision_id, post_type).await?;
        let raw = |content: &WordPressContent, field: &str| {
            content.raw.clone().ok_or_else(|| {
                McpError::ExternalApi(format!(
                    "WordPress did not return the {field} of revision {revision_id} as source: restoring needs credentials with edit permission"
                ))
            })
        };
        let mut body = serde_json::json!({
            "title": raw(&revision.title, "title")?,
            "content": raw(&revision.content, "content")?,
        });
        if let Some(excerpt) = revision.excerpt.as_ref().and_then(|e| e.raw.clone()) {
            body["excerpt"] = serde_json::json!(excerpt);
        }

        let endpoint = self.post_type_endpoint(post_type).await?;
        let url = format!("{}/wp-json/wp/v2/{}/{}", self.base_url, endpoint, post_id);
        let mut request = self.client.post(&url).json(&body);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        info!(
            "Restoring WordPress {} {} to revision {}",
            post_type, post_id, revision_id
        );
        let post = self.execute_request_with_retry(request).await?;
        Ok((revision, post))
    }

    /// 投稿者の名前をまとめて読む（#684）。重ならないIDごとに同時に聞き、
    /// 全体で`limit`（ふつうは`AUTHOR_NAMES_TIMEOUT`）までしか待たない。
    /// 読めなかった人は`user {ID}`にする
    async fn user_names(
        &self,
        user_ids: impl IntoIterator<Item = u64>,
        limit: Duration,
    ) -> HashMap<u64, String> {
        let mut ids: Vec<u64> = user_ids.into_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        let deadline = tokio::time::Instant::now() + limit;
        let names = futures::future::join_all(
            ids.iter()
                .map(|&id| tokio::time::timeout_at(deadline, self.user_name(id))),
        )
        .await;
        ids.into_iter()
            .zip(names)
            .map(|(id, name)| {
                let name = name.ok().flatten().unwrap_or_else(|| format!("user {id}"));
                (id, name)
            })
            .collect()
    }

    /// 投稿者の名前（#641）。読めなければ`None`
    async fn user_name(&self, user_id: u64) -> Option<String> {
        let url = format!("{}/wp-json/wp/v2/users/{}", self.base_url, user_id);
        let user: serde_json::Value = self.fetch_json(&url, None).await.ok()?;
        user.get("name")
            .and_then(|n| n.as_str())
            .map(str::to_string)
    }

    /// GETして、JSONとして読む。`context`があれば付ける
    async fn fetch_json<T>(&self, url: &str, context: Option<&str>) -> Result<T, McpError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let mut request = self.client.get(url);
        if let Some(context) = context {
            request = request.query(&[("context", context)]);
        }
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        self.execute_request_with_retry(request).await
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
        let mime_type = validate_upload(file_data, filename, mime_type)?;

        let url = format!("{}/wp-json/wp/v2/media", self.base_url);
        let title = filename_stem(filename).to_string();

        info!(
            "Uploading media file: {} ({})",
            truncate_chars_with_ellipsis(filename, MAX_ECHOED_INPUT_CHARS),
            mime_type
        );

        self.execute_request_with_retry_rebuilding(|| {
            let part = reqwest::multipart::Part::bytes(file_data.to_vec())
                .file_name(filename.to_string())
                .mime_str(mime_type)
                .expect("ALLOWED_UPLOADS contains only valid MIME types");
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
        let response: DeleteMediaResponse = self.execute_request_with_retry(request).await?;
        Ok(match response {
            DeleteMediaResponse::Deleted { previous, .. } => previous,
            DeleteMediaResponse::Trashed(media) => media,
        })
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

    /// 動画プレーヤーにできるYouTubeのURLかどうか。`extract_youtube_id`で
    /// 動画のIDを取り出せるものだけを受け付ける（#680）
    pub fn validate_youtube_url(url: &str) -> bool {
        Self::extract_youtube_id(url).is_some()
    }

    /// YouTubeのURLから動画のIDを取り出す（#680）。URLを分解し、ホストは
    /// 下の一覧と完全に一致するものだけを受け付ける（`https://example.org/youtube.com/…`
    /// のように、パスの中の文字列に引っかからないため）。受け付ける形は
    /// `youtube.com/watch?v=`（`v`は問い合わせのどこにあってもよい）・`youtu.be/`・
    /// `youtube.com/embed/`・`youtube.com/shorts/`・`youtube.com/live/`・
    /// `youtube-nocookie.com/embed/`。パスはこの形と同じ数の要素だけを受け付け
    /// （末尾の`/`はよい）、`/shorts/<ID>/extra`のような余分な要素があれば断る
    pub fn extract_youtube_id(url: &str) -> Option<String> {
        let parsed = parse_web_url(url)?;
        let host = parsed.host_str()?;
        let segments: Vec<&str> = parsed.path_segments()?.collect();
        // 末尾の`/`による空の要素を1つだけ外す
        let path = match segments.as_slice() {
            [rest @ .., ""] if !rest.is_empty() => rest,
            all => all,
        };
        if host == YOUTUBE_SHORT_LINK_HOST {
            return match path {
                [id] => youtube_video_id(id),
                _ => None,
            };
        }
        if YOUTUBE_NOCOOKIE_HOSTS.contains(&host) {
            return match path {
                ["embed", id] => youtube_video_id(id),
                _ => None,
            };
        }
        if !YOUTUBE_HOSTS.contains(&host) {
            return None;
        }
        match path {
            ["watch"] => parsed
                .query_pairs()
                .find(|(key, _)| key == "v")
                .and_then(|(_, id)| youtube_video_id(&id)),
            ["embed" | "shorts" | "live", id] => youtube_video_id(id),
            _ => None,
        }
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

    /// 本文に足すSNSのURLなら、そのサービスの名前を返す。URLを分解し、ホストが
    /// `SOCIAL_EMBED_HOSTS`と完全に一致し、パスが決まった形で始まるものだけを
    /// 受け付ける（#680。以前は文字列を含むかだけを見ていて、`netflix.com/`も
    /// `x.com/`として受け付けていた）
    pub fn validate_social_url(url: &str) -> Option<&'static str> {
        let parsed = parse_web_url(url)?;
        let host = parsed.host_str()?;
        SOCIAL_EMBED_HOSTS
            .iter()
            .find(|(_, hosts, path_prefix)| {
                hosts.contains(&host) && parsed.path().starts_with(path_prefix)
            })
            .map(|(service, _, _)| *service)
    }

    /// `create_post_with_embeds`が本文に足さないURL（受け付けない形のYouTubeの
    /// URLと、対応しないサービスのURL）。黙って捨てずに応答で知らせる（#680）
    fn skipped_embed_urls<'a>(youtube_urls: &[&'a str], social_urls: &[&'a str]) -> Vec<&'a str> {
        youtube_urls
            .iter()
            .filter(|url| !Self::validate_youtube_url(url))
            .chain(
                social_urls
                    .iter()
                    .filter(|url| Self::validate_social_url(url).is_none()),
            )
            .copied()
            .collect()
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
        // 動画のIDを取り出せないURLは足さない（`skipped_embed_urls`で応答に出す）
        for url in youtube_urls {
            if let Some(video_id) = Self::extract_youtube_id(url) {
                let embed = Self::generate_youtube_embed(&video_id, None, None);
                full_content.push_str(&format!("\n\n{}", embed));
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
            // post_type・statusはPostCreateParamsの既定（post・draft）
            let post_params = PostCreateParams {
                title: title.to_string(),
                content: full_content,
                ..Default::default()
            };
            self.create_advanced_post(post_params).await
        }
    }

    /// Create post with featured image
    ///
    /// 送信内容を独自に組み立てず、ほかの投稿作成系ツールと同じ
    /// `create_advanced_post`（`advanced_post_body`）を経由する。以前は
    /// 読み取り用の`WordPressPost`をそのまま送っており、タイトル・本文が
    /// WordPressに読まれず常にHTTP 400になっていた。
    ///
    /// 以前は`execute_request_with_retry`で5xx・429を再送していたが、
    /// `create_advanced_post`は再送しない。投稿の作成（POST）を再送すると、
    /// 1回目が実は成功していた場合に投稿が重複しうるため、意図的にそろえている。
    async fn create_post_with_featured_image(
        &self,
        title: String,
        content: String,
        featured_media_id: u64,
        status: String,
        slug: Option<String>,
    ) -> Result<WordPressPost, McpError> {
        info!("Creating post with featured image: {}", featured_media_id);
        self.create_advanced_post(PostCreateParams {
            title,
            content,
            status,
            featured_media_id: Some(featured_media_id),
            slug,
            ..Default::default()
        })
        .await
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
            slug: None,
            terms: None,
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
        let endpoint = self.post_type_endpoint(post_type).await?;
        self.check_terms_taxonomies(post_type, params.terms.as_ref())
            .await?;
        let url = format!("{}/wp-json/wp/v2/{}/{}", self.base_url, endpoint, post_id);

        info!("Updating WordPress {}: {}", post_type, post_id);

        // 日付の無い`future`は、すぐに公開されてしまう（#625）。既に予約
        // 済みの投稿（公開日時が未来）なら、日付を送らなくても予約のまま
        if params.status.as_deref() == Some("future") {
            let has_date = params.date.as_deref().is_some_and(|d| !d.trim().is_empty());
            if !has_date && self.get_post(post_id, post_type, false).await?.status != "future" {
                validate_post_status("future", None)?;
            }
        }

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

        if let Some(slug) = params.slug {
            update_data.insert("slug".to_string(), serde_json::Value::String(slug));
        }

        // タクソノミーの項目（#642）
        if let Some(terms) = params.terms {
            for (taxonomy, ids) in terms {
                update_data.insert(taxonomy, serde_json::json!(ids));
            }
        }

        if let Some(date) = params.date.filter(|d| !d.trim().is_empty()) {
            update_data.insert("date".to_string(), serde_json::Value::String(date));
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
        let endpoint = self.post_type_endpoint(post_type).await?;
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
        // サインインできないと、権限・メディアは確かめられない。どちらも
        // 認証の要らない読み取りでも通るので、確かめるとOKに見えてしまう
        // （PR #583で、誤ったパスワードで「権限OK」と出た）
        if !health.authentication_valid {
            return health;
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
    ///
    /// 届くかは、認証の要らないAPIのインデックス（`/wp-json/`）で見る
    /// （PR #568のレビュー）。以前は`/wp-json/wp/v2/settings`を見ていたが、
    /// これは管理者（`manage_options`）だけが読めるので、文書が勧める投稿者・
    /// 寄稿者のアカウントでは403になり、届いているのに「届かない」として
    /// 残りの段を飛ばしていた。認証情報は、要るときだけ付ける
    /// （`get_api_index`）。設定（管理者のメールアドレス等）は、読めたとき
    /// だけ足す
    async fn check_site_accessibility(&self) -> Result<WordPressSiteInfo, McpError> {
        let response = self
            .get_api_index()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to connect to WordPress: {}", e)))?;

        if !response.status().is_success() {
            return Err(McpError::ExternalApi(format!(
                "WordPress site not accessible. Status: {}",
                response.status()
            )));
        }

        let index: serde_json::Value = response
            .json()
            .await
            .map_err(|e| McpError::ExternalApi(format!("Failed to parse site info: {}", e)))?;
        let text = |value: &serde_json::Value, key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        let mut info = WordPressSiteInfo {
            name: text(&index, "name").unwrap_or_else(|| "Unknown".to_string()),
            description: text(&index, "description").unwrap_or_default(),
            url: text(&index, "url").unwrap_or_else(|| self.base_url.clone()),
            admin_email: None,
            timezone_string: text(&index, "timezone_string").filter(|s| !s.is_empty()),
            date_format: None,
            time_format: None,
            start_of_week: None,
        };

        // 管理者のアカウントなら、設定も読める。読めなくても誤りにしない
        if let Some(settings) = self.read_settings_for_health_check().await {
            if let Some(name) = text(&settings, "title") {
                info.name = name;
            }
            info.admin_email = text(&settings, "admin_email");
            info.timezone_string = text(&settings, "timezone_string")
                .filter(|s| !s.is_empty())
                .or(info.timezone_string);
            info.date_format = text(&settings, "date_format");
            info.time_format = text(&settings, "time_format");
            info.start_of_week = settings
                .get("start_of_week")
                .and_then(|v| v.as_u64())
                .map(|n| n as u8);
        }
        Ok(info)
    }

    /// APIのインデックス（`/wp-json/`）を読む。まず認証情報を付けずに読み、
    /// 401・403なら、認証情報を付けて読み直す。RESTをログインした人だけに
    /// 開くプラグインを入れたサイトでは、認証無しでは401になる（PR #568の
    /// レビュー）。最初から付けないのは、パスワードの誤りで「届かない」と
    /// ならないように（誤りは「サインイン」の段で知らせる）
    async fn get_api_index(&self) -> Result<reqwest::Response, reqwest::Error> {
        let url = format!("{}/wp-json/", self.base_url);
        let response = self.client.get(&url).send().await?;
        let status = response.status();
        if status != reqwest::StatusCode::UNAUTHORIZED && status != reqwest::StatusCode::FORBIDDEN {
            return Ok(response);
        }
        match (&self.username, &self.password) {
            (Some(username), Some(password)) => {
                self.client
                    .get(&url)
                    .basic_auth(username, Some(password))
                    .send()
                    .await
            }
            _ => Ok(response),
        }
    }

    /// 設定（`/wp-json/wp/v2/settings`）を読む。管理者でなければ読めないので、
    /// 読めなければ`None`
    async fn read_settings_for_health_check(&self) -> Option<serde_json::Value> {
        let url = format!("{}/wp-json/wp/v2/settings", self.base_url);
        let mut request = self.client.get(&url);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            request = request.basic_auth(username, Some(password));
        }
        let response = request.send().await.ok()?;
        if !response.status().is_success() {
            debug!(
                "Site settings are not readable (status {}); this needs an administrator account and is not an error",
                response.status()
            );
            return None;
        }
        let text = read_body_limited(response, MAX_RESPONSE_BYTES).await.ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Check if WordPress REST API is available
    ///
    /// ルートのAPIディスカバリーインデックス（`/wp-json/`）を見る。
    /// `/wp-json/wp/v2`はwp/v2名前空間自身のルート一覧
    /// （`{"namespace": "wp/v2", "routes": {...}}`）を返すだけで
    /// `"namespaces"`配列を持たないため、そちらを見ると常に
    /// 偽陰性になる。
    async fn check_rest_api(&self) -> Result<(), McpError> {
        let response = self
            .get_api_index()
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

impl WordPressHandler {
    /// ツール名に応じて各ツールを実行する。`call_tool`はこの結果を成功・
    /// 失敗を問わず監査ログに記録するため、各アーム内の`?`や`return`に
    /// よる早期リターンも監査を素通りしない（別メソッドに分けることで
    /// 構造的に保証する）。
    async fn dispatch_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
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

                // mcp-rs自身のバージョン（#434。詳しい状態はmcp_rs_status）
                report.push_str(&format!(
                    "🏷️ mcp-rs {} (call {} for details)\n",
                    env!("CARGO_PKG_VERSION"),
                    crate::mcp::STATUS_TOOL_NAME
                ));
                // ログのファイルのパスは、HTTPで認証していない呼び出し元には
                // 出さない（#543、`mcp_rs_status`と同じ）
                if crate::mcp::current_caller().is_some_and(|c| c.is_unauthenticated_remote()) {
                    report.push_str(
                        "📄 Log file: shown only to authenticated callers in HTTP mode\n\n",
                    );
                } else {
                    report.push_str(&format!("📄 Log file: {}\n\n", health.log_destination));
                }

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
            "get_posts" | "get_pages" => {
                let args = params.arguments.unwrap_or_default();
                let (endpoint, with_taxonomies) = if params.name == "get_posts" {
                    // カスタム投稿タイプ（#642）
                    let post_type = revision_post_type_arg(&args)?;
                    (self.post_type_endpoint(post_type).await?, true)
                } else {
                    ("pages".to_string(), false)
                };
                let query = post_list_query_arg(&args, with_taxonomies)?;
                let (items, page) = self.list(&endpoint, &query).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": list_result_text(&endpoint, &items, page)
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
                let format = PostContentFormat::parse(args.get("format"))?;

                let post = self
                    .get_post(post_id, post_type, format == PostContentFormat::Raw)
                    .await?;

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

                // アイキャッチ画像のURLを取得する。カテゴリ/タグ名と同じく、
                // 取得に失敗してもget_post自体は失敗させず、IDだけ表示する。
                let featured_media = match post.featured_media {
                    Some(media_id) if media_id > 0 => match self.get_media_item(media_id).await {
                        Ok(media) => {
                            featured_media_label(Some(media_id), media.source_url.as_deref())
                        }
                        Err(e) => {
                            warn!(
                                "Failed to resolve featured media {} for get_post: {}",
                                media_id, e
                            );
                            featured_media_label(Some(media_id), None)
                        }
                    },
                    other => featured_media_label(other, None),
                };

                // タイトルも本文と同じ形で返す（rawならHTMLエンティティ化
                // されていない原文）
                let title = post_content_for_display(&post.title, format);
                // 本文のない投稿タイプ（#684）では、空の本文として見せない
                let content_body = match &post.content {
                    Some(content) => post_content_for_display(content, format),
                    None => "(none)".to_string(),
                };
                let mut note = match &post.content {
                    Some(_) => protected_content_note(&post),
                    None => Some(NO_CONTENT_FIELD_NOTE),
                }
                .map(|note| format!("Note: {}\n", note))
                .unwrap_or_default();
                // カスタム投稿タイプの、独自のタクソノミーの項目（#642）
                if !matches!(post_type, "post" | "page") {
                    if let Some(terms) = self.custom_terms_label(post_type, &post).await {
                        note = format!("Terms: {terms}\n{note}");
                    }
                }

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Post Details:\nID: {}\nFormat: {}\nTitle: {}\nStatus: {}\nDate: {}\nSlug: {}\nCategories: {}\nTags: {}\nFeatured Media: {}\n{}Content:\n{}",
                            post.id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string()),
                            post_format_label(&post, format),
                            title,
                            post.status,
                            post_date_label(&post),
                            post_slug_label(&post),
                            categories_names.join(", "),
                            tags_names.join(", "),
                            featured_media,
                            note,
                            content_body
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
                let status = post_status_or_draft(&args)?;
                let slug = optional_slug_arg(&args)?;

                let post = self
                    .create_advanced_post(PostCreateParams {
                        title: title.to_string(),
                        content: content.to_string(),
                        status,
                        slug,
                        ..Default::default()
                    })
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post with ID: {} (Status: {}, Slug: {})",
                            post.id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string()),
                            post.status,
                            post_slug_label(&post))
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

                let status = post_status_or_draft(&args)?;

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

                let slug = optional_slug_arg(&args)?;

                let post = self
                    .create_advanced_post(PostCreateParams {
                        title: title.to_string(),
                        content: content.to_string(),
                        post_type: post_type.clone(),
                        status,
                        date,
                        categories,
                        tags,
                        featured_media_id,
                        meta,
                        slug,
                        terms: terms_arg(&args)?,
                    })
                    .await?;

                // 要求した値ではなく、WordPressが実際に設定したステータスを
                // 示す（権限不足でpendingに落とされる場合等がある）
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Created {} with ID: {} (Status: {}, Slug: {})",
                            if post_type == "page" { "page" } else { "post" },
                            display_opt_id(post.id),
                            post.status,
                            post_slug_label(&post)
                        )
                    }],
                    "isError": false
                }))
            }
            "get_comments" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = args.get("post_id").and_then(|v| v.as_u64());
                let status = comment_status_arg(&args, "status", COMMENT_LIST_STATUSES)?;

                let comments = self.get_comments(post_id, status.as_deref()).await?;
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
                let arg_str = |key: &str| args.get(key).and_then(|v| v.as_str());

                let (file_data, filename, mime_type) = match (
                    arg_str("file_data"),
                    arg_str("file_path"),
                ) {
                    (Some(_), Some(_)) => {
                        return Err(McpError::InvalidParams(
                            "Specify either file_data or file_path, not both".to_string(),
                        ));
                    }
                    (None, None) => {
                        return Err(McpError::InvalidParams(
                            "Missing file_data or file_path".to_string(),
                        ));
                    }
                    (Some(file_data_b64), None) => {
                        let filename = arg_str("filename").ok_or_else(|| {
                            McpError::InvalidParams("Missing filename".to_string())
                        })?;
                        let mime_type = arg_str("mime_type").ok_or_else(|| {
                            McpError::InvalidParams("Missing mime_type".to_string())
                        })?;

                        if file_data_b64.len() > MAX_UPLOAD_BASE64_LEN {
                            return Err(McpError::InvalidParams(format!(
                                "file_data too large: {} base64 chars (max {} bytes decoded)",
                                file_data_b64.len(),
                                MAX_UPLOAD_SIZE_BYTES
                            )));
                        }

                        use base64::{engine::general_purpose, Engine as _};
                        let file_data =
                            general_purpose::STANDARD
                                .decode(file_data_b64)
                                .map_err(|e| {
                                    McpError::InvalidParams(format!("Invalid base64 data: {}", e))
                                })?;
                        (file_data, filename.to_string(), mime_type.to_string())
                    }
                    (None, Some(file_path)) => {
                        let path = resolve_upload_path(file_path, &self.upload_allowed_dirs)?;
                        let file_data = read_upload_file(&path).await?;
                        // 省略時はファイル名・拡張子から決める。中身は
                        // いずれの場合もvalidate_uploadで検証する
                        let filename = match arg_str("filename") {
                            Some(name) => name.to_string(),
                            None => path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        };
                        let mime_type = match arg_str("mime_type") {
                                Some(mime_type) => mime_type.to_string(),
                                None => mime_type_for_filename(&filename)
                                    .ok_or_else(|| {
                                        McpError::InvalidParams(format!(
                                            "Cannot determine mime_type from the file extension. Allowed types: {}",
                                            allowed_upload_mime_list()
                                        ))
                                    })?
                                    .to_string(),
                            };
                        (file_data, filename, mime_type)
                    }
                };

                let media = self.upload_media(&file_data, &filename, &mime_type).await?;
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
                let status = post_status_or_draft(&args)?;
                let slug = optional_slug_arg(&args)?;

                let post = self
                    .create_post_with_featured_image(
                        title.to_string(),
                        content.to_string(),
                        featured_media_id,
                        status,
                        slug,
                    )
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post with featured image. Post ID: {}, Status: {}, Slug: {}", display_opt_id(post.id), post.status, post_slug_label(&post))
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

                let status = post_status_or_draft(&args)?;

                let categories: Option<Vec<u64>> = args
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let tags: Option<Vec<u64>> = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect());

                let featured_media_id = args.get("featured_media_id").and_then(|v| v.as_u64());
                let slug = optional_slug_arg(&args)?;

                let post = self
                    .create_advanced_post(PostCreateParams {
                        title: title.to_string(),
                        content: content.to_string(),
                        status,
                        categories,
                        tags,
                        featured_media_id,
                        slug,
                        ..Default::default()
                    })
                    .await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created post '{}' with ID: {} (Status: {}, Slug: {}), categories: {}, tags: {}",
                            title,
                            post.id.map(|id| id.to_string()).unwrap_or_else(|| "N/A".to_string()),
                            post.status,
                            post_slug_label(&post),
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
                let date = args
                    .get("date")
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
                let slug = optional_slug_arg(&args)?;

                let post = self
                    .update_post(
                        post_id,
                        post_type,
                        PostUpdateParams {
                            title,
                            content,
                            status,
                            date,
                            categories,
                            tags,
                            featured_media_id,
                            meta: None,
                            slug,
                            terms: terms_arg(&args)?,
                        },
                    )
                    .await?;

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated {} ID {} - Title: '{}', Status: {}, Date: {}, Slug: {}",
                            post_type, post_id, post.title.rendered, post.status, post_date_label(&post), post_slug_label(&post))
                    }],
                    "isError": false
                }))
            }
            "get_post_types" => {
                let types: serde_json::Map<String, serde_json::Value> = self
                    .fetch_json(&format!("{}/wp-json/wp/v2/types", self.base_url), None)
                    .await?;
                let mut lines = Vec::new();
                for (slug, info) in &types {
                    let allowed = matches!(slug.as_str(), "post" | "page")
                        || self.allowed_post_types.iter().any(|t| t == slug);
                    let taxonomies = info
                        .get("taxonomies")
                        .and_then(|t| t.as_array())
                        .map(|t| {
                            t.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    lines.push(format!(
                        "{} ({}): {}; taxonomies: {}",
                        slug,
                        info.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                        if allowed {
                            "usable"
                        } else {
                            "not allowed (add it to post_types in [handlers.wordpress] to use it)"
                        },
                        if taxonomies.is_empty() {
                            "none"
                        } else {
                            &taxonomies
                        }
                    ));
                }
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} post types:\n{}", lines.len(), lines.join("\n"))
                    }],
                    "isError": false
                }))
            }
            "get_taxonomies" => {
                let args = params.arguments.unwrap_or_default();
                let mut url = format!("{}/wp-json/wp/v2/taxonomies", self.base_url);
                if let Some(post_type) = args.get("post_type").and_then(|v| v.as_str()) {
                    if !is_slug(post_type) {
                        return Err(McpError::InvalidParams(
                            "post_type is not a post type name".to_string(),
                        ));
                    }
                    url = format!("{url}?type={post_type}");
                }
                let taxonomies: serde_json::Map<String, serde_json::Value> =
                    self.fetch_json(&url, None).await?;
                let lines: Vec<String> = taxonomies
                    .iter()
                    .map(|(slug, info)| {
                        let text = |key: &str| {
                            info.get(key)
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string()
                        };
                        let types = info
                            .get("types")
                            .and_then(|t| t.as_array())
                            .map(|t| {
                                t.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();
                        format!(
                            "{} ({}): rest_base {} (the key in terms and in posts), hierarchical: {}, post types: {}",
                            slug,
                            text("name"),
                            text("rest_base"),
                            info.get("hierarchical")
                                .and_then(|h| h.as_bool())
                                .unwrap_or(false),
                            types
                        )
                    })
                    .collect();
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} taxonomies:\n{}", lines.len(), lines.join("\n"))
                    }],
                    "isError": false
                }))
            }
            "get_terms" => {
                let args = params.arguments.unwrap_or_default();
                let taxonomy = args
                    .get("taxonomy")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::InvalidParams("Missing taxonomy".to_string()))?;
                let terms = self.get_terms(taxonomy).await?;
                let lines: Vec<String> = terms
                    .iter()
                    .map(|term| {
                        let number =
                            |key: &str| term.get(key).and_then(|v| v.as_u64()).unwrap_or(0);
                        let text = |key: &str| term.get(key).and_then(|v| v.as_str()).unwrap_or("");
                        format!(
                            "ID {}: {} (slug: {}, parent: {}, posts: {})",
                            number("id"),
                            text("name"),
                            text("slug"),
                            number("parent"),
                            number("count")
                        )
                    })
                    .collect();
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Found {} terms in {}:\n{}", lines.len(), taxonomy, lines.join("\n"))
                    }],
                    "isError": false
                }))
            }
            "get_revisions" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = positive_id_arg(&args, "post_id")?
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let post_type = revision_post_type_arg(&args)?;

                let revisions = self.get_revisions(post_id, post_type).await?;
                let authors = self
                    .user_names(
                        revisions
                            .iter()
                            .map(|revision| revision.author)
                            .filter(|author| *author > 0),
                        AUTHOR_NAMES_TIMEOUT,
                    )
                    .await;
                let lines: Vec<String> = revisions
                    .iter()
                    .map(|revision| {
                        let author = authors
                            .get(&revision.author)
                            .map(String::as_str)
                            .unwrap_or("unknown");
                        revision_summary(revision, author)
                    })
                    .collect();
                let text = if lines.is_empty() {
                    format!("{post_type} {post_id} has no revisions.")
                } else {
                    format!(
                        "Found {} revisions of {} {} (newest first):\n{}",
                        lines.len(),
                        post_type,
                        post_id,
                        lines.join("\n")
                    )
                };
                Ok(serde_json::json!({
                    "content": [{"type": "text", "text": text}],
                    "isError": false
                }))
            }
            "get_revision" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = positive_id_arg(&args, "post_id")?
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let revision_id = positive_id_arg(&args, "revision_id")?
                    .ok_or_else(|| McpError::InvalidParams("Missing revision_id".to_string()))?;
                let post_type = revision_post_type_arg(&args)?;
                let format = PostContentFormat::parse(args.get("format"))?;

                let revision = self.get_revision(post_id, revision_id, post_type).await?;
                let author = self
                    .user_names([revision.author], AUTHOR_NAMES_TIMEOUT)
                    .await
                    .remove(&revision.author)
                    .unwrap_or_else(|| format!("user {}", revision.author));
                let format_label = match format {
                    PostContentFormat::Raw if revision.content.raw.is_some() => "raw",
                    PostContentFormat::Raw => "rendered HTML (raw source unavailable)",
                    PostContentFormat::Rendered => "rendered HTML",
                    PostContentFormat::Text => "plain text",
                };
                let excerpt = revision
                    .excerpt
                    .as_ref()
                    .map(|e| post_content_for_display(e, format))
                    .unwrap_or_default();
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Revision Details:\nID: {}\nOf: {} {}\nFormat: {}\nDate: {}\nAuthor: {}\nTitle: {}\nExcerpt: {}\nContent:\n{}",
                            revision.id,
                            post_type,
                            post_id,
                            format_label,
                            revision.date.as_deref().unwrap_or("unknown"),
                            author,
                            post_content_for_display(&revision.title, format),
                            excerpt,
                            post_content_for_display(&revision.content, format)
                        )
                    }],
                    "isError": false
                }))
            }
            "restore_revision" => {
                let args = params.arguments.unwrap_or_default();
                let post_id = positive_id_arg(&args, "post_id")?
                    .ok_or_else(|| McpError::InvalidParams("Missing post_id".to_string()))?;
                let revision_id = positive_id_arg(&args, "revision_id")?
                    .ok_or_else(|| McpError::InvalidParams("Missing revision_id".to_string()))?;
                let post_type = revision_post_type_arg(&args)?;

                let (revision, post) = self
                    .restore_revision(post_id, revision_id, post_type)
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!(
                            "Restored {} {} to revision {} (from {}): title, content and excerpt. Title: {}, Status: {}. The status, date and slug were not changed. When revisions are enabled on the site, WordPress keeps the content before the restore as a revision, so get_revisions and restore_revision can usually undo it.",
                            post_type,
                            post_id,
                            revision.id,
                            revision.date.as_deref().unwrap_or("unknown"),
                            post.title.rendered,
                            post.status
                        )
                    }],
                    "isError": false
                }))
            }
            "create_comment" => {
                let args = params.arguments.unwrap_or_default();
                let content = args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| McpError::InvalidParams("Missing content".to_string()))?;
                let parent = positive_id_arg(&args, "parent")?;
                let status = comment_status_arg(&args, "status", &["approve", "hold"])?
                    .unwrap_or_else(|| DEFAULT_REPLY_STATUS.to_string());
                // 返信なら、投稿は返信先のコメントから決められる
                let post_id = match (positive_id_arg(&args, "post_id")?, parent) {
                    (Some(post_id), _) => post_id,
                    (None, Some(parent)) => self.get_comment(parent).await?.post,
                    (None, None) => {
                        return Err(McpError::InvalidParams(
                            "Missing post_id (or parent, to reply to a comment)".to_string(),
                        ))
                    }
                };

                let comment = self
                    .create_comment(post_id, parent, content, &status)
                    .await?;
                let note = if comment.status.as_deref() == Some("hold") {
                    "\nThe comment is held for moderation and not shown on the site. Approve it with update_comment (status: approve) or in the WordPress admin."
                } else {
                    ""
                };
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Created comment {}{}", comment_summary(&comment), note)
                    }],
                    "isError": false
                }))
            }
            "update_comment" => {
                let args = params.arguments.unwrap_or_default();
                let comment_id = positive_id_arg(&args, "comment_id")?
                    .ok_or_else(|| McpError::InvalidParams("Missing comment_id".to_string()))?;
                let status = comment_status_arg(&args, "status", COMMENT_SET_STATUSES)?;
                let content = match args.get("content") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(value) => Some(
                        value
                            .as_str()
                            .filter(|s| !s.trim().is_empty())
                            .ok_or_else(|| {
                                McpError::InvalidParams(
                                    "content must be a non-empty string".to_string(),
                                )
                            })?,
                    ),
                };
                if status.is_none() && content.is_none() {
                    return Err(McpError::InvalidParams(
                        "Pass status or content to change".to_string(),
                    ));
                }

                let comment = self
                    .update_comment(comment_id, status.as_deref(), content)
                    .await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Updated comment {}", comment_summary(&comment))
                    }],
                    "isError": false
                }))
            }
            "delete_comment" => {
                let args = params.arguments.unwrap_or_default();
                let comment_id = positive_id_arg(&args, "comment_id")?
                    .ok_or_else(|| McpError::InvalidParams("Missing comment_id".to_string()))?;
                let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

                self.delete_comment(comment_id, force).await?;
                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": format!("Deleted comment ID {} ({})",
                            comment_id,
                            if force { "permanently" } else { "moved to trash" }
                        )
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

                let status = post_status_or_draft(&args)?;

                let categories = args
                    .get("categories")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<u64>>());

                let tags = args
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect::<Vec<u64>>());

                let slug = optional_slug_arg(&args)?;

                let params = PostCreateParams {
                    title: title.to_string(),
                    content: content.to_string(),
                    post_type,
                    status,
                    categories,
                    tags,
                    slug,
                    ..Default::default()
                };

                let skipped = Self::skipped_embed_urls(&youtube_urls, &social_urls);

                let post = self
                    .create_post_with_embeds(
                        title,
                        content,
                        youtube_urls,
                        social_urls,
                        Some(params),
                    )
                    .await?;

                let mut text = format!(
                    "Created {} with embedded content - ID: {}, Title: {}, Status: {}, Slug: {}",
                    if post.post_type.as_ref().unwrap_or(&"post".to_string()) == "page" {
                        "page"
                    } else {
                        "post"
                    },
                    display_opt_id(post.id),
                    post.title.rendered,
                    post.status,
                    post_slug_label(&post)
                );
                if !skipped.is_empty() {
                    text.push_str(&format!(
                        "\nNot embedded (unsupported URL): {}",
                        skipped.join(", ")
                    ));
                }

                Ok(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": text
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
}

#[async_trait]
impl McpHandler for WordPressHandler {
    async fn initialize(&self, params: InitializeParams) -> Result<serde_json::Value, McpError> {
        info!("WordPress MCP Handler initialized");
        Ok(serde_json::json!({
            "protocolVersion": crate::mcp::negotiate_protocol_version(&params.protocol_version),
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
        let tools = vec![
            Tool {
                name: "wordpress_health_check".to_string(),
                title: Some("Check the WordPress connection".to_string()),
                description: "Check that mcp-rs can use the WordPress site: whether the site and its REST API answer, whether the credentials sign in, whether the user has the permissions the tools need, and whether the media library can be read. Changes nothing. Use it first when other tools fail. Returns a report with the site name and URL, each check's result, and the reason for each failure.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_posts".to_string(),
                title: Some("List posts".to_string()),
                description: "List WordPress posts (or the posts of an allowed custom post type), published ones by default. Use search, categories, tags, author, after and before to narrow them, and get_post to read one post's full content. Any argument other than status (search, a filter, sorting, per_page or page) makes it return one page and the total. With none of them, or with status alone, it returns all posts, but at most 10,000, without the total; on larger sites pass page and per_page and read page after page. Each post has its ID, title, status, date, link, slug and excerpt.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Which posts to list (optional): 'publish' (default), 'future' (scheduled), 'draft', 'pending', 'private', or 'any'. Several can be given with commas, e.g. 'future,draft'. Other than 'publish', the credentials must be allowed to edit posts"
                        },
                        "search": {
                            "type": "string",
                            "description": "Keyword to search for in the title and content (optional)"
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Only posts in any of these category IDs (optional; get_categories lists them)"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Only posts with any of these tag IDs (optional; get_tags lists them)"
                        },
                        "author": {
                            "type": "number",
                            "description": "Author user ID (optional)"
                        },
                        "after": {
                            "type": "string",
                            "description": "Only items published after this date or date-time, ISO8601 in the site's time zone, e.g. '2026-11-01' (optional)"
                        },
                        "before": {
                            "type": "string",
                            "description": "Only items published before this date or date-time (optional)"
                        },
                        "orderby": {
                            "type": "string",
                            "enum": ["date", "modified", "title", "id", "slug", "author", "relevance"],
                            "description": "Sort field (optional, default 'date'). 'relevance' needs search"
                        },
                        "order": {
                            "type": "string",
                            "enum": ["asc", "desc"],
                            "description": "Sort order (optional, default 'desc')"
                        },
                        "per_page": {
                            "type": "number",
                            "description": "Items per page, 1-100 (optional). With any argument other than status, only one page is read (default 20 per page), and the result shows the total"
                        },
                        "page": {
                            "type": "number",
                            "description": "Page number, from 1 (optional)"
                        }
                    },
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_pages".to_string(),
                title: Some("List pages".to_string()),
                description: "List WordPress pages (static pages such as About or Contact, not blog posts), published ones by default. Use get_posts for posts and get_post with post_type 'page' to read one page's full content. Any argument other than status (search, a filter, sorting, per_page or page) makes it return one page and the total. With none of them, or with status alone, it returns all pages, but at most 10,000, without the total; on larger sites pass page and per_page and read page after page. Each page has its ID, title, status, date, link, slug and excerpt.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {
                            "type": "string",
                            "description": "Which pages to list (optional): 'publish' (default), 'future', 'draft', 'pending', 'private', or 'any', separated by commas. Other than 'publish', the credentials must be allowed to edit pages"
                        },
                        "search": {
                            "type": "string",
                            "description": "Keyword to search for in the title and content (optional)"
                        },
                        "author": {
                            "type": "number",
                            "description": "Author user ID (optional)"
                        },
                        "after": {
                            "type": "string",
                            "description": "Only items published after this date or date-time, ISO8601 in the site's time zone, e.g. '2026-11-01' (optional)"
                        },
                        "before": {
                            "type": "string",
                            "description": "Only items published before this date or date-time (optional)"
                        },
                        "orderby": {
                            "type": "string",
                            "enum": ["date", "modified", "title", "id", "slug", "author", "relevance", "menu_order"],
                            "description": "Sort field (optional, default 'date'). 'relevance' needs search"
                        },
                        "order": {
                            "type": "string",
                            "enum": ["asc", "desc"],
                            "description": "Sort order (optional, default 'desc')"
                        },
                        "per_page": {
                            "type": "number",
                            "description": "Items per page, 1-100 (optional). With any argument other than status, only one page is read (default 20 per page), and the result shows the total"
                        },
                        "page": {
                            "type": "number",
                            "description": "Page number, from 1 (optional)"
                        }
                    },
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_all_content".to_string(),
                title: Some("List all posts and pages".to_string()),
                description: "List the published WordPress posts and the published pages in one call (at most 10,000 of each), for an overview of the whole site. Returns each item's ID, title, status, date, link, slug and excerpt, posts and pages separately. On large sites prefer get_posts or get_pages with search, filters or per_page, which return one page at a time.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_post".to_string(),
                title: Some("Read a post or page".to_string()),
                description: "Read one WordPress post, page, or custom post type item by ID, with its full content. Returns the ID, title, status, date, slug, categories, tags, featured image and content. By default the content is the editable source (with Gutenberg block markup), which can be passed back to update_post unchanged. If the credentials cannot edit the item, the content is rendered HTML instead and is labeled so; do not write that back with update_post, because it would replace the blocks. Use get_posts or get_pages to find the ID.".to_string(),
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
                        },
                        "format": {
                            "type": "string",
                            "enum": ["raw", "rendered", "text"],
                            "description": "How to return the content. 'raw': editable source (requires edit permission; falls back to rendered HTML otherwise). 'rendered': HTML as displayed on the site. 'text': plain text with tags removed. Defaults to 'raw'.",
                            "default": "raw"
                        }
                    },
                    "required": ["post_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "create_post".to_string(),
                title: Some("Create a post".to_string()),
                description: "Create a WordPress post with a title and content only. It is a draft unless status says otherwise, so nothing is published by accident. Use create_post_with_categories_tags to also set categories, tags or a featured image, and create_advanced_post for pages, custom post types, scheduling, custom taxonomies or SEO fields. Returns the new post's ID, status and slug.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Body of the post as HTML or Gutenberg block markup (the format get_post returns)"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["publish", "draft", "pending", "private"],
                            "description": "Post status. Defaults to 'draft' to avoid accidental publication.",
                            "default": "draft"
                        },
                        "slug": {
                            "type": "string",
                            "description": "URL slug (optional). If omitted, WordPress generates one from the title (a Japanese title gives a Japanese slug). Use it to set an ASCII slug, e.g. 'my-first-post'. WordPress may change it (for example, appending '-2' if the slug is already used); the result shows the actual slug"
                        }
                    },
                    "required": ["title", "content"]
                }),
                ..Default::default()
            },
            Tool {
                name: "create_advanced_post".to_string(),
                title: Some("Create a post or page with all options".to_string()),
                description: "Create a WordPress post, page, or custom post type item with every option: categories, tags, terms of custom taxonomies, featured image, SEO meta fields, slug, and a publish date for scheduling (status 'future'). It is a draft unless status says otherwise. Use it to schedule a post or to create a page; create_post is enough for a plain draft post. Returns the new item's ID, status and slug.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "terms": {
                            "type": "object",
                            "description": "Terms of taxonomies, such as a custom post type's own categories: {\"<taxonomy rest_base>\": [term IDs]}, e.g. {\"product_cat\": [12, 15]}. Each listed taxonomy's terms are replaced with the given ones. See get_taxonomies and get_terms",
                            "additionalProperties": {"type": "array", "items": {"type": "number"}}
                        },
                        "title": {
                            "type": "string",
                            "description": "The post/page title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Body as HTML or Gutenberg block markup (the format get_post returns)"
                        },
                        "post_type": {
                            "type": "string",
                            "description": "'post' (a blog post, the default), 'page' (a static page), or an allowed custom post type.",
                            "enum": ["post", "page"],
                            "default": "post"
                        },
                        "status": {
                            "type": "string",
                            "description": "'draft' (the default, not shown on the site), 'publish' (shown at once), 'pending' (awaiting review), 'private' (only for signed-in editors), or 'future' (scheduled; needs date).",
                            "enum": ["publish", "draft", "pending", "private", "future"],
                            "default": "draft"
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
                        },
                        "slug": {
                            "type": "string",
                            "description": "URL slug (optional). If omitted, WordPress generates one from the title (a Japanese title gives a Japanese slug). Use it to set an ASCII slug, e.g. 'my-first-post'. WordPress may change it (for example, appending '-2' if the slug is already used); the result shows the actual slug"
                        }
                    },
                    "required": ["title", "content"]
                }),
                ..Default::default()
            },
            Tool {
                name: "get_comments".to_string(),
                title: Some("List comments".to_string()),
                description: "List WordPress comments, optionally for one post: ID, post, parent, status, author name, date and content. Only approved comments by default; use status 'hold' to find comments awaiting moderation. Commenters' email and IP addresses are never returned. Use update_comment to approve, hold or mark as spam, create_comment to reply, and delete_comment to delete.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Optional post ID to filter comments"
                        },
                        "status": {
                            "type": "string",
                            "enum": COMMENT_LIST_STATUSES,
                            "description": "Comment status: 'approve' (approved, the default), 'hold' (awaiting moderation), 'spam', 'trash', or 'all' (approved and awaiting moderation)"
                        }
                    },
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_post_types".to_string(),
                title: Some("List post types".to_string()),
                description: "List the site's post types (post, page, and custom ones such as products or events) with their taxonomies, and whether mcp-rs may use each. Custom post types are used only when listed in post_types in the mcp-rs config. Use it before working with custom post types, then get_taxonomies and get_terms for their categories.".to_string(),
                input_schema: serde_json::json!({"type": "object", "properties": {}, "required": []}),
                ..Default::default()
            },
            Tool {
                name: "get_taxonomies".to_string(),
                title: Some("List taxonomies".to_string()),
                description: "List the site's taxonomies (categories, tags, and custom ones such as product categories) with the post types they belong to and the rest_base to use as the key in the terms argument of create_advanced_post and update_post. Use get_terms next to get the term IDs of one taxonomy.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_type": {
                            "type": "string",
                            "description": "Only the taxonomies of this post type"
                        }
                    },
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_terms".to_string(),
                title: Some("List the terms of a taxonomy".to_string()),
                description: "List the terms of one taxonomy (for example the product categories of a shop): ID, name, slug, parent and number of posts. Use the IDs in the terms argument of create_advanced_post and update_post. For the built-in categories and tags, get_categories and get_tags return the same with descriptions.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "taxonomy": {
                            "type": "string",
                            "description": "Taxonomy name or rest_base, from get_taxonomies"
                        }
                    },
                    "required": ["taxonomy"]
                }),
                ..Default::default()
            },
            Tool {
                name: "get_revisions".to_string(),
                title: Some("List revisions".to_string()),
                description: "List the revisions (saved earlier versions) of a post or page, newest first: revision ID, date, author, title, length and the start of the content. Needs credentials with edit permission on the post. Use get_revision to read one in full, and restore_revision to go back to it.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID"
                        },
                        "post_type": {
                            "type": "string",
                            "enum": ["post", "page"],
                            "description": "Post type of post_id: 'post' (the default), 'page', or an allowed custom post type.",
                            "default": "post"
                        }
                    },
                    "required": ["post_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "get_revision".to_string(),
                title: Some("Read a revision".to_string()),
                description: "Read one revision (an earlier saved version) of a post or page: its title, excerpt and full content, as editable source by default. Use it to compare an old version with the current one (get_post) before restore_revision. Get the revision ID from get_revisions.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID"
                        },
                        "revision_id": {
                            "type": "number",
                            "description": "Revision ID, from get_revisions"
                        },
                        "post_type": {
                            "type": "string",
                            "enum": ["post", "page"],
                            "description": "Post type of post_id: 'post' (the default), 'page', or an allowed custom post type.",
                            "default": "post"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["raw", "rendered", "text"],
                            "description": "'raw' (editable source, the default), 'rendered' (HTML), or 'text' (plain text)",
                            "default": "raw"
                        }
                    },
                    "required": ["post_id", "revision_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "restore_revision".to_string(),
                title: Some("Restore a revision".to_string()),
                description: "Restore a post or page to one of its revisions: its title, content and excerpt are replaced with the revision's. The status, publish date, slug, categories and tags are not changed, so a published post stays published with the restored content. The restore can usually be undone the same way, because WordPress keeps the previous content as a revision when revisions are enabled; depending on the site's revision settings it may not be kept, so read the current content with get_post first if it must be kept. Returns the post's ID, title and the restored revision.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID"
                        },
                        "revision_id": {
                            "type": "number",
                            "description": "Revision ID to restore, from get_revisions"
                        },
                        "post_type": {
                            "type": "string",
                            "enum": ["post", "page"],
                            "description": "Post type of post_id: 'post' (the default), 'page', or an allowed custom post type.",
                            "default": "post"
                        }
                    },
                    "required": ["post_id", "revision_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "create_comment".to_string(),
                title: Some("Create a comment or reply".to_string()),
                description: "Create a comment, usually a reply to another comment (parent), as the configured WordPress user. WordPress comments have no drafts, so by default the comment is held for moderation and is not shown on the site until approved (update_comment with status 'approve', or in the WordPress admin). Pass status 'approve' to publish it at once. Returns the new comment's ID, post, parent and status.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "post_id": {
                            "type": "number",
                            "description": "Post or page ID to comment on. Optional when parent is given (the post of the parent comment is used)"
                        },
                        "parent": {
                            "type": "number",
                            "description": "ID of the comment to reply to"
                        },
                        "content": {
                            "type": "string",
                            "description": "Comment text"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["hold", "approve"],
                            "description": "'hold' (held for moderation, the default) or 'approve' (published at once)",
                            "default": DEFAULT_REPLY_STATUS
                        }
                    },
                    "required": ["content"]
                }),
                ..Default::default()
            },
            Tool {
                name: "update_comment".to_string(),
                title: Some("Moderate or edit a comment".to_string()),
                description: "Change a comment's status (approve, hold for moderation, mark as spam, move to trash) or its text. Approving a comment publishes it on the site; marking it as spam or moving it to the trash takes it off the site. Use delete_comment to delete it permanently. Returns the comment's ID, post, parent and new status.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "comment_id": {
                            "type": "number",
                            "description": "Comment ID"
                        },
                        "status": {
                            "type": "string",
                            "enum": COMMENT_SET_STATUSES,
                            "description": "'approve' (published), 'hold' (back to awaiting moderation), 'spam', or 'trash'"
                        },
                        "content": {
                            "type": "string",
                            "description": "New comment text"
                        }
                    },
                    "required": ["comment_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "delete_comment".to_string(),
                title: Some("Delete a comment".to_string()),
                description: "Delete a comment. By default it is moved to the trash, where it can be restored in the WordPress admin; with force: true it is deleted permanently and cannot be restored. To only hide a comment, use update_comment with status 'hold' or 'spam' instead. Returns the comment ID and whether it was trashed or deleted.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "comment_id": {
                            "type": "number",
                            "description": "Comment ID"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Delete permanently instead of moving to the trash"
                        }
                    },
                    "required": ["comment_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "upload_media".to_string(),
                title: Some("Upload a media file".to_string()),
                description: format!(
                    "Upload an image, PDF, video or audio file to the WordPress media library. Pass either file_data (base64, with filename and mime_type) or file_path (a file on the machine where mcp-rs runs, when allowed by upload_allowed_dirs). The filename extension and the file's content must match mime_type. Size is capped at {} MiB; with file_data, the transport's message size limit (including base64 overhead) usually applies first. Returns the new media ID and URL; use the ID with set_featured_image or featured_media_id.",
                    MAX_UPLOAD_SIZE_BYTES / (1024 * 1024)
                ),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_data": {
                            "type": "string",
                            "description": "Base64 encoded file data. Requires filename and mime_type. Use file_path instead for files on this machine when it is enabled"
                        },
                        "file_path": {
                            "type": "string",
                            "description": "Absolute path of a file to upload, read by the mcp-rs server. Only files inside the folders listed in the server's upload_allowed_dirs setting can be read (disabled by default). filename and mime_type default to the file's name and extension"
                        },
                        "filename": {
                            "type": "string",
                            "description": "Original filename; its extension must match mime_type (e.g., 'photo.jpg' for image/jpeg). Required with file_data"
                        },
                        "mime_type": {
                            "type": "string",
                            "description": format!(
                                "MIME type of the file. Required with file_data. Allowed: {}",
                                allowed_upload_mime_list()
                            )
                        }
                    },
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_media".to_string(),
                title: Some("List media files".to_string()),
                description: "List the files in the WordPress media library (at most 10,000): ID, title, MIME type, URL, alt text and date, with the total number of files. Use get_media_item for one file, update_media to change its title, alt text or caption, and the ID as featured_media_id or with set_featured_image.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "get_media_item".to_string(),
                title: Some("Read a media file's details".to_string()),
                description: "Read one file of the WordPress media library by ID: its title, alt text and URL. Use get_media to find the ID, and update_media to change the title, alt text, caption or description.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "update_media".to_string(),
                title: Some("Edit media details".to_string()),
                description: "Change the details of a file in the WordPress media library: title, alt text, caption and description, or attach it to a post. Only the fields given are changed; the file itself is not replaced (upload a new file with upload_media for that). Returns the media ID.".to_string(),
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
                            "description": "ID of the post or page to attach the file to (shown as 'Uploaded to' in the media library). It does not insert the file into the content"
                        }
                    },
                    "required": ["media_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "delete_media".to_string(),
                title: Some("Delete a media file".to_string()),
                description: "Delete a file from the WordPress media library, together with its resized copies. WordPress has no trash for media by default, so force: true is usually required, and the file is then deleted permanently and cannot be restored. Posts that show the file (in their content or as a featured image) will show a broken image. Returns the media ID.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "media_id": {
                            "type": "number",
                            "description": "Media ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Permanently delete (bypass trash). Usually required for media, since WordPress does not support trashing media by default"
                        }
                    },
                    "required": ["media_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "create_post_with_featured_image".to_string(),
                title: Some("Create a post with a featured image".to_string()),
                description: "Create a WordPress post with a title, content and a featured image (the media ID from upload_media or get_media). It is a draft unless status says otherwise. To add a featured image to an existing post, use set_featured_image instead. Returns the new post's ID, status and slug.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Body of the post as HTML or Gutenberg block markup (the format get_post returns)"
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Media ID for featured image"
                        },
                        "status": {
                            "type": "string",
                            "description": "Post status. Defaults to 'draft' so nothing is published unintentionally. 'future' is not available because this tool has no date parameter.",
                            "enum": ["publish", "draft", "pending", "private"],
                            "default": "draft"
                        },
                        "slug": {
                            "type": "string",
                            "description": "URL slug (optional). If omitted, WordPress generates one from the title (a Japanese title gives a Japanese slug). Use it to set an ASCII slug, e.g. 'my-first-post'. WordPress may change it (for example, appending '-2' if the slug is already used); the result shows the actual slug"
                        }
                    },
                    "required": ["title", "content", "featured_media_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "set_featured_image".to_string(),
                title: Some("Set a post's featured image".to_string()),
                description: "Set or replace the featured image (thumbnail) of an existing post, using a media ID from upload_media or get_media. Only the featured image is changed. Returns the post ID.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "get_categories".to_string(),
                title: Some("List categories".to_string()),
                description: "List the post categories of the site (at most 10,000): ID, name, slug, description, parent and number of posts. Use the IDs in the categories argument of the post tools and get_posts. For other taxonomies, use get_taxonomies and get_terms.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "create_category".to_string(),
                title: Some("Create a category".to_string()),
                description: "Create a post category, optionally under a parent category. Fails if a category with the same name already exists under the same parent: check get_categories first. Returns the new category's ID, to use in the categories argument of the post tools.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "update_category".to_string(),
                title: Some("Edit a category".to_string()),
                description: "Change a category's name, description, parent or slug. Only the fields given are changed, and WordPress does not change the slug when the name changes. Posts in the category stay in it. Returns the category ID and name.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "delete_category".to_string(),
                title: Some("Delete a category".to_string()),
                description: "Delete a category permanently. WordPress has no trash for categories, so force must be true. Posts that were only in this category are moved to the default category. The default category itself cannot be deleted. Returns the category ID.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category_id": {
                            "type": "number",
                            "description": "Category ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Must be true: WordPress has no trash for categories, so they can only be deleted permanently"
                        }
                    },
                    "required": ["category_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "get_tags".to_string(),
                title: Some("List tags".to_string()),
                description: "List the post tags of the site (at most 10,000): ID, name, slug, description and number of posts. Use the IDs in the tags argument of the post tools and get_posts.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "create_tag".to_string(),
                title: Some("Create a tag".to_string()),
                description: "Create a post tag. Fails if a tag with the same name already exists: check get_tags first. Returns the new tag's ID, to use in the tags argument of the post tools.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "update_tag".to_string(),
                title: Some("Edit a tag".to_string()),
                description: "Change a tag's name, description or slug. Only the fields given are changed, and WordPress does not change the slug when the name changes. Posts with the tag keep it. Returns the tag ID and name.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "delete_tag".to_string(),
                title: Some("Delete a tag".to_string()),
                description: "Delete a tag permanently. WordPress has no trash for tags, so force must be true. The tag is removed from every post that has it; the posts themselves are not changed otherwise. Returns the tag ID.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tag_id": {
                            "type": "number",
                            "description": "Tag ID to delete"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Must be true: WordPress has no trash for tags, so they can only be deleted permanently"
                        }
                    },
                    "required": ["tag_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "create_post_with_categories_tags".to_string(),
                title: Some("Create a post with categories and tags".to_string()),
                description: "Create a WordPress post with categories, tags and optionally a featured image, by their IDs (from get_categories, get_tags and upload_media). It is a draft unless status says otherwise. Use create_advanced_post to schedule it or for pages and custom post types. Returns the new post's ID, status, slug, categories and tags.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "The post title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Body of the post as HTML or Gutenberg block markup (the format get_post returns)"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["publish", "draft", "pending", "private"],
                            "description": "Post status. Defaults to 'draft' to avoid accidental publication.",
                            "default": "draft"
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Category IDs from get_categories (optional). Without it, WordPress uses the default category"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Tag IDs from get_tags (optional)"
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Featured image media ID (optional)"
                        },
                        "slug": {
                            "type": "string",
                            "description": "URL slug (optional). If omitted, WordPress generates one from the title (a Japanese title gives a Japanese slug). Use it to set an ASCII slug, e.g. 'my-first-post'. WordPress may change it (for example, appending '-2' if the slug is already used); the result shows the actual slug"
                        }
                    },
                    "required": ["title", "content"]
                }),
                ..Default::default()
            },
            Tool {
                name: "update_post_categories_tags".to_string(),
                title: Some("Change a post's categories and tags".to_string()),
                description: "Replace the categories and/or tags of an existing post. Each list given replaces the post's current list, so include the IDs to keep; a list not given is not changed. Nothing else of the post is changed. Use update_post to change other fields at the same time. Returns the post ID with its new categories and tags.".to_string(),
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
                            "description": "Category IDs from get_categories (optional). Replaces all of the post's current categories, so include the ones to keep"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Tag IDs from get_tags (optional). Replaces all of the post's current tags, so include the ones to keep; [] removes them all"
                        }
                    },
                    "required": ["post_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "update_post".to_string(),
                title: Some("Edit a post or page".to_string()),
                description: "Change an existing post, page or custom post type item: title, content, status, publish date, slug, categories, tags, terms of custom taxonomies or featured image. Only the fields given are changed. Setting status 'publish' publishes it on the site, and status 'future' with a future date schedules it. When revisions are enabled, WordPress keeps the previous content as a revision (get_revisions, restore_revision). Returns the ID, title, status, date and slug.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "terms": {
                            "type": "object",
                            "description": "Terms of taxonomies, such as a custom post type's own categories: {\"<taxonomy rest_base>\": [term IDs]}, e.g. {\"product_cat\": [12, 15]}. Each listed taxonomy's terms are replaced with the given ones. See get_taxonomies and get_terms",
                            "additionalProperties": {"type": "array", "items": {"type": "number"}}
                        },
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
                            "description": "Post status (optional): 'publish', 'draft', 'pending', 'private', or 'future' (scheduled; needs 'date' unless the post is already scheduled)"
                        },
                        "date": {
                            "type": "string",
                            "description": "Publication date (optional, ISO8601 in the site's time zone, e.g. '2026-11-19T10:00:00'). With status 'future' and a future date, the post is scheduled. get_post shows the current date"
                        },
                        "categories": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Category IDs from get_categories (optional). Replaces all of the post's current categories, so include the ones to keep"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "number"},
                            "description": "Tag IDs from get_tags (optional). Replaces all of the post's current tags, so include the ones to keep; [] removes them all"
                        },
                        "featured_media_id": {
                            "type": "number",
                            "description": "Featured image media ID (optional)"
                        },
                        "slug": {
                            "type": "string",
                            "description": "New URL slug (optional, e.g. an ASCII slug for a post with a Japanese title). WordPress does not update the slug when the title changes. Changing the slug of a published post changes its URL"
                        }
                    },
                    "required": ["post_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "delete_post".to_string(),
                title: Some("Delete a post or page".to_string()),
                description: "Delete a post, page or custom post type item. By default it is moved to the trash, where it can be restored in the WordPress admin; with force: true it is deleted permanently, with its revisions, and cannot be restored. To only take it off the site, use update_post with status 'draft' or 'private' instead. Returns the ID and whether it was trashed or deleted.".to_string(),
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
                            "description": "Delete permanently instead of moving to the trash (optional, default false). It cannot be undone"
                        }
                    },
                    "required": ["post_id"]
                }),
                ..Default::default()
            },
            Tool {
                name: "create_post_with_embeds".to_string(),
                title: Some("Create a post with embedded videos".to_string()),
                description: "Create a WordPress post or page whose content is followed by embedded media: each YouTube video URL (a watch, youtu.be, embed, Shorts or live URL) becomes a video player, and each X/Twitter, Instagram post, Facebook or TikTok URL is added on its own line, for WordPress to show as an embed. WordPress itself does not embed Instagram or Facebook (since 5.5.2), so those stay plain links unless the site has a plugin for them; the result shows only that a URL was added, not how WordPress shows it. Other URLs are left out, and the result lists them. It is a draft unless status says otherwise. To embed in the middle of the content, write the URLs or embed blocks into content and use create_post instead. Returns the new post's ID, title, status and slug.".to_string(),
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
                            "description": "YouTube video URLs: youtube.com/watch?v=..., youtu.be/..., youtube.com/embed/..., youtube.com/shorts/..., youtube.com/live/... or youtube-nocookie.com/embed/.... Other forms are left out and listed in the result"
                        },
                        "social_urls": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Social media URLs to embed: X/Twitter, Instagram posts (instagram.com/p/...), Facebook or TikTok. Instagram and Facebook stay plain links unless the site has a plugin that embeds them. Other URLs are left out and listed in the result"
                        },
                        "post_type": {
                            "type": "string",
                            "description": "Post type (post or page)",
                            "enum": ["post", "page"]
                        },
                        "status": {
                            "type": "string",
                            "description": "Post status. Defaults to 'draft' so nothing is published unintentionally. 'future' is not available because this tool has no date parameter.",
                            "enum": ["publish", "draft", "pending", "private"],
                            "default": "draft"
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
                        "slug": {
                            "type": "string",
                            "description": "URL slug (optional). If omitted, WordPress generates one from the title (a Japanese title gives a Japanese slug). Use it to set an ASCII slug, e.g. 'my-first-post'. WordPress may change it (for example, appending '-2' if the slug is already used); the result shows the actual slug"
                        }
                    },
                    "required": ["title", "content"]
                }),
                ..Default::default()
            },
            Tool {
                name: "get_settings".to_string(),
                title: Some("Read site settings".to_string()),
                description: "Read the main WordPress site settings: site title, tagline, what the front page shows, posts per page and language (time zone and the default category are not shown). Needs administrator credentials. Use update_settings to change them, or set_front_page and set_front_page_to_posts for the front page.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                ..Default::default()
            },
            Tool {
                name: "update_settings".to_string(),
                title: Some("Change site settings".to_string()),
                description: "Change WordPress site settings: site title, tagline, time zone, language, posts per page, default category, and the front page. Only the fields given are changed, and the changes apply to the live site at once. Needs administrator credentials. Read the current values with get_settings first. Returns the site title, tagline and what the front page shows.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "set_front_page".to_string(),
                title: Some("Show a page as the front page".to_string()),
                description: "Make a static page (from get_pages) the site's front page, instead of the latest posts. This changes the live site at once. Use set_front_page_to_posts to go back to showing the latest posts. Needs administrator credentials. Returns the page ID.".to_string(),
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
                ..Default::default()
            },
            Tool {
                name: "set_front_page_to_posts".to_string(),
                title: Some("Show latest posts on the front page".to_string()),
                description: "Make the site's front page show the latest posts, instead of a static page. Optionally give the page that lists the posts. This changes the live site at once. Use set_front_page to show a static page instead. Needs administrator credentials.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "posts_page_id": {
                            "type": "number",
                            "description": "ID of a page (from get_pages) on which to list the posts, when the front page itself is not that list (optional)"
                        }
                    },
                    "required": []
                }),
                ..Default::default()
            },
        ];
        Ok(tools
            .into_iter()
            .map(|tool| {
                let mut tool = Tool {
                    annotations: Some(wordpress_tool_annotations(&tool.name)),
                    ..tool
                };
                with_allowed_post_types(&mut tool, &self.allowed_post_types);
                tool
            })
            .collect())
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
        let started = std::time::Instant::now();
        let tool_name = params.name.clone();
        let target_id = extract_target_id(&params.arguments);

        if let Err(e) = self
            .rate_limiter
            .check_rate_limit(RATE_LIMIT_CLIENT_ID)
            .await
        {
            warn!("WordPress tool call rate limited: {}", e);
            let result = Err(McpError::Other(format!("Rate limit exceeded: {}", e)));
            self.audit_tool_call(&tool_name, target_id.as_deref(), &result, started)
                .await;
            return result;
        }

        let result = self.dispatch_tool(params).await;
        self.audit_tool_call(&tool_name, target_id.as_deref(), &result, started)
            .await;
        result
    }

    async fn status(&self) -> Option<String> {
        Some(format!("WordPress site {}", self.base_url))
    }

    /// 各リソースは、同じデータを読めるツールと同じ扱いにする（#538）。
    /// `wordpress://posts`は投稿の本文まで返すので、要約だけを返す
    /// `get_posts`ではなく、本文を返す`get_post`に対応させる
    fn prompts(&self) -> Vec<crate::mcp::PromptSpec> {
        crate::handlers::prompts::wordpress_prompts()
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: &HashMap<String, String>,
    ) -> Result<serde_json::Value, McpError> {
        crate::handlers::prompts::wordpress_prompt(name, arguments)
    }

    fn resource_tool(&self, uri: &str) -> Option<String> {
        let tool = match uri {
            "wordpress://posts" => "get_post",
            "wordpress://comments" => "get_comments",
            "wordpress://categories" => "get_categories",
            "wordpress://tags" => "get_tags",
            // テンプレート（#644）。テンプレートの文字列と、作ったURIの両方
            _ => match resource_template_target(uri)? {
                (ResourceKind::Post | ResourceKind::Page, _) => "get_post",
                (ResourceKind::Media, _) => "get_media_item",
            },
        };
        Some(tool.to_string())
    }

    async fn list_resource_templates(&self) -> Result<Vec<crate::mcp::ResourceTemplate>, McpError> {
        let template = |uri: &str, name: &str, description: &str| crate::mcp::ResourceTemplate {
            uri_template: uri.to_string(),
            name: name.to_string(),
            description: Some(description.to_string()),
            mime_type: Some("application/json".to_string()),
        };
        Ok(vec![
            template(
                "wordpress://post/{id}",
                "WordPress Post",
                "One post by ID, with its editable source when the credentials allow",
            ),
            template(
                "wordpress://page/{id}",
                "WordPress Page",
                "One page by ID, with its editable source when the credentials allow",
            ),
            template(
                "wordpress://media/{id}",
                "WordPress Media Item",
                "One media item by ID: URL, type, title, alt text and caption",
            ),
        ])
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
        // ツールの呼び出しと同じレート制限を通す（#538）。`wordpress://posts`
        // 等は全ページを取得するので、1回の読み取りでも重い
        if let Err(e) = self
            .rate_limiter
            .check_rate_limit(RATE_LIMIT_CLIENT_ID)
            .await
        {
            warn!("WordPress resource read rate limited: {}", e);
            return Err(McpError::Other(format!("Rate limit exceeded: {}", e)));
        }
        match params.uri.as_str() {
            "wordpress://posts" => {
                let posts = self.get_posts(None).await?;
                Ok(serde_json::json!({
                    "contents": [{
                        "uri": params.uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&posts)?
                    }]
                }))
            }
            "wordpress://comments" => {
                let comments = self.get_comments(None, None).await?;
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
            uri => match resource_template_target(uri) {
                Some((kind, ResourceId::Id(id))) => {
                    let text = match kind {
                        ResourceKind::Post | ResourceKind::Page => {
                            let post_type = if kind == ResourceKind::Page {
                                "page"
                            } else {
                                "post"
                            };
                            let post = self
                                .get_post(id, post_type, true)
                                .await
                                .map_err(|e| not_found_as_resource(e, uri))?;
                            serde_json::to_string_pretty(&post)?
                        }
                        ResourceKind::Media => {
                            let media = self
                                .get_media_item(id)
                                .await
                                .map_err(|e| not_found_as_resource(e, uri))?;
                            serde_json::to_string_pretty(&media)?
                        }
                    };
                    Ok(serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": text
                        }]
                    }))
                }
                _ => Err(McpError::ResourceNotFound(params.uri)),
            },
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
            keyring: None,
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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
            keyring: None,
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
        };

        let result = WordPressHandler::try_new(secure_config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handler_debug_redacts_password() {
        let config = WordPressConfig {
            url: "https://secure.example.com".to_string(),
            username: "wp-user".to_string(),
            password: "super-secret-plaintext".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
            keyring: None,
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
        };
        let handler = WordPressHandler::try_new(config).unwrap();

        let debug_output = format!("{:?}", handler);
        assert!(!debug_output.contains("super-secret-plaintext"));
        assert!(debug_output.contains("[REDACTED]"));
        // パスワード以外のフィールドは引き続き見えること
        assert!(debug_output.contains("wp-user"));
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
                keyring: None,
                upload_allowed_dirs: None,
                post_types: None,
                default_site: None,
                sites: Default::default(),
            };

            let result = WordPressHandler::try_new(config);
            assert!(result.is_err(), "URL {} should be rejected", url);
        }
    }

    fn rate_limited_handler(rate_limit: RateLimitConfig) -> WordPressHandler {
        let config = WordPressConfig {
            // どのツールもこのホストへは実際には到達しない
            // （バーストが0の場合、ネットワークI/Oより先にレート制限で
            // 拒否されるため、到達可能である必要がない）。
            url: "https://example.invalid".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(rate_limit),
            encrypted_credentials: None,
            keyring: None,
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
        };
        WordPressHandler::try_new(config).unwrap()
    }

    #[tokio::test]
    async fn test_call_tool_rejects_when_rate_limit_exhausted() {
        // burst_size: 0はトークンバケットが常に空であることを意味し、
        // 最初のcheck_rate_limit呼び出しが（ネットワークI/Oより前に）
        // 即座に失敗する。
        let handler = rate_limited_handler(RateLimitConfig {
            requests_per_second: 1,
            burst_size: 0,
            enabled: true,
        });

        let result = handler
            .call_tool(ToolCallParams {
                name: "get_posts".to_string(),
                arguments: None,
            })
            .await;

        assert!(
            matches!(result, Err(McpError::Other(ref msg)) if msg.contains("Rate limit exceeded")),
            "expected a rate-limit error, got: {:?}",
            result
        );
    }

    /// リソースの読み取りも、ツールと同じレート制限を通る（#538）
    #[tokio::test]
    async fn test_read_resource_rejects_when_rate_limit_exhausted() {
        let handler = rate_limited_handler(RateLimitConfig {
            requests_per_second: 1,
            burst_size: 0,
            enabled: true,
        });

        let result = handler
            .read_resource(ResourceReadParams {
                uri: "wordpress://posts".to_string(),
            })
            .await;

        assert!(
            matches!(result, Err(McpError::Other(ref msg)) if msg.contains("Rate limit exceeded")),
            "expected a rate-limit error, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_call_tool_audits_mutating_tool_rejected_by_rate_limit() {
        // レート制限で拒否された変更系ツール呼び出しも、WordPress側の
        // エラーによる失敗と同様に監査ログに残す必要がある（インシデント
        // 調査では「誰がdelete_postを乱打して制限にかかったか」も重要な
        // 情報のため）。
        let handler = rate_limited_handler(RateLimitConfig {
            requests_per_second: 1,
            burst_size: 0,
            enabled: true,
        });

        let result = handler
            .call_tool(ToolCallParams {
                name: "delete_post".to_string(),
                arguments: None,
            })
            .await;
        assert!(
            matches!(result, Err(McpError::Other(ref msg)) if msg.contains("Rate limit exceeded"))
        );

        let entries = handler.audit_logger.get_all_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, AuditLevel::Error);
        assert_eq!(entries[0].result, Some("FAILURE".to_string()));
        assert_eq!(entries[0].action, Some("delete_post".to_string()));
        assert!(entries[0]
            .metadata
            .get("error")
            .is_some_and(|e| e.contains("Rate limit exceeded")));
    }

    #[tokio::test]
    async fn test_call_tool_rate_limit_runs_before_tool_dispatch() {
        // レート制限チェックはツール名のmatchより前に実行されるため、
        // 存在しないツール名でもToolNotFoundではなくレート制限エラーに
        // なる。
        let handler = rate_limited_handler(RateLimitConfig {
            requests_per_second: 1,
            burst_size: 0,
            enabled: true,
        });

        let result = handler
            .call_tool(ToolCallParams {
                name: "nonexistent_tool".to_string(),
                arguments: None,
            })
            .await;

        assert!(
            matches!(result, Err(McpError::Other(ref msg)) if msg.contains("Rate limit exceeded"))
        );
    }

    #[tokio::test]
    async fn test_call_tool_passes_rate_limit_when_within_burst() {
        // バースト許容量内であればレート制限チェック自体は通過し、
        // 実際のツールディスパッチ（この場合は未知のツール名なので
        // ToolNotFound）まで到達することを確認する。
        let handler = rate_limited_handler(RateLimitConfig::default());

        let result = handler
            .call_tool(ToolCallParams {
                name: "nonexistent_tool".to_string(),
                arguments: None,
            })
            .await;

        assert!(matches!(result, Err(McpError::ToolNotFound(_))));
    }

    #[test]
    fn test_is_mutating_tool_detects_known_prefixes() {
        for name in [
            "create_post",
            "create_advanced_post",
            "update_post",
            "delete_post",
            "upload_media",
            "update_media",
            "delete_media",
            "set_featured_image",
            "set_front_page",
        ] {
            assert!(is_mutating_tool(name), "{} should be mutating", name);
        }

        for name in [
            "wordpress_health_check",
            "get_posts",
            "get_pages",
            "get_all_content",
            "get_comments",
            "get_settings",
        ] {
            assert!(!is_mutating_tool(name), "{} should not be mutating", name);
        }
    }

    /// HTTPで認証していない呼び出し元には、ログのファイルのパスを出さない
    /// （#543）。ローカル（STDIO）と、認証した呼び出し元には出す
    #[tokio::test]
    async fn test_health_check_hides_log_path_from_unauthenticated_http_callers() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let call = |caller: crate::mcp::Caller| {
            let handler = &handler;
            async move {
                let result = crate::mcp::with_caller(
                    caller,
                    handler.call_tool(ToolCallParams {
                        name: "wordpress_health_check".to_string(),
                        arguments: None,
                    }),
                )
                .await
                .unwrap();
                result["content"][0]["text"].as_str().unwrap().to_string()
            }
        };
        let hidden = "Log file: shown only to authenticated callers";

        let remote = call(
            crate::mcp::Caller::unrestricted().with_ip_address(Some("203.0.113.1".to_string())),
        )
        .await;
        assert!(remote.contains(hidden), "{remote}");

        let local = call(crate::mcp::Caller::unrestricted()).await;
        assert!(local.contains("Log file: "), "{local}");
        assert!(!local.contains(hidden), "{local}");

        let authenticated = call(
            crate::mcp::Caller::new("api_key:a", crate::mcp::ToolAccess::All)
                .with_ip_address(Some("203.0.113.1".to_string())),
        )
        .await;
        assert!(!authenticated.contains(hidden), "{authenticated}");
    }

    /// リソースは、読み取り専用の実在するツールと同じ扱いになる（#538）
    #[tokio::test]
    async fn test_every_resource_maps_to_a_read_only_tool() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let tools = handler.list_tools().await.unwrap();
        let resources = handler.list_resources().await.unwrap();
        assert!(!resources.is_empty());
        for resource in resources {
            let name = handler
                .resource_tool(&resource.uri)
                .unwrap_or_else(|| panic!("{} has no tool", resource.uri));
            let tool = tools
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("{} maps to missing tool {}", resource.uri, name));
            assert_eq!(
                tool.annotations.as_ref().and_then(|a| a.read_only_hint),
                Some(true),
                "{}",
                name
            );
        }
    }

    /// ツールの説明は、AIのクライアントがツールを選ぶ材料（#676）。どの
    /// ツールにも、名前（title）と、何をするか・いつ使うか・何を返すかを
    /// 書いた説明があり、引数にも説明がある。説明は英語で書き、ほかの
    /// ツールの名前は、実在するものだけを挙げる
    #[tokio::test]
    async fn test_every_tool_is_described() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let tools = handler.list_tools().await.unwrap();
        let names: std::collections::HashSet<&str> =
            tools.iter().map(|t| t.name.as_str()).collect();
        // ツールの名前と同じ形の、ツールではない言葉（設定の項目）
        let not_tools = ["upload_allowed_dirs"];
        for tool in &tools {
            let name = &tool.name;
            let title = tool.title.as_deref().unwrap_or_default();
            assert!(
                !title.is_empty() && title.len() <= 60,
                "{name}: title {title:?}"
            );
            let description = &tool.description;
            assert!(description.len() >= 140, "{name}: description too short");
            assert!(description.ends_with('.'), "{name}: {description}");
            assert!(description.is_ascii(), "{name}: {description}");
            for word in description.split(|c: char| !(c.is_ascii_lowercase() || c == '_')) {
                let looks_like_a_tool = [
                    "get_", "create_", "update_", "delete_", "set_", "restore_", "upload_",
                ]
                .iter()
                .any(|prefix| word.starts_with(prefix));
                if looks_like_a_tool && !not_tools.contains(&word) {
                    assert!(names.contains(word), "{name}: mentions unknown tool {word}");
                }
            }
            let properties = tool.input_schema["properties"].as_object().unwrap();
            for (argument, schema) in properties {
                let text = schema["description"].as_str().unwrap_or_default();
                assert!(!text.is_empty(), "{name}.{argument}: no description");
                assert!(text.is_ascii(), "{name}.{argument}: {text}");
            }
        }
    }

    #[tokio::test]
    async fn test_every_tool_has_annotations() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let tools = handler.list_tools().await.unwrap();
        let annotations = |name: &str| {
            tools
                .iter()
                .find(|t| t.name == name)
                .and_then(|t| t.annotations.clone())
                .unwrap_or_else(|| panic!("{} has no annotations", name))
        };
        for tool in &tools {
            let a = annotations(&tool.name);
            assert_eq!(a.open_world_hint, Some(false), "{}", tool.name);
            assert_eq!(
                a.read_only_hint,
                Some(!is_mutating_tool(&tool.name)),
                "{}",
                tool.name
            );
        }

        let get = annotations("get_posts");
        assert_eq!(get.destructive_hint, None);

        let create = annotations("create_post");
        assert_eq!(create.destructive_hint, Some(false));
        assert_eq!(create.idempotent_hint, Some(false));
        assert_eq!(annotations("upload_media").destructive_hint, Some(false));

        for name in [
            "update_post",
            "delete_post",
            "set_front_page",
            "update_settings",
        ] {
            let a = annotations(name);
            assert_eq!(a.destructive_hint, Some(true), "{}", name);
            assert_eq!(a.idempotent_hint, Some(true), "{}", name);
        }
    }

    #[test]
    fn test_extract_target_id_finds_known_keys_in_priority_order() {
        let mut args = HashMap::new();
        args.insert("post_id".to_string(), serde_json::json!(42));
        assert_eq!(
            extract_target_id(&Some(args.clone())),
            Some("42".to_string())
        );

        // "id"があれば他のキーより優先される
        args.insert("id".to_string(), serde_json::json!("abc"));
        assert_eq!(extract_target_id(&Some(args)), Some("abc".to_string()));

        let mut media_args = HashMap::new();
        media_args.insert("media_id".to_string(), serde_json::json!(7));
        assert_eq!(extract_target_id(&Some(media_args)), Some("7".to_string()));

        // set_front_page/set_front_page_to_postsが使うキー
        let mut page_args = HashMap::new();
        page_args.insert("page_id".to_string(), serde_json::json!(99));
        assert_eq!(extract_target_id(&Some(page_args)), Some("99".to_string()));

        let mut posts_page_args = HashMap::new();
        posts_page_args.insert("posts_page_id".to_string(), serde_json::json!(5));
        assert_eq!(
            extract_target_id(&Some(posts_page_args)),
            Some("5".to_string())
        );

        assert_eq!(extract_target_id(&None), None);
        assert_eq!(extract_target_id(&Some(HashMap::new())), None);
    }

    #[tokio::test]
    async fn test_call_tool_audits_mutating_tool_including_on_failure() {
        // "delete_"接頭辞を持つツール名はmutatingと判定されるため、
        // 存在しないツール名でcatch-allのToolNotFoundになる場合でも
        // （ネットワークI/Oなしで）監査ログに失敗エントリが記録される
        // ことを確認する。
        let handler = rate_limited_handler(RateLimitConfig::default());
        let mut args = HashMap::new();
        args.insert("post_id".to_string(), serde_json::json!(7));

        let result = handler
            .call_tool(ToolCallParams {
                name: "delete_fake_thing_for_test".to_string(),
                arguments: Some(args),
            })
            .await;
        assert!(matches!(result, Err(McpError::ToolNotFound(_))));

        let entries = handler.audit_logger.get_all_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].level, AuditLevel::Error);
        assert_eq!(entries[0].result, Some("FAILURE".to_string()));
        assert_eq!(
            entries[0].action,
            Some("delete_fake_thing_for_test".to_string())
        );
        assert_eq!(entries[0].metadata.get("target_id"), Some(&"7".to_string()));
        assert!(entries[0].metadata.contains_key("error"));
    }

    /// 読み取りのツールも、呼び出し元・所要時間と一緒に記録する（#55）
    #[tokio::test]
    async fn test_call_tool_audits_read_tools_with_caller() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let call = || {
            handler.call_tool(ToolCallParams {
                name: "get_fake_thing_for_test".to_string(),
                arguments: None,
            })
        };

        let caller = crate::mcp::Caller::new("api_key:reader", crate::mcp::ToolAccess::All)
            .with_subject(Some("user-1".to_string()))
            .with_ip_address(Some("192.0.2.1".to_string()));
        let result = crate::mcp::with_caller(caller, call()).await;
        assert!(matches!(result, Err(McpError::ToolNotFound(_))));
        // 呼び出し元が分からない（STDIO等）場合は`local`
        let _ = call().await;

        let entries = handler.audit_logger.get_all_entries().await;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].user_id.as_deref(), Some("api_key:reader"));
        assert_eq!(
            entries[0].metadata.get("subject").map(String::as_str),
            Some("user-1")
        );
        assert!(entries[0].metadata.contains_key("duration_ms"));
        assert_eq!(entries[0].ip_address.as_deref(), Some("192.0.2.1"));
        assert_eq!(entries[0].result.as_deref(), Some("FAILURE"));
        assert_eq!(
            entries[0].action.as_deref(),
            Some("get_fake_thing_for_test")
        );
        assert_eq!(entries[1].user_id.as_deref(), Some("local"));
        assert!(!entries[1].metadata.contains_key("subject"));
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
                raw: None,
                protected: false,
            },
            content: Some(WordPressContent {
                rendered: "full content".to_string(),
                raw: None,
                protected: false,
            }),
            excerpt: Some(WordPressContent {
                rendered: long_excerpt,
                raw: None,
                protected: false,
            }),
            author: None,
            featured_media: None,
            categories: None,
            tags: None,
            extra: Default::default(),
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
                raw: None,
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

    const PNG_BYTES: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00];

    fn assert_invalid_params(result: Result<&'static str, McpError>, expected: &str) {
        assert!(
            matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains(expected)),
            "expected InvalidParams containing '{}', got: {:?}",
            expected,
            result
        );
    }

    #[test]
    fn test_file_signature_matches() {
        assert!(file_signature_matches("image/png", PNG_BYTES));
        assert!(file_signature_matches(
            "image/jpeg",
            &[0xFF, 0xD8, 0xFF, 0xE0]
        ));
        assert!(file_signature_matches("image/gif", b"GIF89a\x00\x00"));
        assert!(file_signature_matches(
            "image/webp",
            b"RIFF\x00\x00\x00\x00WEBP"
        ));
        assert!(file_signature_matches("application/pdf", b"%PDF-1.4\n"));

        // 動画・音声
        assert!(file_signature_matches(
            "video/mp4",
            b"\x00\x00\x00\x20ftypisom"
        ));
        assert!(file_signature_matches(
            "audio/mp4",
            b"\x00\x00\x00\x20ftypM4A "
        ));
        assert!(file_signature_matches(
            "video/webm",
            &[0x1A, 0x45, 0xDF, 0xA3, 0x00]
        ));
        assert!(file_signature_matches("video/ogg", b"OggS\x00"));
        assert!(file_signature_matches("audio/ogg", b"OggS\x00"));
        assert!(file_signature_matches("audio/mpeg", b"ID3\x04\x00"));
        assert!(file_signature_matches("audio/mpeg", &[0xFF, 0xFB, 0x90]));
        assert!(file_signature_matches(
            "audio/wav",
            b"RIFF\x00\x00\x00\x00WAVE"
        ));
        assert!(!file_signature_matches("video/mp4", b"<html></html>"));
        assert!(!file_signature_matches("audio/mpeg", b"MZ\x90\x00"));
        // WebP（RIFF/WEBP）をWAVと偽ることはできない
        assert!(!file_signature_matches(
            "audio/wav",
            b"RIFF\x00\x00\x00\x00WEBP"
        ));

        // PDFヘッダは先頭1024バイト以内にあればよい
        assert!(file_signature_matches(
            "application/pdf",
            b"\xEF\xBB\xBF\n%PDF-1.7\n"
        ));

        assert!(!file_signature_matches("image/png", b"not a png"));
        assert!(!file_signature_matches("application/pdf", b"<html></html>"));
        let mut late_pdf_header = vec![b' '; PDF_HEADER_SEARCH_WINDOW];
        late_pdf_header.extend_from_slice(b"%PDF-1.7");
        assert!(!file_signature_matches("application/pdf", &late_pdf_header));
        // BOM・空白以外の前置き（HTML等）を持つポリグロットは拒否する
        assert!(!file_signature_matches(
            "application/pdf",
            b"<html><script>alert(1)</script><!-- %PDF-1.7 -->"
        ));
        // シグネチャ未定義の形式はfail-closedで拒否する
        assert!(!file_signature_matches("text/plain", b"hello"));
    }

    #[test]
    fn test_validate_upload_accepts_legitimate_files() {
        assert_eq!(
            validate_upload(PNG_BYTES, "テスト画像.png", "image/png").unwrap(),
            "image/png"
        );
        assert_eq!(
            validate_upload(b"%PDF-1.7", "doc.pdf", "application/pdf").unwrap(),
            "application/pdf"
        );
        assert_eq!(
            validate_upload(b"\x00\x00\x00\x20ftypisom", "clip.mp4", "video/mp4").unwrap(),
            "video/mp4"
        );
        // 内容がMP4でも、拡張子が音声用(.m4a)ならvideo/mp4としては拒否
        assert_invalid_params(
            validate_upload(b"\x00\x00\x00\x20ftypisom", "clip.m4a", "video/mp4"),
            "extension",
        );
    }

    #[test]
    fn test_validate_upload_normalizes_mime_type_and_extension_case() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        // 大文字小文字・パラメータ・image/jpg別名はいずれもimage/jpegに正規化
        assert_eq!(
            validate_upload(&jpeg, "a.JPG", "IMAGE/JPEG").unwrap(),
            "image/jpeg"
        );
        assert_eq!(
            validate_upload(&jpeg, "a.jpeg", "image/jpeg; charset=binary").unwrap(),
            "image/jpeg"
        );
        assert_eq!(
            validate_upload(&jpeg, "a.jpg", "image/jpg").unwrap(),
            "image/jpeg"
        );
        assert_eq!(
            validate_upload(&jpeg, "a.jpe", "image/pjpeg").unwrap(),
            "image/jpeg"
        );
        assert_eq!(
            validate_upload(PNG_BYTES, "a.png", "image/x-png").unwrap(),
            "image/png"
        );
        assert_eq!(
            validate_upload(b"ID3\x04\x00", "a.mp3", "audio/mp3").unwrap(),
            "audio/mpeg"
        );
        assert_eq!(
            validate_upload(b"RIFF\x00\x00\x00\x00WAVE", "a.WAV", "audio/x-wav").unwrap(),
            "audio/wav"
        );
    }

    #[test]
    fn test_validate_upload_rejects_oversized_file() {
        let oversized = vec![0u8; MAX_UPLOAD_SIZE_BYTES + 1];
        assert_invalid_params(
            validate_upload(&oversized, "big.png", "image/png"),
            "too large",
        );
    }

    #[test]
    fn test_validate_upload_rejects_disallowed_mime_types() {
        assert_invalid_params(
            validate_upload(b"#!/bin/sh", "x.sh", "application/x-sh"),
            "not allowed",
        );
        assert_invalid_params(
            validate_upload(b"<svg><script/></svg>", "x.svg", "image/svg+xml"),
            "not allowed",
        );
        // マジックナンバーで内容を検証できないため許可しない
        assert_invalid_params(
            validate_upload(b"hello", "x.txt", "text/plain"),
            "not allowed",
        );
    }

    #[test]
    fn test_validate_upload_rejects_extension_mismatch() {
        // 内容とMIMEは正しくても、WordPressは拡張子で保存形式を決めるため
        // .htmlとして保存させられると格納型XSSになりうる
        assert_invalid_params(
            validate_upload(PNG_BYTES, "x.html", "image/png"),
            "extension",
        );
        assert_invalid_params(
            validate_upload(PNG_BYTES, "no_extension", "image/png"),
            "extension",
        );
    }

    #[test]
    fn test_validate_upload_rejects_path_separators_and_control_chars() {
        for filename in [
            "a.png\\evil.html",
            "evil.html/.png",
            "../x.png",
            "x\n.png",
            "invoice\u{202E}gpj.exe.png",
        ] {
            assert_invalid_params(
                validate_upload(PNG_BYTES, filename, "image/png"),
                "path separators or control characters",
            );
        }
    }

    #[test]
    fn test_validate_upload_rejects_overlong_filename() {
        let filename = format!("{}.png", "a".repeat(MAX_UPLOAD_FILENAME_CHARS));
        assert_invalid_params(
            validate_upload(PNG_BYTES, &filename, "image/png"),
            "too long",
        );
    }

    #[test]
    fn test_validate_upload_truncates_echoed_input() {
        let huge_mime = "x".repeat(10_000);
        let err = validate_upload(PNG_BYTES, "a.png", &huge_mime).unwrap_err();
        assert!(err.to_string().len() < 1_000, "error echoed the full input");
    }

    #[test]
    fn test_validate_upload_rejects_mime_type_spoofing() {
        assert_invalid_params(
            validate_upload(b"this is not actually a png", "fake.png", "image/png"),
            "does not match",
        );
    }

    #[tokio::test]
    async fn test_upload_media_runs_validation_before_network() {
        // upload_media自体が検証を通すこと（到達不能なダミーURLに対して
        // ネットワークI/Oが発生する前に拒否されること）を確認する
        let handler = rate_limited_handler(RateLimitConfig::default());

        let result = handler
            .upload_media(b"#!/bin/sh", "x.sh", "application/x-sh")
            .await;

        assert!(matches!(result, Err(McpError::InvalidParams(_))));
    }

    #[tokio::test]
    async fn test_call_tool_audits_upload_rejected_by_validation() {
        // アーム内の`?`で返る検証エラー（MIME偽装の試み等）も監査ログに
        // 残ること
        let handler = rate_limited_handler(RateLimitConfig::default());
        use base64::{engine::general_purpose, Engine as _};
        let mut args = HashMap::new();
        args.insert(
            "file_data".to_string(),
            serde_json::json!(general_purpose::STANDARD.encode(b"<script>x</script>")),
        );
        args.insert("filename".to_string(), serde_json::json!("x.html"));
        args.insert("mime_type".to_string(), serde_json::json!("image/png"));

        let result = handler
            .call_tool(ToolCallParams {
                name: "upload_media".to_string(),
                arguments: Some(args),
            })
            .await;
        assert!(matches!(result, Err(McpError::InvalidParams(_))));

        let entries = handler.audit_logger.get_all_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, Some("upload_media".to_string()));
        assert_eq!(entries[0].result, Some("FAILURE".to_string()));
    }

    const PNG_HEADER: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    fn handler_with_upload_dirs(dirs: &[&Path]) -> WordPressHandler {
        let config = WordPressConfig {
            // 検証で拒否されるケースのみを試すため、到達可能である必要はない
            url: "https://example.invalid".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
            keyring: None,
            upload_allowed_dirs: Some(
                dirs.iter()
                    .map(|d| d.to_string_lossy().into_owned())
                    .collect(),
            ),
            post_types: None,
            default_site: None,
            sites: Default::default(),
        };
        WordPressHandler::try_new(config).unwrap()
    }

    async fn call_upload(
        handler: &WordPressHandler,
        args: &[(&str, serde_json::Value)],
    ) -> Result<serde_json::Value, McpError> {
        let args: HashMap<String, serde_json::Value> = args
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        handler
            .call_tool(ToolCallParams {
                name: "upload_media".to_string(),
                arguments: Some(args),
            })
            .await
    }

    fn invalid_params_message(result: Result<serde_json::Value, McpError>) -> String {
        match result {
            Err(McpError::InvalidParams(msg)) => msg,
            other => panic!("expected InvalidParams, got: {:?}", other),
        }
    }

    #[test]
    fn test_canonicalize_upload_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.png");
        std::fs::write(&file, PNG_HEADER).unwrap();

        let dirs = canonicalize_upload_dirs(&[dir.path().to_string_lossy().into_owned()]).unwrap();
        assert_eq!(dirs, vec![std::fs::canonicalize(dir.path()).unwrap()]);

        assert!(canonicalize_upload_dirs(&[]).unwrap().is_empty());
        // 存在しないフォルダ・ファイルは起動時にエラー
        let missing = dir.path().join("missing").to_string_lossy().into_owned();
        assert!(canonicalize_upload_dirs(&[missing]).is_err());
        assert!(canonicalize_upload_dirs(&[file.to_string_lossy().into_owned()]).is_err());
    }

    #[test]
    fn test_resolve_upload_path() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("allowed");
        let outside = root.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(allowed.join("a.png"), PNG_HEADER).unwrap();
        std::fs::write(outside.join("secret.png"), PNG_HEADER).unwrap();
        let allowed_dirs =
            canonicalize_upload_dirs(&[allowed.to_string_lossy().into_owned()]).unwrap();
        let resolve = |p: &Path| resolve_upload_path(&p.to_string_lossy(), &allowed_dirs);

        // 許可フォルダ内のファイル
        assert_eq!(
            resolve(&allowed.join("a.png")).unwrap(),
            std::fs::canonicalize(allowed.join("a.png")).unwrap()
        );

        // 許可フォルダ外・..での脱出・存在しないファイル・フォルダ自体は、
        // いずれも同じエラー（外のファイルの有無が分からないように）
        let not_allowed = "file_path must be an existing file inside one of upload_allowed_dirs";
        for path in [
            outside.join("secret.png"),
            allowed.join("..").join("outside").join("secret.png"),
            allowed.join("missing.png"),
            allowed.clone(),
        ] {
            let msg = invalid_params_message(resolve(&path).map(|_| serde_json::Value::Null));
            assert_eq!(msg, not_allowed, "path: {}", path.display());
        }

        // 相対パスは受け付けない
        let msg = invalid_params_message(
            resolve_upload_path("a.png", &allowed_dirs).map(|_| serde_json::Value::Null),
        );
        assert!(msg.contains("absolute"), "got: {}", msg);

        // 許可フォルダ未設定なら無効
        let msg = invalid_params_message(
            resolve_upload_path(&allowed.join("a.png").to_string_lossy(), &[])
                .map(|_| serde_json::Value::Null),
        );
        assert!(msg.contains("upload_allowed_dirs"), "got: {}", msg);
    }

    #[test]
    fn test_resolve_upload_path_rejects_symlink_escaping_allowed_dir() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("allowed");
        let outside = root.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret.png"), PNG_HEADER).unwrap();
        let link = allowed.join("link.png");

        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(outside.join("secret.png"), &link).is_ok();
        // Windowsではシンボリックリンクの作成に権限（開発者モード等）が
        // 必要なため、作れない環境ではこのテストを実質スキップする
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_file(outside.join("secret.png"), &link).is_ok();
        if !created {
            eprintln!("skipping: cannot create a symlink in this environment");
            return;
        }

        let allowed_dirs =
            canonicalize_upload_dirs(&[allowed.to_string_lossy().into_owned()]).unwrap();
        assert!(resolve_upload_path(&link.to_string_lossy(), &allowed_dirs).is_err());
    }

    /// Windowsのジャンクション（ディレクトリへのリンク）は権限なしで
    /// 作れるため、Windowsでもリンクによる脱出を確実に検証できる
    #[cfg(windows)]
    #[test]
    fn test_resolve_upload_path_rejects_junction_escaping_allowed_dir() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("allowed");
        let outside = root.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret.png"), PNG_HEADER).unwrap();
        let junction = allowed.join("junction");

        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");

        let allowed_dirs =
            canonicalize_upload_dirs(&[allowed.to_string_lossy().into_owned()]).unwrap();
        let msg = invalid_params_message(
            resolve_upload_path(
                &junction.join("secret.png").to_string_lossy(),
                &allowed_dirs,
            )
            .map(|_| serde_json::Value::Null),
        );
        assert_eq!(
            msg,
            "file_path must be an existing file inside one of upload_allowed_dirs"
        );
    }

    #[test]
    fn test_mime_type_for_filename() {
        assert_eq!(mime_type_for_filename("a.png"), Some("image/png"));
        assert_eq!(mime_type_for_filename("a.JPG"), Some("image/jpeg"));
        assert_eq!(mime_type_for_filename("a.pdf"), Some("application/pdf"));
        assert_eq!(mime_type_for_filename("a.svg"), None);
        assert_eq!(mime_type_for_filename(".env"), None);
        assert_eq!(mime_type_for_filename("noext"), None);
    }

    #[tokio::test]
    async fn test_read_upload_file_rejects_oversized_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.png");
        std::fs::write(&path, vec![0u8; MAX_UPLOAD_SIZE_BYTES + 1]).unwrap();
        let msg = invalid_params_message(
            read_upload_file(&path)
                .await
                .map(|_| serde_json::Value::Null),
        );
        assert!(msg.contains("too large"), "got: {}", msg);

        std::fs::write(&path, PNG_HEADER).unwrap();
        assert_eq!(read_upload_file(&path).await.unwrap(), PNG_HEADER);
    }

    #[tokio::test]
    async fn test_upload_media_file_path_argument_validation() {
        let dir = tempfile::tempdir().unwrap();
        let fake_png = dir.path().join("fake.png");
        std::fs::write(&fake_png, b"this is not a png").unwrap();
        let handler = handler_with_upload_dirs(&[dir.path()]);
        let fake_png_arg = serde_json::json!(fake_png.to_string_lossy());

        // 拡張子だけ.pngのテキストは、中身の検証で拒否される
        let msg = invalid_params_message(
            call_upload(&handler, &[("file_path", fake_png_arg.clone())]).await,
        );
        assert!(
            msg.contains("does not match declared mime_type"),
            "got: {}",
            msg
        );

        // file_dataとfile_pathの同時指定・どちらも無し
        let msg = invalid_params_message(
            call_upload(
                &handler,
                &[
                    ("file_path", fake_png_arg),
                    ("file_data", serde_json::json!("AAAA")),
                ],
            )
            .await,
        );
        assert!(msg.contains("not both"), "got: {}", msg);
        let msg = invalid_params_message(call_upload(&handler, &[]).await);
        assert!(
            msg.contains("Missing file_data or file_path"),
            "got: {}",
            msg
        );

        // 拡張子からMIMEタイプを決められないファイル
        let env_file = dir.path().join("x.env");
        std::fs::write(&env_file, b"SECRET=1").unwrap();
        let msg = invalid_params_message(
            call_upload(
                &handler,
                &[("file_path", serde_json::json!(env_file.to_string_lossy()))],
            )
            .await,
        );
        assert!(msg.contains("Cannot determine mime_type"), "got: {}", msg);

        // upload_allowed_dirs未設定のハンドラーではfile_path自体が無効
        let disabled = handler_with_upload_dirs(&[]);
        let msg = invalid_params_message(
            call_upload(
                &disabled,
                &[("file_path", serde_json::json!(fake_png.to_string_lossy()))],
            )
            .await,
        );
        assert!(msg.contains("file_path is disabled"), "got: {}", msg);
    }

    /// 決まった応答を返し、受けたリクエスト数を数えるだけのHTTPサーバー。
    /// 再送の有無をネットワーク越しに確かめるために使う。
    async fn spawn_fixed_response_server(
        status_line: &'static str,
        body: String,
    ) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let server_count = count.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                server_count.fetch_add(1, Ordering::SeqCst);
                let body = body.clone();
                tokio::spawn(async move {
                    // ヘッダーの終わりまで読む（本文は読み捨てる）
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let response = format!(
                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        status_line,
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        (format!("http://{}", addr), count)
    }

    /// パスごとに決まった応答を返すHTTPサーバー。`routes`は（パスの先頭、
    /// 状態行、本文）で、どれにも合わなければ404
    async fn spawn_routing_server(routes: Vec<(&'static str, &'static str, String)>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let request = String::from_utf8_lossy(&buf);
                    let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                    let (status, body) = routes
                        .iter()
                        .find(|(prefix, _, _)| path.starts_with(prefix))
                        .map(|(_, status, body)| (*status, body.clone()))
                        .unwrap_or(("404 Not Found", "{}".to_string()));
                    let response = format!(
                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        status,
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn test_health_check_without_settings_access() {
        // 投稿者・寄稿者のアカウント: 設定は403でも、届いていて、残りの段も
        // 確かめる（PR #568のレビュー）
        let base = spawn_routing_server(vec![
            (
                "/wp-json/wp/v2/settings",
                "403 Forbidden",
                r#"{"code":"rest_forbidden"}"#.to_string(),
            ),
            (
                "/wp-json/wp/v2/users/me",
                "200 OK",
                r#"{"id":2,"name":"author"}"#.to_string(),
            ),
            ("/wp-json/wp/v2/posts", "200 OK", "[]".to_string()),
            ("/wp-json/wp/v2/media", "200 OK", "[]".to_string()),
            (
                "/wp-json/",
                "200 OK",
                r#"{"name":"Example Site","description":"d","url":"https://example.com","namespaces":["wp/v2"]}"#
                    .to_string(),
            ),
        ])
        .await;
        let handler = handler_for_local_server(base);
        let health = handler.health_check().await;
        assert!(health.site_accessible, "{:?}", health.error_details);
        assert!(health.rest_api_available, "{:?}", health.error_details);
        let info = health.site_info.unwrap();
        assert_eq!(info.name, "Example Site");
        assert_eq!(info.admin_email, None);
        // 設定の403は、誤りとして数えない
        assert!(
            !health
                .error_details
                .iter()
                .any(|e| e.contains("Site accessibility")),
            "{:?}",
            health.error_details
        );
    }

    #[tokio::test]
    async fn test_health_check_rest_closed_to_anonymous_users() {
        // RESTをログインした人だけに開くサイト: 認証無しの`/wp-json/`は401。
        // 認証情報を付けて読み直し、届いているとする（PR #568のレビュー）
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let request = String::from_utf8_lossy(&buf).to_ascii_lowercase();
                    let (status, body) = if !request.contains("\r\nauthorization: basic ") {
                        ("401 Unauthorized", r#"{"code":"rest_not_logged_in"}"#)
                    } else if request.starts_with("get /wp-json/ ") {
                        (
                            "200 OK",
                            r#"{"name":"Members Only","url":"https://example.com","namespaces":["wp/v2"]}"#,
                        )
                    } else {
                        ("200 OK", "[]")
                    };
                    let response = format!(
                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        status,
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        let mut handler = handler_for_local_server(format!("http://{}", addr));
        handler.username = Some("author".to_string());
        handler.password = Some("app password".to_string());
        let health = handler.health_check().await;
        assert!(health.site_accessible, "{:?}", health.error_details);
        assert!(health.rest_api_available, "{:?}", health.error_details);
        assert_eq!(health.site_info.unwrap().name, "Members Only");
    }

    #[tokio::test]
    async fn test_health_check_site_not_reachable() {
        let base = spawn_routing_server(vec![(
            "/wp-json/",
            "503 Service Unavailable",
            "{}".to_string(),
        )])
        .await;
        let handler = handler_for_local_server(base);
        let health = handler.health_check().await;
        assert!(!health.site_accessible);
        assert!(health.error_details[0].contains("Site accessibility failed"));
    }

    /// テスト用のローカルHTTPサーバーに向けたハンドラー（`try_new`は
    /// HTTPSを強制するため、フィールドを直接組み立てる）
    fn handler_for_local_server(base_url: String) -> WordPressHandler {
        WordPressHandler {
            client: Client::new(),
            base_url,
            secure_credentials: None,
            username: None,
            password: None,
            rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig::default())),
            audit_logger: Arc::new(AuditLogger::with_defaults()),
            upload_allowed_dirs: Arc::new(Vec::new()),
            allowed_post_types: Arc::new(vec!["product".to_string()]),
            type_endpoints: Default::default(),
        }
    }

    /// 接続を受け付けるだけで応答を返さないサーバー（タイムアウトの再現用）
    async fn spawn_silent_server() -> (String, Arc<std::sync::atomic::AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let server_count = count.clone();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                server_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                held.push(socket);
            }
        });
        (format!("http://{}", addr), count)
    }

    fn handler_with_timeout(base_url: String, timeout: Duration) -> WordPressHandler {
        WordPressHandler {
            client: Client::builder().timeout(timeout).build().unwrap(),
            ..handler_for_local_server(base_url)
        }
    }

    fn request_count(count: &std::sync::atomic::AtomicUsize) -> usize {
        count.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[test]
    fn test_is_safe_to_resend() {
        use reqwest::Method;
        assert!(is_safe_to_resend(&Method::GET));
        assert!(is_safe_to_resend(&Method::PUT));
        assert!(!is_safe_to_resend(&Method::POST));
        assert!(!is_safe_to_resend(&Method::DELETE));
        assert!(!is_safe_to_resend(&Method::PATCH));
    }

    #[tokio::test]
    async fn test_post_is_not_resent_on_server_error() {
        // 5xxでもアップロード（POST）は処理済みの可能性があるため再送しない
        let (base_url, count) =
            spawn_fixed_response_server("500 Internal Server Error", "{}".to_string()).await;
        let handler = handler_for_local_server(base_url);

        let result = handler.upload_media(PNG_HEADER, "a.png", "image/png").await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(ref msg)) if msg.contains("Not retried automatically")),
            "got: {:?}",
            result
        );
        assert_eq!(request_count(&count), 1);
    }

    #[tokio::test]
    async fn test_delete_is_not_resent_on_server_error() {
        let (base_url, count) =
            spawn_fixed_response_server("503 Service Unavailable", "{}".to_string()).await;
        let handler = handler_for_local_server(base_url);

        let result = handler.delete_media(1, Some(true)).await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(ref msg)) if msg.contains("Not retried automatically")),
            "got: {:?}",
            result
        );
        assert_eq!(request_count(&count), 1);
    }

    #[tokio::test]
    async fn test_get_is_resent_on_server_error() {
        // GETはこれまでどおり再送する（待機の合計は約3秒）
        let (base_url, count) =
            spawn_fixed_response_server("500 Internal Server Error", "{}".to_string()).await;
        let handler = handler_for_local_server(base_url);

        let result = handler.get_media_item(1).await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(ref msg)) if msg.contains("after 3 attempts") && !msg.contains("Not retried")),
            "got: {:?}",
            result
        );
        assert_eq!(request_count(&count), 3);
    }

    #[tokio::test]
    async fn test_post_is_resent_on_too_many_requests() {
        // 429は処理前に拒否されているため、POSTでも再送する
        let (base_url, count) =
            spawn_fixed_response_server("429 Too Many Requests", "{}".to_string()).await;
        let handler = handler_for_local_server(base_url);

        let result = handler.upload_media(PNG_HEADER, "a.png", "image/png").await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(_))),
            "got: {:?}",
            result
        );
        assert_eq!(request_count(&count), 3);
    }

    #[tokio::test]
    async fn test_post_is_not_resent_on_timeout() {
        let (base_url, count) = spawn_silent_server().await;
        let handler = handler_with_timeout(base_url, Duration::from_millis(200));

        let result = handler.upload_media(PNG_HEADER, "a.png", "image/png").await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(ref msg)) if msg.contains("Not retried automatically")),
            "got: {:?}",
            result
        );
        assert_eq!(request_count(&count), 1);
    }

    #[tokio::test]
    async fn test_get_is_resent_on_timeout() {
        let (base_url, count) = spawn_silent_server().await;
        let handler = handler_with_timeout(base_url, Duration::from_millis(200));

        let result = handler.get_media_item(1).await;
        assert!(
            matches!(result, Err(McpError::Http(_))),
            "got: {:?}",
            result
        );
        assert_eq!(request_count(&count), 3);
    }

    #[tokio::test]
    async fn test_post_is_resent_when_connection_fails() {
        // 接続できなかった場合はリクエストが届いていないため、POSTでも
        // 再送する（最後は接続エラーとして返る）
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let handler = handler_for_local_server(format!("http://{}", addr));

        let result = handler.upload_media(PNG_HEADER, "a.png", "image/png").await;
        assert!(
            matches!(result, Err(McpError::Http(ref e)) if e.is_connect()),
            "got: {:?}",
            result
        );
    }

    fn sample_media_json(id: u64) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "status": "inherit",
            "source_url": "https://example.com/wp-content/uploads/a.png"
        })
    }

    #[tokio::test]
    async fn test_delete_media_force_parses_deleted_previous_response() {
        // force=trueの応答は{deleted, previous}の形。解析でき、再送しないこと
        let body = serde_json::json!({"deleted": true, "previous": sample_media_json(383)});
        let (base_url, count) = spawn_fixed_response_server("200 OK", body.to_string()).await;
        let handler = handler_for_local_server(base_url);

        let media = handler.delete_media(383, Some(true)).await.unwrap();
        assert_eq!(media.id, Some(383));
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_delete_media_trash_response_still_parses() {
        // ゴミ箱移動に対応したサイトでは、メディアそのものが返る
        let body = sample_media_json(7);
        let (base_url, _) = spawn_fixed_response_server("200 OK", body.to_string()).await;
        let handler = handler_for_local_server(base_url);

        let media = handler.delete_media(7, None).await.unwrap();
        assert_eq!(media.id, Some(7));
    }

    #[tokio::test]
    async fn test_unparseable_success_response_is_not_resent() {
        let (base_url, count) =
            spawn_fixed_response_server("200 OK", r#"{"unexpected":true}"#.to_string()).await;
        let handler = handler_for_local_server(base_url);

        let result = handler.delete_media(1, Some(true)).await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(ref msg)) if msg.contains("processed the request")),
            "got: {:?}",
            result
        );
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_unparseable_success_response_is_not_resent_for_upload() {
        // upload_media（execute_request_with_retry_rebuilding）も同じ
        let (base_url, count) =
            spawn_fixed_response_server("201 Created", r#"{"unexpected":true}"#.to_string()).await;
        let handler = handler_for_local_server(base_url);

        let result = handler.upload_media(PNG_HEADER, "a.png", "image/png").await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(_))),
            "got: {:?}",
            result
        );
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_not_implemented_is_not_retried() {
        // メディアのゴミ箱移動に未対応のサイトは501を返す。再送しても同じ
        let body = r#"{"code":"rest_trash_not_supported","message":"The post does not support trashing. Set 'force=true' to delete."}"#;
        let (base_url, count) =
            spawn_fixed_response_server("501 Not Implemented", body.to_string()).await;
        let handler = handler_for_local_server(base_url);

        let result = handler.delete_media(1, None).await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(ref msg)) if msg.contains("rest_trash_not_supported")),
            "got: {:?}",
            result
        );
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_is_retryable_status() {
        use reqwest::StatusCode;
        assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(!is_retryable_status(StatusCode::NOT_IMPLEMENTED));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(StatusCode::OK));
    }

    #[test]
    fn test_validate_upload_rejects_empty_file() {
        let result = validate_upload(&[], "a.png", "image/png");
        assert!(
            matches!(result, Err(McpError::InvalidParams(ref msg)) if msg == "File is empty"),
            "got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_call_tool_rejects_oversized_base64_before_decoding() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let mut args = HashMap::new();
        args.insert(
            "file_data".to_string(),
            serde_json::json!("A".repeat(MAX_UPLOAD_BASE64_LEN + 4)),
        );
        args.insert("filename".to_string(), serde_json::json!("big.png"));
        args.insert("mime_type".to_string(), serde_json::json!("image/png"));

        let result = handler
            .call_tool(ToolCallParams {
                name: "upload_media".to_string(),
                arguments: Some(args),
            })
            .await;

        assert!(
            matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains("too large")),
            "got: {:?}",
            result
        );
    }

    #[test]
    fn test_is_edit_context_denied_matches_format_wp_error_output() {
        // 編集権限の不足（rest_forbidden_context）に加え、WAF等が別のコードや
        // HTMLで拒否した場合も、表示用コンテキストで取り直す
        for (status, body) in [
            (
                reqwest::StatusCode::UNAUTHORIZED,
                r#"{"code":"rest_forbidden_context","message":"Sorry, you are not allowed to edit this post."}"#,
            ),
            (
                reqwest::StatusCode::FORBIDDEN,
                r#"{"code":"rest_forbidden_context","message":"Sorry, you are not allowed to edit this post."}"#,
            ),
            (
                reqwest::StatusCode::FORBIDDEN,
                "<html><body>Blocked by security plugin</body></html>",
            ),
        ] {
            assert!(is_edit_context_denied(&McpError::ExternalApi(
                format_wp_error(status, body)
            )));
        }

        // 存在しない投稿やサーバーエラー、接続エラーでは取り直さない
        assert!(!is_edit_context_denied(&McpError::ExternalApi(
            format_wp_error(
                reqwest::StatusCode::NOT_FOUND,
                r#"{"code":"rest_post_invalid_id","message":"Invalid post ID."}"#,
            )
        )));
        assert!(!is_edit_context_denied(&McpError::ExternalApi(format!(
            "{} (after 3 attempts)",
            format_wp_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "")
        ))));
        assert!(!is_edit_context_denied(&McpError::Other(
            "WordPress API error 401".to_string()
        )));
    }

    #[test]
    fn test_post_content_format_parse() {
        use serde_json::json;
        assert_eq!(
            PostContentFormat::parse(None).unwrap(),
            PostContentFormat::Raw
        );
        assert_eq!(
            PostContentFormat::parse(Some(&json!(null))).unwrap(),
            PostContentFormat::Raw
        );
        assert_eq!(
            PostContentFormat::parse(Some(&json!("rendered"))).unwrap(),
            PostContentFormat::Rendered
        );
        assert_eq!(
            PostContentFormat::parse(Some(&json!("text"))).unwrap(),
            PostContentFormat::Text
        );
        // 未知の文字列も、文字列以外の値も、既定値で黙って続けずにエラーにする
        for invalid in [json!("full"), json!(1), json!(true)] {
            assert!(matches!(
                PostContentFormat::parse(Some(&invalid)),
                Err(McpError::InvalidParams(_))
            ));
        }
    }
    #[test]
    fn test_post_content_for_display_returns_full_content() {
        let paragraph = "日本語の本文です。".repeat(50);
        let raw = format!(
            "<!-- wp:paragraph -->\n<p>{}</p>\n<!-- /wp:paragraph -->",
            paragraph
        );
        let content = WordPressContent {
            rendered: format!("<p>{}</p>\n", paragraph),
            raw: Some(raw.clone()),
            protected: false,
        };

        // 原文はブロック区切りを含めてそのまま、切り詰めずに返す
        assert_eq!(
            post_content_for_display(&content, PostContentFormat::Raw),
            raw
        );
        assert_eq!(
            post_content_for_display(&content, PostContentFormat::Rendered),
            content.rendered
        );

        // プレーンテキストも100文字で切り詰めない
        let text = post_content_for_display(&content, PostContentFormat::Text);
        assert_eq!(text.trim(), paragraph);
        assert!(text.chars().count() > 100);
    }

    fn post_with_content(content: WordPressContent) -> WordPressPost {
        WordPressPost {
            id: Some(1),
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
                rendered: "Don&#8217;t".to_string(),
                raw: content.raw.as_ref().map(|_| "Don't".to_string()),
                protected: false,
            },
            content: Some(content),
            excerpt: None,
            author: None,
            featured_media: None,
            categories: None,
            tags: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn test_featured_media_label() {
        // WordPressは未設定を0で返す。欠落も未設定として扱う
        assert_eq!(featured_media_label(Some(0), None), "none");
        assert_eq!(featured_media_label(None, None), "none");
        assert_eq!(
            featured_media_label(Some(0), Some("https://example.com/a.png")),
            "none"
        );
        // URLが取れた場合はIDとURL
        assert_eq!(
            featured_media_label(Some(123), Some("https://example.com/a.png")),
            "123 (https://example.com/a.png)"
        );
        // URLが取れなかった（メディア取得失敗・空文字）場合はIDのみ
        assert_eq!(featured_media_label(Some(123), None), "123");
        assert_eq!(featured_media_label(Some(123), Some("")), "123");
    }

    #[test]
    fn test_post_format_label_reports_raw_fallback() {
        let with_raw = post_with_content(WordPressContent {
            rendered: "<p>x</p>".to_string(),
            raw: Some("x".to_string()),
            protected: false,
        });
        assert_eq!(post_format_label(&with_raw, PostContentFormat::Raw), "raw");
        assert_eq!(
            post_content_for_display(&with_raw.title, PostContentFormat::Raw),
            "Don't"
        );

        // 原文を要求したが取れなかった場合、タイトル・本文とも表示用HTMLに
        // なり、ラベルでその旨を示す
        let without_raw = post_with_content(WordPressContent {
            rendered: "<p>x</p>".to_string(),
            raw: None,
            protected: false,
        });
        assert!(post_format_label(&without_raw, PostContentFormat::Raw)
            .contains("raw source unavailable"));
        assert_eq!(
            post_content_for_display(&without_raw.title, PostContentFormat::Raw),
            "Don&#8217;t"
        );
        assert_eq!(
            post_format_label(&without_raw, PostContentFormat::Rendered),
            "rendered HTML"
        );
    }

    #[test]
    fn test_protected_content_note() {
        // 閲覧パスワード付きで、表示用コンテキストのため本文が空のとき
        let hidden = post_with_content(WordPressContent {
            rendered: String::new(),
            raw: None,
            protected: true,
        });
        assert!(protected_content_note(&hidden).is_some());

        // 編集用コンテキストで原文が取れていれば注意書きは不要
        let editable = post_with_content(WordPressContent {
            rendered: String::new(),
            raw: Some("secret body".to_string()),
            protected: true,
        });
        assert!(protected_content_note(&editable).is_none());

        let public = post_with_content(WordPressContent {
            rendered: "<p>x</p>".to_string(),
            raw: None,
            protected: false,
        });
        assert!(protected_content_note(&public).is_none());
    }

    #[test]
    fn test_wordpress_content_raw_is_optional_and_not_serialized_when_absent() {
        // context=viewの応答（rawなし）とcontext=editの応答（rawあり）の両方を読める
        let view: WordPressContent =
            serde_json::from_str(r#"{"rendered":"<p>x</p>","protected":false}"#).unwrap();
        assert_eq!(view.raw, None);
        let edit: WordPressContent =
            serde_json::from_str(r#"{"rendered":"<p>x</p>","raw":"x","protected":false}"#).unwrap();
        assert_eq!(edit.raw.as_deref(), Some("x"));

        // rawがNoneのときは送信内容に含めない（既存の送信内容を変えない）
        let serialized = serde_json::to_value(&view).unwrap();
        assert!(serialized.get("raw").is_none());
    }

    #[tokio::test]
    async fn test_get_post_rejects_invalid_format_before_network() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let mut args = HashMap::new();
        args.insert("post_id".to_string(), serde_json::json!(1));
        args.insert("format".to_string(), serde_json::json!("full"));

        let result = handler
            .call_tool(ToolCallParams {
                name: "get_post".to_string(),
                arguments: Some(args),
            })
            .await;

        assert!(matches!(result, Err(McpError::InvalidParams(_))));
    }

    #[test]
    fn test_advanced_post_body_sends_title_and_content_as_strings() {
        // create_post_with_featured_imageは以前、読み取り用のWordPressPostを
        // そのまま送っており、タイトル・本文が{"rendered": ...}になっていた。
        // WordPressは書き込み時にこれを読まず、HTTP 400（empty_content）に
        // なっていた。現在はこの関数を経由する
        let body = advanced_post_body(PostCreateParams {
            title: "Hello".to_string(),
            content: "<p>Body</p>".to_string(),
            featured_media_id: Some(42),
            ..Default::default()
        });

        assert_eq!(body["title"], serde_json::json!("Hello"));
        assert_eq!(body["content"], serde_json::json!("<p>Body</p>"));
        assert_eq!(body["featured_media"], serde_json::json!(42));
        assert_eq!(body["status"], serde_json::json!("draft"));
    }

    #[test]
    fn test_advanced_post_body_sends_slug_only_when_given() {
        let body = advanced_post_body(PostCreateParams {
            title: "こんにちは".to_string(),
            slug: Some("hello".to_string()),
            ..Default::default()
        });
        assert_eq!(body["slug"], serde_json::json!("hello"));

        // 省略時は送らない（WordPressにタイトルから作らせる）
        let body = advanced_post_body(PostCreateParams::default());
        assert!(body.get("slug").is_none());
    }

    #[test]
    fn test_optional_slug_arg() {
        use serde_json::json;
        let mut args = HashMap::new();
        assert_eq!(optional_slug_arg(&args).unwrap(), None);

        args.insert("slug".to_string(), json!(null));
        assert_eq!(optional_slug_arg(&args).unwrap(), None);

        args.insert("slug".to_string(), json!("  my-first-post "));
        assert_eq!(
            optional_slug_arg(&args).unwrap().as_deref(),
            Some("my-first-post")
        );

        // 空文字列はWordPressでは「自動生成に戻す」意味になるため拒否する
        args.insert("slug".to_string(), json!("   "));
        assert!(matches!(
            optional_slug_arg(&args),
            Err(McpError::InvalidParams(_))
        ));

        args.insert("slug".to_string(), json!(123));
        assert!(matches!(
            optional_slug_arg(&args),
            Err(McpError::InvalidParams(_))
        ));
    }

    #[test]
    fn test_post_slug_label() {
        let post_with_slug = |slug: Option<&str>| -> WordPressPost {
            serde_json::from_value(serde_json::json!({
                "id": 1,
                "slug": slug,
                "status": "draft",
                "title": {"rendered": ""},
                "content": {"rendered": ""}
            }))
            .unwrap()
        };

        assert_eq!(post_slug_label(&post_with_slug(Some("hello"))), "hello");
        // 日本語のスラッグはパーセントエンコードされて返るのでデコードする
        assert_eq!(
            post_slug_label(&post_with_slug(Some("%e3%83%86%e3%82%b9%e3%83%88"))),
            "テスト"
        );
        // スラッグ未指定の下書きは空文字列で返る
        assert!(post_slug_label(&post_with_slug(Some(""))).starts_with("none"));
        assert!(post_slug_label(&post_with_slug(None)).starts_with("none"));
    }

    /// 受けたリクエストの本文を記録し、決まった応答を返すHTTPサーバー
    async fn spawn_body_capturing_server(
        body: String,
    ) -> (String, Arc<tokio::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_captured = captured.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let body = body.clone();
                let captured = server_captured.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let header_end = loop {
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            break pos + 4;
                        }
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    };
                    let headers = String::from_utf8_lossy(&buf[..header_end]).to_lowercase();
                    let content_length = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    while buf.len() < header_end + content_length {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    captured.lock().await.push(
                        String::from_utf8_lossy(&buf[header_end..header_end + content_length])
                            .to_string(),
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        (format!("http://{}", addr), captured)
    }

    /// パスの先頭で応答を選び（どれも200 OK）、届いた要求の
    /// `メソッド パス`と本文を記録するサーバー（#684）
    async fn spawn_routing_capturing_server(
        routes: Vec<(&'static str, String)>,
    ) -> (String, Arc<tokio::sync::Mutex<Vec<(String, String)>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_captured = captured.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                let captured = server_captured.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let header_end = loop {
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            break pos + 4;
                        }
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    };
                    let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
                    let content_length = headers
                        .to_lowercase()
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    while buf.len() < header_end + content_length {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let mut words = headers.split_whitespace();
                    let method = words.next().unwrap_or("").to_string();
                    let path = words.next().unwrap_or("").to_string();
                    captured.lock().await.push((
                        format!("{method} {path}"),
                        String::from_utf8_lossy(&buf[header_end..header_end + content_length])
                            .to_string(),
                    ));
                    let (status, body) = routes
                        .iter()
                        .find(|(prefix, _)| path.starts_with(prefix))
                        .map(|(_, body)| ("200 OK", body.clone()))
                        .unwrap_or(("404 Not Found", "{}".to_string()));
                    let response = format!(
                        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        status,
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        (format!("http://{}", addr), captured)
    }

    fn post_response_json(slug: &str) -> String {
        serde_json::json!({
            "id": 7,
            "slug": slug,
            "status": "draft",
            "type": "post",
            "title": {"rendered": "テスト"},
            "content": {"rendered": "<p>本文</p>"}
        })
        .to_string()
    }

    fn tool_text(result: &serde_json::Value) -> &str {
        result["content"][0]["text"].as_str().unwrap()
    }

    #[tokio::test]
    async fn test_update_post_tool_sends_slug_and_shows_actual_slug() {
        let (base_url, captured) =
            spawn_body_capturing_server(post_response_json("my-first-post")).await;
        let handler = handler_for_local_server(base_url);

        let args: HashMap<String, serde_json::Value> = [
            ("post_id".to_string(), serde_json::json!(7)),
            ("slug".to_string(), serde_json::json!("my-first-post")),
        ]
        .into_iter()
        .collect();
        let result = handler
            .call_tool(ToolCallParams {
                name: "update_post".to_string(),
                arguments: Some(args),
            })
            .await
            .unwrap();

        let bodies = captured.lock().await;
        assert_eq!(bodies.len(), 1);
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        // スラッグだけを送り、タイトル・本文等は送らない
        assert_eq!(sent, serde_json::json!({"slug": "my-first-post"}));
        assert!(tool_text(&result).contains("Slug: my-first-post"));
    }

    #[test]
    fn test_extract_youtube_id_forms() {
        for url in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ&t=10s",
            "https://youtu.be/dQw4w9WgXcQ?si=abc",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://youtube.com/shorts/dQw4w9WgXcQ?feature=share",
            "https://www.youtube.com/live/dQw4w9WgXcQ",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
            // `v`が問い合わせの先頭でなくてもよい
            "https://www.youtube.com/watch?feature=shared&v=dQw4w9WgXcQ",
            // ホストの大文字・末尾のスラッシュ
            "https://WWW.YouTube.com/shorts/dQw4w9WgXcQ/",
            "https://youtu.be/dQw4w9WgXcQ/",
            "https://www.youtube.com/watch/?v=dQw4w9WgXcQ",
        ] {
            assert_eq!(
                WordPressHandler::extract_youtube_id(url).as_deref(),
                Some("dQw4w9WgXcQ"),
                "{}",
                url
            );
            assert!(WordPressHandler::validate_youtube_url(url), "{}", url);
        }
        for url in [
            "https://vimeo.com/123456789",
            "https://www.youtube.com/@channel",
            "https://www.youtube.com/watch?v=",
            "https://www.youtube.com/watch?feature=shared",
            "https://www.youtube.com/shorts/",
            // パスの中の文字列・似たホスト・ユーザー名に引っかからない
            "https://example.org/youtube.com/shorts/dQw4w9WgXcQ",
            "https://notyoutube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com.example.org/watch?v=dQw4w9WgXcQ",
            "https://youtube.com@example.org/watch?v=dQw4w9WgXcQ",
            "https://example.org/?u=youtu.be/dQw4w9WgXcQ",
            // nocookieは`/embed/`だけ
            "https://www.youtube-nocookie.com/watch?v=dQw4w9WgXcQ",
            // IDの後ろに余分な要素がある
            "https://www.youtube.com/shorts/dQw4w9WgXcQ/extra",
            "https://www.youtube.com/watch/extra?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ/extra",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ/extra",
            "https://youtu.be/",
            // `http`・`https`以外・IDに使えない文字
            "javascript://youtube.com/%0Aalert(1)//watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ%22onload%3D",
        ] {
            assert_eq!(WordPressHandler::extract_youtube_id(url), None, "{}", url);
            assert!(!WordPressHandler::validate_youtube_url(url), "{}", url);
        }
    }

    #[test]
    fn test_validate_social_url_checks_host() {
        for (url, service) in [
            ("https://twitter.com/user/status/1", "twitter"),
            ("https://x.com/user/status/1", "twitter"),
            ("https://mobile.x.com/user/status/1", "twitter"),
            ("https://www.instagram.com/p/ABC123xyz/", "instagram"),
            ("https://www.facebook.com/user/posts/1", "facebook"),
            ("https://www.tiktok.com/@user/video/1", "tiktok"),
            ("https://vm.tiktok.com/ZM123/", "tiktok"),
        ] {
            assert_eq!(
                WordPressHandler::validate_social_url(url),
                Some(service),
                "{}",
                url
            );
        }
        for url in [
            "https://example.org/x.com/user/status/1",
            // 以前は`x.com/`を含むので受け付けていた
            "https://www.netflix.com/title/1",
            "https://x.com.example.org/user/status/1",
            "https://notfacebook.com/user",
            // Instagramは投稿（`/p/`）だけ
            "https://www.instagram.com/user/",
            "ftp://x.com/user/status/1",
        ] {
            assert_eq!(WordPressHandler::validate_social_url(url), None, "{}", url);
        }
    }

    #[tokio::test]
    async fn test_create_post_with_embeds_lists_skipped_urls() {
        let (base_url, captured) = spawn_body_capturing_server(post_response_json("hello")).await;
        let handler = handler_for_local_server(base_url);

        let args: HashMap<String, serde_json::Value> = [
            ("title".to_string(), serde_json::json!("Videos")),
            ("content".to_string(), serde_json::json!("<p>Intro</p>")),
            (
                "youtube_urls".to_string(),
                serde_json::json!([
                    "https://www.youtube.com/shorts/abc123_-X",
                    "https://vimeo.com/123456789"
                ]),
            ),
            (
                "social_urls".to_string(),
                serde_json::json!([
                    "https://www.tiktok.com/@user/video/1",
                    "https://example.org/x.com/user/status/1"
                ]),
            ),
        ]
        .into_iter()
        .collect();
        let result = handler
            .call_tool(ToolCallParams {
                name: "create_post_with_embeds".to_string(),
                arguments: Some(args),
            })
            .await
            .unwrap();

        let bodies = captured.lock().await;
        assert_eq!(bodies.len(), 1);
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        let content = sent["content"].as_str().unwrap();
        // Shortsは動画プレーヤーになり、対応しないURLは本文に入らない
        assert!(
            content.contains("https://www.youtube.com/embed/abc123_-X"),
            "{}",
            content
        );
        assert!(content.contains("https://www.tiktok.com/@user/video/1"));
        assert!(!content.contains("vimeo.com"), "{}", content);
        assert!(!content.contains("example.org"), "{}", content);

        let text = tool_text(&result);
        assert!(
            text.contains(
                "Not embedded (unsupported URL): https://vimeo.com/123456789, https://example.org/x.com/user/status/1"
            ),
            "{}",
            text
        );
    }

    #[tokio::test]
    async fn test_create_post_tools_send_slug() {
        for (tool, extra) in [
            ("create_post", None),
            ("create_advanced_post", None),
            (
                "create_post_with_featured_image",
                Some(("featured_media_id", 5)),
            ),
            ("create_post_with_categories_tags", None),
            ("create_post_with_embeds", None),
        ] {
            // WordPressが重複を避けて変えた、実際のスラッグを表示すること
            let (base_url, captured) =
                spawn_body_capturing_server(post_response_json("hello-2")).await;
            let handler = handler_for_local_server(base_url);

            let mut args: HashMap<String, serde_json::Value> = [
                ("title".to_string(), serde_json::json!("こんにちは")),
                ("content".to_string(), serde_json::json!("本文")),
                ("slug".to_string(), serde_json::json!("hello")),
            ]
            .into_iter()
            .collect();
            if let Some((key, value)) = extra {
                args.insert(key.to_string(), serde_json::json!(value));
            }

            let result = handler
                .call_tool(ToolCallParams {
                    name: tool.to_string(),
                    arguments: Some(args),
                })
                .await
                .unwrap_or_else(|e| panic!("{} failed: {:?}", tool, e));

            let bodies = captured.lock().await;
            assert_eq!(bodies.len(), 1, "{}", tool);
            let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
            assert_eq!(sent["slug"], serde_json::json!("hello"), "{}", tool);
            assert!(
                tool_text(&result).contains("Slug: hello-2"),
                "{}: {}",
                tool,
                tool_text(&result)
            );
        }
    }

    #[tokio::test]
    async fn test_post_tools_declare_slug_argument() {
        let handler = handler_for_local_server("http://127.0.0.1:1".to_string());
        let tools = handler.list_tools().await.unwrap();
        for name in [
            "create_post",
            "create_advanced_post",
            "create_post_with_featured_image",
            "create_post_with_categories_tags",
            "create_post_with_embeds",
            "update_post",
        ] {
            let tool = tools.iter().find(|t| t.name == name).unwrap();
            assert_eq!(
                tool.input_schema["properties"]["slug"]["type"],
                serde_json::json!("string"),
                "{}",
                name
            );
        }
    }

    #[test]
    fn test_post_status_or_draft() {
        use serde_json::json;
        let mut args = HashMap::new();
        assert_eq!(post_status_or_draft(&args).unwrap(), "draft");

        args.insert("status".to_string(), json!(null));
        assert_eq!(post_status_or_draft(&args).unwrap(), "draft");

        args.insert("status".to_string(), json!("publish"));
        assert_eq!(post_status_or_draft(&args).unwrap(), "publish");

        // 文字列以外は、既定値で黙って続けずに入力エラーにする
        args.insert("status".to_string(), json!(1));
        assert!(matches!(
            post_status_or_draft(&args),
            Err(McpError::InvalidParams(_))
        ));
    }

    #[test]
    fn test_validate_post_status() {
        // 標準のステータスに加え、プラグインが追加する独自のステータスも
        // 拒否しない（不正な値はWordPressのエラーとして返る）
        for status in ["publish", "draft", "pending", "private", "pitch"] {
            assert!(validate_post_status(status, None).is_ok(), "{}", status);
        }
        // 予約投稿は日付があれば受け付ける
        assert!(validate_post_status("future", Some("2030-01-01T09:00:00")).is_ok());

        // 日付なし（空文字を含む）のfutureは、WordPressが即座に公開して
        // しまうため拒否する。日付を指定できるツールも案内する
        for date in [None, Some(""), Some("  ")] {
            assert!(
                matches!(
                    validate_post_status("future", date),
                    Err(McpError::InvalidParams(ref msg)) if msg.contains("create_advanced_post")
                ),
                "date: {:?}",
                date
            );
        }
    }

    #[tokio::test]
    async fn test_create_advanced_post_validates_status_before_network() {
        // 投稿作成系のツールはすべてcreate_advanced_postを経由するため、
        // そこでの検証がすべてのツールに効く（到達不能なホストへの
        // ネットワークI/Oより前に拒否されること）
        let handler = rate_limited_handler(RateLimitConfig::default());
        let result = handler
            .create_advanced_post(PostCreateParams {
                title: "t".to_string(),
                content: "c".to_string(),
                status: "future".to_string(),
                ..Default::default()
            })
            .await;
        assert!(matches!(result, Err(McpError::InvalidParams(_))));

        let mut args = HashMap::new();
        args.insert("title".to_string(), serde_json::json!("t"));
        args.insert("content".to_string(), serde_json::json!("c"));
        args.insert("featured_media_id".to_string(), serde_json::json!(1));
        args.insert("status".to_string(), serde_json::json!("future"));
        let result = handler
            .call_tool(ToolCallParams {
                name: "create_post_with_featured_image".to_string(),
                arguments: Some(args),
            })
            .await;
        assert!(matches!(result, Err(McpError::InvalidParams(_))));
    }

    #[test]
    fn test_post_create_params_default_status_is_draft() {
        assert_eq!(PostCreateParams::default().status, "draft");
    }

    #[tokio::test]
    async fn test_post_creation_tools_document_draft_default() {
        // ツール定義上の既定値も、実装の既定（draft）とそろっていること
        let tools = rate_limited_handler(RateLimitConfig::default())
            .list_tools()
            .await
            .unwrap();
        for name in [
            "create_advanced_post",
            "create_post_with_embeds",
            "create_post_with_featured_image",
        ] {
            let tool = tools.iter().find(|t| t.name == name).unwrap();
            assert_eq!(
                tool.input_schema["properties"]["status"]["default"],
                serde_json::json!("draft"),
                "{} should default to draft",
                name
            );
        }
    }

    // ---- #625: 既存の投稿を予約投稿にする ----

    fn post_json_with(status: &str, date: &str) -> String {
        serde_json::json!({
            "id": 7,
            "slug": "post",
            "status": status,
            "date": date,
            "type": "post",
            "title": {"rendered": "テスト"},
            "content": {"rendered": "<p>本文</p>"}
        })
        .to_string()
    }

    async fn call_update_post(
        handler: &WordPressHandler,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let args: HashMap<String, serde_json::Value> =
            serde_json::from_value(args).expect("arguments must be an object");
        handler
            .call_tool(ToolCallParams {
                name: "update_post".to_string(),
                arguments: Some(args),
            })
            .await
    }

    /// `date`と`future`を送ると、予約投稿になる。結果に公開日時を出す
    #[tokio::test]
    async fn test_update_post_schedules_with_date() {
        let (base_url, captured) =
            spawn_body_capturing_server(post_json_with("future", "2030-01-01T10:00:00")).await;
        let handler = handler_for_local_server(base_url);
        let result = call_update_post(
            &handler,
            serde_json::json!({
                "post_id": 7,
                "status": "future",
                "date": "2030-01-01T10:00:00"
            }),
        )
        .await
        .unwrap();

        let bodies = captured.lock().await;
        // 日付があれば、投稿を取りに行かずに更新だけを送る
        assert_eq!(bodies.len(), 1);
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"status": "future", "date": "2030-01-01T10:00:00"})
        );
        assert!(
            tool_text(&result).contains("Date: 2030-01-01T10:00:00 (site time, scheduled)"),
            "{}",
            tool_text(&result)
        );
    }

    /// 日付だけを変える（予約の日時の変更・公開日時の修正）
    #[tokio::test]
    async fn test_update_post_sends_date_only() {
        let (base_url, captured) =
            spawn_body_capturing_server(post_json_with("draft", "2030-02-01T09:00:00")).await;
        let handler = handler_for_local_server(base_url);
        call_update_post(
            &handler,
            serde_json::json!({"post_id": 7, "date": "2030-02-01T09:00:00"}),
        )
        .await
        .unwrap();
        let bodies = captured.lock().await;
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        assert_eq!(sent, serde_json::json!({"date": "2030-02-01T09:00:00"}));
    }

    /// 日付の無い`future`は、予約済みでない投稿なら断り、更新を送らない
    /// （WordPressがすぐに公開してしまうため）
    #[tokio::test]
    async fn test_update_post_refuses_future_without_date_for_a_draft() {
        for date in [None, Some(""), Some("  ")] {
            let (base_url, captured) =
                spawn_body_capturing_server(post_json_with("draft", "2026-11-18T23:00:00")).await;
            let handler = handler_for_local_server(base_url);
            let mut args = serde_json::json!({"post_id": 7, "status": "future"});
            if let Some(date) = date {
                args["date"] = serde_json::json!(date);
            }
            let result = call_update_post(&handler, args).await;
            assert!(
                matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains("requires a publish date")),
                "date {:?}: {:?}",
                date,
                result
            );
            // 今の状態を確かめる読み取り（本文の無いGET）だけで、更新は送らない
            let bodies = captured.lock().await;
            assert!(bodies.iter().all(|body| body.is_empty()), "{:?}", bodies);
        }
    }

    /// 既に予約済みの投稿なら、日付を送らずに`future`のまま更新できる
    #[tokio::test]
    async fn test_update_post_keeps_an_already_scheduled_post() {
        let (base_url, captured) =
            spawn_body_capturing_server(post_json_with("future", "2030-01-01T10:00:00")).await;
        let handler = handler_for_local_server(base_url);
        call_update_post(
            &handler,
            serde_json::json!({"post_id": 7, "status": "future", "title": "新しい題"}),
        )
        .await
        .unwrap();
        let bodies = captured.lock().await;
        let sent: Vec<serde_json::Value> = bodies
            .iter()
            .filter(|body| !body.is_empty())
            .map(|body| serde_json::from_str(body).unwrap())
            .collect();
        assert_eq!(
            sent,
            vec![serde_json::json!({"title": "新しい題", "status": "future"})]
        );
    }

    #[test]
    fn test_post_date_label() {
        let post = |status: &str, date: Option<&str>| -> WordPressPost {
            let mut value: serde_json::Value =
                serde_json::from_str(&post_json_with(status, "x")).unwrap();
            value["date"] = serde_json::json!(date);
            serde_json::from_value(value).unwrap()
        };
        assert_eq!(
            post_date_label(&post("future", Some("2030-01-01T10:00:00"))),
            "2030-01-01T10:00:00 (site time, scheduled)"
        );
        assert_eq!(
            post_date_label(&post("publish", Some("2026-11-18T19:00:00"))),
            "2026-11-18T19:00:00 (site time)"
        );
        assert_eq!(post_date_label(&post("draft", None)), "N/A");
    }

    #[test]
    fn test_post_status_filter_arg() {
        let args = |value: serde_json::Value| -> HashMap<String, serde_json::Value> {
            [("status".to_string(), value)].into_iter().collect()
        };
        assert_eq!(post_status_filter_arg(&HashMap::new()).unwrap(), None);
        for ok in ["future", "future,draft", "any", " publish "] {
            assert_eq!(
                post_status_filter_arg(&args(serde_json::json!(ok))).unwrap(),
                Some(ok.trim().to_string())
            );
        }
        // URLに入る値なので、状態の名前以外は断る
        for bad in [
            serde_json::json!("future&per_page=1"),
            serde_json::json!("future,"),
            serde_json::json!("Future"),
            serde_json::json!(""),
            serde_json::json!(3),
        ] {
            assert!(
                matches!(
                    post_status_filter_arg(&args(bad.clone())),
                    Err(McpError::InvalidParams(_))
                ),
                "{}",
                bad
            );
        }
    }

    /// `get_posts`の`status`を、WordPressの一覧の`status`で送る
    #[tokio::test]
    async fn test_get_posts_with_status_lists_scheduled_posts() {
        let list = format!("[{}]", post_json_with("future", "2030-01-01T10:00:00"));
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/posts?per_page=100&page=1&status=future",
            "200 OK",
            list,
        )])
        .await;
        let handler = handler_for_local_server(base);
        let args: HashMap<String, serde_json::Value> =
            [("status".to_string(), serde_json::json!("future"))]
                .into_iter()
                .collect();
        let result = handler
            .call_tool(ToolCallParams {
                name: "get_posts".to_string(),
                arguments: Some(args),
            })
            .await
            .unwrap();
        let text = tool_text(&result);
        assert!(text.starts_with("Found 1 posts"), "{}", text);
        assert!(text.contains("\"status\": \"future\""), "{}", text);
        assert!(text.contains("2030-01-01T10:00:00"), "{}", text);
    }

    // ---- #598: 応答の大きさ・ページ数の上限 ----

    fn response_with(body: &str) -> reqwest::Response {
        reqwest::Response::from(http::Response::new(body.to_string()))
    }

    #[tokio::test]
    async fn test_read_body_limited() {
        assert_eq!(
            read_body_limited(response_with("hello"), 5).await.unwrap(),
            "hello"
        );
        let err = read_body_limited(response_with("hello!"), 5)
            .await
            .unwrap_err();
        assert!(
            matches!(err, McpError::ExternalApi(ref msg) if msg.contains("larger than")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn test_read_error_body_reads_only_the_beginning() {
        let big = "x".repeat(MAX_ERROR_BODY_BYTES + 10);
        assert_eq!(
            read_error_body(response_with(&big)).await.len(),
            MAX_ERROR_BODY_BYTES
        );
        assert_eq!(read_error_body(response_with("{}")).await, "{}");
    }

    /// `X-WP-TotalPages`がとても大きくても、`MAX_LIST_PAGES`ページで止める
    #[tokio::test]
    async fn test_fetch_all_pages_stops_at_the_page_limit() {
        use std::sync::atomic::Ordering;
        // 状態行の後ろに、ページ数のヘッダーを足す
        let (base, count) = spawn_fixed_response_server(
            "200 OK\r\nX-WP-TotalPages: 4294967295\r\nX-WP-Total: 1",
            "[]".to_string(),
        )
        .await;
        let handler = handler_for_local_server(base);
        let posts = handler.get_posts(None).await.unwrap();
        assert!(posts.is_empty());
        assert_eq!(count.load(Ordering::SeqCst), MAX_LIST_PAGES as usize);
    }

    /// 大きすぎる応答は、エラーにする（読み切らない）
    #[tokio::test]
    async fn test_too_large_response_is_an_error() {
        let (base, _count) =
            spawn_fixed_response_server("200 OK", "x".repeat(MAX_RESPONSE_BYTES + 1)).await;
        let handler = handler_for_local_server(base);
        let err = handler.get_posts(None).await.unwrap_err();
        assert!(
            matches!(err, McpError::ExternalApi(ref msg) if msg.contains("larger than 32 MB")),
            "{err:?}"
        );
    }

    // ---- #637: 検索・絞り込み・ページ ----

    fn list_args(value: serde_json::Value) -> HashMap<String, serde_json::Value> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn test_post_list_query_arg() {
        // 何も無ければ、今までどおり全件
        let query = post_list_query_arg(&HashMap::new(), true).unwrap();
        assert_eq!(query, PostListQuery::default());
        assert!(!query.is_single_page());
        // statusだけも、全件（#625）
        let query =
            post_list_query_arg(&list_args(serde_json::json!({"status": "future"})), true).unwrap();
        assert!(!query.is_single_page());

        let query = post_list_query_arg(
            &list_args(serde_json::json!({
                "search": " hello world ",
                "categories": [3, 4],
                "tags": 7,
                "author": 2,
                "after": "2026-11-01",
                "before": "2026-11-30T23:59:59",
                "orderby": "relevance",
                "order": "asc",
                "per_page": 5,
                "page": 2
            })),
            true,
        )
        .unwrap();
        assert!(query.is_single_page());
        assert_eq!(query.search.as_deref(), Some("hello world"));
        assert_eq!(query.categories, vec![3, 4]);
        assert_eq!(query.tags, vec![7]);
        assert_eq!(query.after.as_deref(), Some("2026-11-01T00:00:00"));
        assert_eq!(query.before.as_deref(), Some("2026-11-30T23:59:59"));
        assert_eq!((query.per_page, query.page), (Some(5), Some(2)));

        for (bad, why) in [
            (serde_json::json!({"categories": [0]}), "positive integer"),
            (serde_json::json!({"categories": []}), "1 to 100"),
            (serde_json::json!({"author": -1}), "positive integer"),
            (serde_json::json!({"after": "last week"}), "ISO8601"),
            (serde_json::json!({"after": "2026-11-01&x=1"}), "ISO8601"),
            (serde_json::json!({"orderby": "rand"}), "orderby must be"),
            (serde_json::json!({"orderby": "relevance"}), "needs search"),
            (serde_json::json!({"order": "up"}), "'asc' or 'desc'"),
            (serde_json::json!({"per_page": 101}), "1 to 100"),
            (serde_json::json!({"page": 0}), "positive integer"),
            (serde_json::json!({"search": ""}), "non-empty"),
            (
                serde_json::json!({"search": "x".repeat(201)}),
                "200 characters",
            ),
        ] {
            match post_list_query_arg(&list_args(bad.clone()), true) {
                Err(McpError::InvalidParams(msg)) => assert!(msg.contains(why), "{bad}: {msg}"),
                other => panic!("{bad}: {other:?}"),
            }
        }
        // 固定ページには、カテゴリー・タグが無い。menu_orderで並べられる
        assert!(post_list_query_arg(&list_args(serde_json::json!({"tags": [1]})), false).is_err());
        assert!(post_list_query_arg(
            &list_args(serde_json::json!({"orderby": "menu_order"})),
            false
        )
        .is_ok());
        assert!(post_list_query_arg(
            &list_args(serde_json::json!({"orderby": "menu_order"})),
            true
        )
        .is_err());
    }

    #[test]
    fn test_post_list_query_string_is_encoded() {
        let query = PostListQuery {
            status: Some("publish".to_string()),
            search: Some("Rust & MCP 日本語".to_string()),
            categories: vec![3, 4],
            after: Some("2026-11-01T00:00:00+09:00".to_string()),
            ..Default::default()
        };
        let encoded = query.query_string();
        assert_eq!(
            encoded,
            "status=publish&search=Rust+%26+MCP+%E6%97%A5%E6%9C%AC%E8%AA%9E&categories=3%2C4&after=2026-11-01T00%3A00%3A00%2B09%3A00"
        );
        assert_eq!(PostListQuery::default().query_string(), "");
    }

    #[test]
    fn test_list_result_text() {
        assert!(list_result_text("posts", &[], None).starts_with("Found 0 posts:"));
        let page = ListPage {
            page: 2,
            per_page: 5,
            total_pages: 3,
            total: 12,
        };
        assert!(list_result_text("posts", &[], Some(page))
            .starts_with("Found 0 posts (page 2 of 3, 5 per page; 12 in total):"));
    }

    /// 検索・絞り込みを渡すと、そのページだけを、符号化したURLで読む
    #[tokio::test]
    async fn test_get_posts_with_search_reads_one_page() {
        let list = format!("[{}]", post_json_with("publish", "2026-11-10T10:00:00"));
        // 状態行の後ろに、ページのヘッダーを足す
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/posts?per_page=5&page=2&search=hello+world&categories=3%2C4",
            "200 OK\r\nX-WP-Total: 6\r\nX-WP-TotalPages: 2",
            list,
        )])
        .await;
        let handler = handler_for_local_server(base);
        let result = handler
            .call_tool(ToolCallParams {
                name: "get_posts".to_string(),
                arguments: Some(list_args(serde_json::json!({
                    "search": "hello world",
                    "categories": [3, 4],
                    "per_page": 5,
                    "page": 2
                }))),
            })
            .await
            .unwrap();
        let text = tool_text(&result);
        assert!(
            text.starts_with("Found 1 posts (page 2 of 2, 5 per page; 6 in total):"),
            "{text}"
        );
    }

    /// 固定ページも同じ。ページの既定は20件
    #[tokio::test]
    async fn test_get_pages_with_search_uses_the_default_page_size() {
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/pages?per_page=20&page=1&search=about",
            "200 OK\r\nX-WP-Total: 0\r\nX-WP-TotalPages: 0",
            "[]".to_string(),
        )])
        .await;
        let handler = handler_for_local_server(base);
        let result = handler
            .call_tool(ToolCallParams {
                name: "get_pages".to_string(),
                arguments: Some(list_args(serde_json::json!({"search": "about"}))),
            })
            .await
            .unwrap();
        assert!(
            tool_text(&result).starts_with("Found 0 pages (page 1 of 1, 20 per page; 0 in total):")
        );
    }

    // ---- #639: コメントの管理 ----

    fn comment_json(id: u64, status: &str, parent: u64) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "post": 7,
            "parent": parent,
            "status": status,
            "date": "2026-11-19T10:00:00",
            "author_name": "読者",
            "author_email": "reader@example.com",
            "author_ip": "203.0.113.9",
            "link": "https://example.com/post/#comment-1",
            "content": {"rendered": "<p>ありがとうございます</p>"}
        })
    }

    async fn call_tool_with(
        handler: &WordPressHandler,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let args: HashMap<String, serde_json::Value> =
            serde_json::from_value(args).expect("arguments must be an object");
        handler
            .call_tool(ToolCallParams {
                name: name.to_string(),
                arguments: Some(args),
            })
            .await
    }

    /// `status`で絞る。メールアドレス・IPアドレスは出さない
    #[tokio::test]
    async fn test_get_comments_filters_by_status_without_personal_data() {
        let list = serde_json::json!([comment_json(1, "hold", 0)]).to_string();
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/comments?per_page=100&page=1&post=7&status=hold",
            "200 OK\r\nX-WP-Total: 1\r\nX-WP-TotalPages: 1",
            list,
        )])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(
            &handler,
            "get_comments",
            serde_json::json!({"post_id": 7, "status": "hold"}),
        )
        .await
        .unwrap();
        let text = tool_text(&result);
        assert!(text.starts_with("Found 1 comments:"), "{text}");
        assert!(text.contains("\"status\": \"hold\""), "{text}");
        assert!(!text.contains("reader@example.com"), "{text}");
        assert!(!text.contains("203.0.113.9"), "{text}");
    }

    #[tokio::test]
    async fn test_get_comments_refuses_unknown_status() {
        let handler = handler_for_local_server("http://127.0.0.1:9".to_string());
        let result = call_tool_with(
            &handler,
            "get_comments",
            serde_json::json!({"status": "approve&post=1"}),
        )
        .await;
        assert!(
            matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains("status must be one of")),
            "{:?}",
            result
        );
    }

    /// 返信は、既定で承認待ちで作る。投稿は、返信先のコメントから決める
    #[tokio::test]
    async fn test_create_comment_reply_is_held_by_default() {
        let (base_url, captured) =
            spawn_body_capturing_server(comment_json(5, "hold", 1).to_string()).await;
        let handler = handler_for_local_server(base_url);
        let result = call_tool_with(
            &handler,
            "create_comment",
            serde_json::json!({"parent": 1, "content": "ご質問ありがとうございます"}),
        )
        .await
        .unwrap();
        let bodies = captured.lock().await;
        // 返信先のコメントを読む（本文の無いGET）、作る
        assert_eq!(bodies.len(), 2, "{:?}", bodies);
        assert!(bodies[0].is_empty());
        let sent: serde_json::Value = serde_json::from_str(&bodies[1]).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({
                "post": 7,
                "parent": 1,
                "content": "ご質問ありがとうございます",
                "status": "hold"
            })
        );
        let text = tool_text(&result);
        assert!(
            text.contains("ID 5 on post 7, reply to 1 (Status: hold"),
            "{text}"
        );
        assert!(text.contains("held for moderation"), "{text}");
    }

    /// `status: approve`で、すぐに公開する。`post_id`があれば、返信先を読まない
    #[tokio::test]
    async fn test_create_comment_can_publish_at_once() {
        let (base_url, captured) =
            spawn_body_capturing_server(comment_json(6, "approved", 1).to_string()).await;
        let handler = handler_for_local_server(base_url);
        let result = call_tool_with(
            &handler,
            "create_comment",
            serde_json::json!({"post_id": 7, "parent": 1, "content": "返信", "status": "approve"}),
        )
        .await
        .unwrap();
        let bodies = captured.lock().await;
        assert_eq!(bodies.len(), 1);
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        assert_eq!(sent["status"], "approve");
        assert!(!tool_text(&result).contains("held for moderation"));
    }

    #[tokio::test]
    async fn test_create_comment_requires_content_and_post() {
        let handler = handler_for_local_server("http://127.0.0.1:9".to_string());
        for (args, message) in [
            (serde_json::json!({"post_id": 7}), "Missing content"),
            (
                serde_json::json!({"post_id": 7, "content": "  "}),
                "Missing content",
            ),
            (serde_json::json!({"content": "返信"}), "Missing post_id"),
            (
                serde_json::json!({"post_id": 7, "content": "返信", "status": "spam"}),
                "status must be one of: approve, hold",
            ),
            (
                serde_json::json!({"post_id": 0, "content": "返信"}),
                "post_id must be a positive integer",
            ),
        ] {
            let result = call_tool_with(&handler, "create_comment", args.clone()).await;
            assert!(
                matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains(message)),
                "{args}: {:?}",
                result
            );
        }
    }

    /// 状態を変える
    #[tokio::test]
    async fn test_update_comment_changes_status() {
        let (base_url, captured) =
            spawn_body_capturing_server(comment_json(1, "spam", 0).to_string()).await;
        let handler = handler_for_local_server(base_url);
        let result = call_tool_with(
            &handler,
            "update_comment",
            serde_json::json!({"comment_id": 1, "status": "spam"}),
        )
        .await
        .unwrap();
        let bodies = captured.lock().await;
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        assert_eq!(sent, serde_json::json!({"status": "spam"}));
        assert!(tool_text(&result).contains("(Status: spam"));
    }

    #[tokio::test]
    async fn test_update_comment_requires_a_change() {
        let handler = handler_for_local_server("http://127.0.0.1:9".to_string());
        for (args, message) in [
            (
                serde_json::json!({"comment_id": 1}),
                "Pass status or content",
            ),
            (
                serde_json::json!({"status": "approve"}),
                "Missing comment_id",
            ),
            (
                serde_json::json!({"comment_id": 1, "status": "all"}),
                "status must be one of",
            ),
        ] {
            let result = call_tool_with(&handler, "update_comment", args.clone()).await;
            assert!(
                matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains(message)),
                "{args}: {:?}",
                result
            );
        }
    }

    /// 既定はゴミ箱。`force`で完全に消す
    #[tokio::test]
    async fn test_delete_comment_moves_to_trash_by_default() {
        let (base_url, _) =
            spawn_body_capturing_server(comment_json(1, "trash", 0).to_string()).await;
        let handler = handler_for_local_server(base_url);
        let trashed = call_tool_with(
            &handler,
            "delete_comment",
            serde_json::json!({"comment_id": 1}),
        )
        .await
        .unwrap();
        assert_eq!(tool_text(&trashed), "Deleted comment ID 1 (moved to trash)");
        let deleted = call_tool_with(
            &handler,
            "delete_comment",
            serde_json::json!({"comment_id": 1, "force": true}),
        )
        .await
        .unwrap();
        assert_eq!(tool_text(&deleted), "Deleted comment ID 1 (permanently)");
    }

    /// 印: 読むだけ・作る・変える（#417と同じ規則）。変えるツールは監査ログに残る
    #[tokio::test]
    async fn test_comment_tools_annotations() {
        assert_eq!(
            wordpress_tool_annotations("get_comments").read_only_hint,
            Some(true)
        );
        let create = wordpress_tool_annotations("create_comment");
        assert_eq!(create.read_only_hint, Some(false));
        assert_eq!(create.destructive_hint, Some(false));
        for name in ["update_comment", "delete_comment"] {
            let a = wordpress_tool_annotations(name);
            assert_eq!(a.read_only_hint, Some(false), "{name}");
            assert_eq!(a.destructive_hint, Some(true), "{name}");
        }
        for name in ["create_comment", "update_comment", "delete_comment"] {
            assert!(is_mutating_tool(name), "{name}");
        }
        let handler = handler_for_local_server("http://127.0.0.1:9".to_string());
        let tools = handler.list_tools().await.unwrap();
        for name in ["create_comment", "update_comment", "delete_comment"] {
            assert!(tools.iter().any(|t| t.name == name), "{name}");
        }
    }

    // ---- #641: リビジョン ----

    /// リビジョンの応答（`context=edit`）。書き戻しの応答（投稿）としても読める
    /// ように、投稿の項目（`status`・`slug`・`type`）も持たせる
    fn revision_json(id: u64, parent: u64) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "parent": parent,
            "author": 2,
            "date": "2026-11-18T09:00:00",
            "title": {"raw": "前の題", "rendered": "前の題"},
            "content": {"raw": "<!-- wp:paragraph --><p>前の本文</p><!-- /wp:paragraph -->", "rendered": "<p>前の本文</p>"},
            "excerpt": {"raw": "前の抜粋", "rendered": "<p>前の抜粋</p>"},
            "status": "publish",
            "slug": "post",
            "type": "post"
        })
    }

    /// 一覧: 新しい順に、日時・書いた人の名前・題・長さを出す
    #[tokio::test]
    async fn test_get_revisions_lists_with_author_names() {
        let list = serde_json::json!([revision_json(12, 7), revision_json(11, 7)]).to_string();
        let base = spawn_routing_server(vec![
            (
                "/wp-json/wp/v2/posts/7/revisions?per_page=100&page=1",
                "200 OK\r\nX-WP-Total: 2\r\nX-WP-TotalPages: 1",
                list,
            ),
            (
                "/wp-json/wp/v2/users/2",
                "200 OK",
                r#"{"id":2,"name":"編集者"}"#.to_string(),
            ),
        ])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(&handler, "get_revisions", serde_json::json!({"post_id": 7}))
            .await
            .unwrap();
        let text = tool_text(&result);
        assert!(
            text.starts_with("Found 2 revisions of post 7 (newest first):\nID 12 (Date: 2026-11-18T09:00:00, Author: 編集者): 前の題 — 4 characters: 前の本文"),
            "{text}"
        );
    }

    /// 書いた人の名前は、重ならない人ごとに同時に聞き、全体の時間を過ぎたら
    /// `user {ID}`にする（#684）
    #[tokio::test]
    async fn test_user_names_are_fetched_concurrently_within_limit() {
        let (base, count) = spawn_silent_server().await;
        let handler = handler_for_local_server(base);
        let started = std::time::Instant::now();
        let names = handler
            .user_names([3, 2, 3, 5], Duration::from_millis(300))
            .await;
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(names.len(), 3);
        assert_eq!(names[&2], "user 2");
        assert_eq!(names[&5], "user 5");
        // 同時に聞いている（1人ずつなら、最初の人で時間切れになる）
        assert_eq!(request_count(&count), 3);
    }

    #[tokio::test]
    async fn test_get_revisions_of_a_page_without_revisions() {
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/pages/3/revisions?per_page=100&page=1",
            "200 OK\r\nX-WP-Total: 0\r\nX-WP-TotalPages: 0",
            "[]".to_string(),
        )])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(
            &handler,
            "get_revisions",
            serde_json::json!({"post_id": 3, "post_type": "page"}),
        )
        .await
        .unwrap();
        assert_eq!(tool_text(&result), "page 3 has no revisions.");
    }

    /// 1つのリビジョンを、既定では編集用の原文で読む
    #[tokio::test]
    async fn test_get_revision_reads_raw_by_default() {
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/posts/7/revisions/11?context=edit",
            "200 OK",
            revision_json(11, 7).to_string(),
        )])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(
            &handler,
            "get_revision",
            serde_json::json!({"post_id": 7, "revision_id": 11}),
        )
        .await
        .unwrap();
        let text = tool_text(&result);
        assert!(text.contains("Format: raw"), "{text}");
        assert!(text.contains("Excerpt: 前の抜粋"), "{text}");
        assert!(text.contains("<!-- wp:paragraph -->"), "{text}");

        let result = call_tool_with(
            &handler,
            "get_revision",
            serde_json::json!({"post_id": 7, "revision_id": 11, "format": "text"}),
        )
        .await
        .unwrap();
        assert!(tool_text(&result).ends_with("Content:\n前の本文"));
    }

    /// 書き戻し: リビジョンの原文の題・本文・抜粋だけを送る（状態等は送らない）
    #[tokio::test]
    async fn test_restore_revision_sends_title_content_and_excerpt() {
        let (base_url, captured) =
            spawn_body_capturing_server(revision_json(11, 7).to_string()).await;
        let handler = handler_for_local_server(base_url);
        let result = call_tool_with(
            &handler,
            "restore_revision",
            serde_json::json!({"post_id": 7, "revision_id": 11}),
        )
        .await
        .unwrap();
        let bodies = captured.lock().await;
        assert_eq!(bodies.len(), 2, "{:?}", bodies);
        assert!(bodies[0].is_empty());
        let sent: serde_json::Value = serde_json::from_str(&bodies[1]).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({
                "title": "前の題",
                "content": "<!-- wp:paragraph --><p>前の本文</p><!-- /wp:paragraph -->",
                "excerpt": "前の抜粋"
            })
        );
        let text = tool_text(&result);
        assert!(text.starts_with("Restored post 7 to revision 11"), "{text}");
        assert!(text.contains("Status: publish"), "{text}");
    }

    /// 別の投稿のリビジョンは、書き戻さない
    #[tokio::test]
    async fn test_restore_revision_refuses_another_posts_revision() {
        let (base_url, captured) =
            spawn_body_capturing_server(revision_json(11, 8).to_string()).await;
        let handler = handler_for_local_server(base_url);
        let result = call_tool_with(
            &handler,
            "restore_revision",
            serde_json::json!({"post_id": 7, "revision_id": 11}),
        )
        .await;
        assert!(
            matches!(result, Err(McpError::InvalidParams(ref msg)) if msg == "revision 11 belongs to post 8, not 7"),
            "{:?}",
            result
        );
        // 読んだだけで、書き戻しは送らない
        assert_eq!(captured.lock().await.len(), 1);
    }

    /// 原文が読めなければ（編集の権限が無い）、表示用HTMLで上書きしない
    #[tokio::test]
    async fn test_restore_revision_needs_raw_source() {
        let mut revision = revision_json(11, 7);
        revision["title"] = serde_json::json!({"rendered": "前の題"});
        revision["content"] = serde_json::json!({"rendered": "<p>前の本文</p>"});
        let (base_url, captured) = spawn_body_capturing_server(revision.to_string()).await;
        let handler = handler_for_local_server(base_url);
        let result = call_tool_with(
            &handler,
            "restore_revision",
            serde_json::json!({"post_id": 7, "revision_id": 11}),
        )
        .await;
        assert!(
            matches!(result, Err(McpError::ExternalApi(ref msg)) if msg.contains("needs credentials with edit permission")),
            "{:?}",
            result
        );
        assert_eq!(captured.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn test_revision_tools_check_arguments_and_marks() {
        let handler = handler_for_local_server("http://127.0.0.1:9".to_string());
        for (name, args, message) in [
            ("get_revisions", serde_json::json!({}), "Missing post_id"),
            (
                "get_revisions",
                serde_json::json!({"post_id": 7, "post_type": "attachment"}),
                "post_type 'attachment' is not allowed",
            ),
            (
                "get_revision",
                serde_json::json!({"post_id": 7}),
                "Missing revision_id",
            ),
            (
                "restore_revision",
                serde_json::json!({"post_id": 7, "revision_id": 0}),
                "revision_id must be a positive integer",
            ),
        ] {
            let result = call_tool_with(&handler, name, args.clone()).await;
            assert!(
                matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains(message)),
                "{name} {args}: {:?}",
                result
            );
        }
        for name in ["get_revisions", "get_revision"] {
            assert_eq!(wordpress_tool_annotations(name).read_only_hint, Some(true));
        }
        let restore = wordpress_tool_annotations("restore_revision");
        assert_eq!(restore.read_only_hint, Some(false));
        assert_eq!(restore.destructive_hint, Some(true));
        assert_eq!(restore.idempotent_hint, Some(true));
        assert!(is_mutating_tool("restore_revision"));
        // コメントのツールの監査ログに、コメントのIDを残す
        let args: HashMap<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({"comment_id": 5})).unwrap();
        assert_eq!(extract_target_id(&Some(args)), Some("5".to_string()));
    }

    // ---- #642: カスタム投稿タイプと独自のタクソノミー ----
    // handler_for_local_serverは、post_types = ["product"]

    fn product_type_json() -> String {
        r#"{"slug":"product","name":"Products","rest_base":"products","rest_namespace":"wp/v2","taxonomies":["product_cat"]}"#.to_string()
    }

    fn product_json() -> serde_json::Value {
        serde_json::json!({
            "id": 21,
            "slug": "mug",
            "status": "publish",
            "type": "product",
            "title": {"rendered": "マグカップ"},
            "content": {"rendered": "<p>説明</p>"},
            "product_cat": [12, 15],
            // 投稿タイプの応答としても読めるように
            "rest_base": "products",
            "rest_namespace": "wp/v2"
        })
    }

    /// 設定で許したカスタム投稿タイプは、サイトの`rest_base`の場所を使う
    #[tokio::test]
    async fn test_custom_post_type_uses_rest_base() {
        let list = serde_json::json!([product_json()]).to_string();
        let base = spawn_routing_server(vec![
            (
                "/wp-json/wp/v2/types/product",
                "200 OK",
                product_type_json(),
            ),
            (
                "/wp-json/wp/v2/products?per_page=100&page=1",
                "200 OK\r\nX-WP-Total: 1\r\nX-WP-TotalPages: 1",
                list,
            ),
        ])
        .await;
        let handler = handler_for_local_server(base);
        assert_eq!(handler.post_type_endpoint("post").await.unwrap(), "posts");
        assert_eq!(handler.post_type_endpoint("page").await.unwrap(), "pages");
        let result = call_tool_with(
            &handler,
            "get_posts",
            serde_json::json!({"post_type": "product"}),
        )
        .await
        .unwrap();
        let text = tool_text(&result);
        assert!(text.starts_with("Found 1 products:"), "{text}");
        assert!(text.contains("マグカップ"), "{text}");
        // 2回目は、サイトに聞かない
        assert!(handler.type_endpoints.lock().await.contains_key("product"));
    }

    /// 設定に無い型・名前の形でない型は、サイトに送らない
    #[tokio::test]
    async fn test_custom_post_type_must_be_allowed() {
        let handler = handler_for_local_server("http://127.0.0.1:9".to_string());
        for (post_type, message) in [
            ("event", "post_type 'event' is not allowed: mcp-rs uses only post, page, product. To use it, add it to post_types"),
            ("../users", "is not a post type name"),
        ] {
            let result = call_tool_with(
                &handler,
                "get_post",
                serde_json::json!({"post_id": 1, "post_type": post_type}),
            )
            .await;
            assert!(
                matches!(result, Err(McpError::InvalidParams(ref msg)) if msg.contains(message)),
                "{post_type}: {:?}",
                result
            );
        }
    }

    /// `wp/v2`の外（WooCommerceの`wc/v3`等）にある型は使えない
    #[tokio::test]
    async fn test_custom_post_type_outside_wp_v2_is_refused() {
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/types/product",
            "200 OK",
            r#"{"slug":"product","rest_base":"products","rest_namespace":"wc/v3"}"#.to_string(),
        )])
        .await;
        let handler = handler_for_local_server(base);
        let err = handler.post_type_endpoint("product").await.unwrap_err();
        assert!(err.to_string().contains("namespace 'wc/v3'"), "{err}");
    }

    /// `terms`で、独自のタクソノミーの項目を付け替える
    #[tokio::test]
    async fn test_update_post_sends_terms() {
        let (base_url, captured) = spawn_routing_capturing_server(product_routes()).await;
        let handler = handler_for_local_server(base_url);
        call_tool_with(
            &handler,
            "update_post",
            serde_json::json!({
                "post_id": 21,
                "post_type": "product",
                "terms": {"product_cat": [12, 15], "product_tag": []}
            }),
        )
        .await
        .unwrap();
        let requests = captured.lock().await;
        let (request, body) = requests.last().unwrap();
        assert_eq!(request, "PUT /wp-json/wp/v2/products/21");
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"product_cat": [12, 15], "product_tag": []})
        );
    }

    /// `product`の型・タクソノミー・投稿の応答（#684）
    fn product_routes() -> Vec<(&'static str, String)> {
        vec![
            ("/wp-json/wp/v2/types/product", product_type_json()),
            (
                "/wp-json/wp/v2/taxonomies?type=product",
                r#"{"product_cat":{"rest_base":"product_cat"},"product_tag":{"rest_base":"product_tag"}}"#
                    .to_string(),
            ),
            ("/wp-json/wp/v2/products", product_json().to_string()),
        ]
    }

    /// 投稿タイプに登録されていないタクソノミーの`terms`は、送らずに断る
    /// （WordPressは黙って無視するため。#684）
    #[tokio::test]
    async fn test_terms_of_unregistered_taxonomy_are_refused() {
        let (base_url, captured) = spawn_routing_capturing_server(product_routes()).await;
        let handler = handler_for_local_server(base_url);
        for (tool, args) in [
            (
                "update_post",
                serde_json::json!({
                    "post_id": 21,
                    "post_type": "product",
                    "terms": {"product_cat": [12], "genre": [3]}
                }),
            ),
            (
                "create_advanced_post",
                serde_json::json!({
                    "title": "マグカップ",
                    "content": "説明",
                    "post_type": "product",
                    "terms": {"genre": [3]}
                }),
            ),
        ] {
            let result = call_tool_with(&handler, tool, args).await;
            assert!(
                matches!(result, Err(McpError::InvalidParams(ref msg))
                    if msg.contains("'genre' is not a taxonomy of post type 'product'")
                        && msg.contains("Valid: product_cat, product_tag")),
                "{tool}: {:?}",
                result
            );
        }
        // 投稿は書き換えていない
        let requests = captured.lock().await;
        assert!(
            requests
                .iter()
                .all(|(request, _)| request.starts_with("GET ")),
            "{requests:?}"
        );
    }

    /// `terms`が無ければ、タクソノミーを調べない（#684）
    #[tokio::test]
    async fn test_update_post_without_terms_skips_taxonomy_lookup() {
        let (base_url, captured) = spawn_routing_capturing_server(product_routes()).await;
        let handler = handler_for_local_server(base_url);
        call_tool_with(
            &handler,
            "update_post",
            serde_json::json!({"post_id": 21, "post_type": "product", "title": "x"}),
        )
        .await
        .unwrap();
        let requests = captured.lock().await;
        assert!(
            !requests
                .iter()
                .any(|(request, _)| request.contains("/taxonomies")),
            "{requests:?}"
        );
    }

    /// 本文のない投稿タイプ（`editor`に対応していない）の投稿も読め、
    /// 空の本文としては見せない（#684）
    #[tokio::test]
    async fn test_get_post_of_type_without_content() {
        let mut product = product_json();
        let object = product.as_object_mut().unwrap();
        object.remove("content");
        object.remove("title");
        let post: WordPressPost = serde_json::from_value(product.clone()).unwrap();
        assert!(post.content.is_none());
        assert_eq!(post.title.rendered, "");
        // 本文のないまま書き出しても、`content`を含めない
        assert!(serde_json::to_value(&post)
            .unwrap()
            .get("content")
            .is_none());

        let base = spawn_routing_server(vec![
            (
                "/wp-json/wp/v2/types/product",
                "200 OK",
                product_type_json(),
            ),
            ("/wp-json/wp/v2/products/21", "200 OK", product.to_string()),
            (
                "/wp-json/wp/v2/taxonomies?type=product",
                "200 OK",
                r#"{"product_cat":{"rest_base":"product_cat"}}"#.to_string(),
            ),
            (
                "/wp-json/wp/v2/categories",
                "200 OK\r\nX-WP-TotalPages: 1",
                "[]".to_string(),
            ),
            (
                "/wp-json/wp/v2/tags",
                "200 OK\r\nX-WP-TotalPages: 1",
                "[]".to_string(),
            ),
        ])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(
            &handler,
            "get_post",
            serde_json::json!({"post_id": 21, "post_type": "product"}),
        )
        .await
        .unwrap();
        let text = tool_text(&result);
        assert!(
            text.contains("Note: This post type has no content field"),
            "{text}"
        );
        assert!(text.ends_with("Content:\n(none)"), "{text}");
    }

    #[test]
    fn test_terms_arg_rejects_other_fields_and_bad_ids() {
        let args = |value: serde_json::Value| -> HashMap<String, serde_json::Value> {
            serde_json::from_value(serde_json::json!({ "terms": value })).unwrap()
        };
        assert_eq!(terms_arg(&HashMap::new()).unwrap(), None);
        for (value, message) in [
            (serde_json::json!([1]), "terms must be an object"),
            (
                serde_json::json!({"status": [1]}),
                "'status' is not a taxonomy",
            ),
            (serde_json::json!({"Product Cat": [1]}), "is not a taxonomy"),
            (
                serde_json::json!({"product_cat": 1}),
                "terms.product_cat must be a list of term IDs",
            ),
            (
                serde_json::json!({"product_cat": [0]}),
                "terms.product_cat must be a list of term IDs",
            ),
        ] {
            let err = terms_arg(&args(value.clone())).unwrap_err();
            assert!(err.to_string().contains(message), "{value}: {err}");
        }
    }

    /// 型の一覧は、使えるか（設定で許したか）も出す
    #[tokio::test]
    async fn test_get_post_types_shows_whether_usable() {
        let base = spawn_routing_server(vec![(
            "/wp-json/wp/v2/types",
            "200 OK",
            r#"{"post":{"name":"Posts","taxonomies":["category","post_tag"]},"product":{"name":"Products","taxonomies":["product_cat"]},"event":{"name":"Events","taxonomies":[]}}"#.to_string(),
        )])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(&handler, "get_post_types", serde_json::json!({}))
            .await
            .unwrap();
        let text = tool_text(&result);
        assert!(
            text.contains("post (Posts): usable; taxonomies: category, post_tag"),
            "{text}"
        );
        assert!(
            text.contains("product (Products): usable; taxonomies: product_cat"),
            "{text}"
        );
        assert!(
            text.contains("event (Events): not allowed (add it to post_types"),
            "{text}"
        );
    }

    /// 項目の一覧は、タクソノミーの名前から`rest_base`を決めて読む
    #[tokio::test]
    async fn test_get_terms_resolves_rest_base() {
        let base = spawn_routing_server(vec![
            (
                "/wp-json/wp/v2/taxonomies",
                "200 OK",
                r#"{"product_cat":{"name":"Product categories","rest_base":"product_cat","rest_namespace":"wp/v2"}}"#.to_string(),
            ),
            (
                "/wp-json/wp/v2/product_cat?per_page=100&page=1",
                "200 OK\r\nX-WP-Total: 1\r\nX-WP-TotalPages: 1",
                r#"[{"id":12,"name":"食器","slug":"dishes","parent":0,"count":3}]"#.to_string(),
            ),
        ])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(
            &handler,
            "get_terms",
            serde_json::json!({"taxonomy": "product_cat"}),
        )
        .await
        .unwrap();
        assert_eq!(
            tool_text(&result),
            "Found 1 terms in product_cat:\nID 12: 食器 (slug: dishes, parent: 0, posts: 3)"
        );
        let missing = call_tool_with(
            &handler,
            "get_terms",
            serde_json::json!({"taxonomy": "genre"}),
        )
        .await;
        assert!(
            matches!(missing, Err(McpError::InvalidParams(ref msg)) if msg.contains("taxonomy 'genre' was not found")),
            "{:?}",
            missing
        );
    }

    /// カスタム投稿タイプの`get_post`は、独自のタクソノミーの項目も出す
    #[tokio::test]
    async fn test_get_post_shows_custom_terms() {
        let base = spawn_routing_server(vec![
            (
                "/wp-json/wp/v2/types/product",
                "200 OK",
                product_type_json(),
            ),
            (
                "/wp-json/wp/v2/products/21",
                "200 OK",
                product_json().to_string(),
            ),
            (
                "/wp-json/wp/v2/taxonomies?type=product",
                "200 OK",
                r#"{"product_cat":{"rest_base":"product_cat"}}"#.to_string(),
            ),
            (
                "/wp-json/wp/v2/categories",
                "200 OK\r\nX-WP-TotalPages: 1",
                "[]".to_string(),
            ),
            (
                "/wp-json/wp/v2/tags",
                "200 OK\r\nX-WP-TotalPages: 1",
                "[]".to_string(),
            ),
        ])
        .await;
        let handler = handler_for_local_server(base);
        let result = call_tool_with(
            &handler,
            "get_post",
            serde_json::json!({"post_id": 21, "post_type": "product"}),
        )
        .await
        .unwrap();
        let text = tool_text(&result);
        assert!(text.contains("Terms: product_cat: 12, 15\n"), "{text}");
    }

    /// ツールの`post_type`には、許した型だけを見せる
    #[tokio::test]
    async fn test_tools_list_only_allowed_post_types() {
        let handler = handler_for_local_server("http://127.0.0.1:9".to_string());
        let tools = handler.list_tools().await.unwrap();
        let enum_of = |name: &str| {
            tools.iter().find(|t| t.name == name).unwrap().input_schema["properties"]["post_type"]
                ["enum"]
                .clone()
        };
        assert_eq!(
            enum_of("get_post"),
            serde_json::json!(["post", "page", "product"])
        );
        assert_eq!(
            enum_of("update_post"),
            serde_json::json!(["post", "page", "product"])
        );
        assert_eq!(enum_of("get_posts"), serde_json::json!(["post", "product"]));

        // 許していなければ、今までと同じ（get_postsにpost_typeは無い）
        let plain = rate_limited_handler(RateLimitConfig::default());
        let tools = plain.list_tools().await.unwrap();
        let get_posts = tools.iter().find(|t| t.name == "get_posts").unwrap();
        assert!(get_posts.input_schema["properties"]
            .get("post_type")
            .is_none());
        let get_post = tools.iter().find(|t| t.name == "get_post").unwrap();
        assert_eq!(
            get_post.input_schema["properties"]["post_type"]["enum"],
            serde_json::json!(["post", "page"])
        );
    }

    #[test]
    fn test_try_new_rejects_bad_post_types() {
        for post_type in ["Product", "page", "a/b"] {
            let config: WordPressConfig = toml::from_str(&format!(
                "url = \"https://example.com\"\nenabled = true\nusername = \"u\"\npassword = \"p\"\npost_types = [\"{post_type}\"]\n"
            ))
            .unwrap();
            let err = WordPressHandler::try_new(config).unwrap_err();
            assert!(
                err.contains("is not a custom post type name"),
                "{post_type}: {err}"
            );
        }
    }

    // ---- #644: リソースのテンプレート ----

    #[test]
    fn test_resource_template_target_parses_ids() {
        assert_eq!(
            resource_template_target("wordpress://post/{id}"),
            Some((ResourceKind::Post, ResourceId::Placeholder))
        );
        assert_eq!(
            resource_template_target("wordpress://page/42"),
            Some((ResourceKind::Page, ResourceId::Id(42)))
        );
        assert_eq!(
            resource_template_target("wordpress://media/7"),
            Some((ResourceKind::Media, ResourceId::Id(7)))
        );
        for uri in [
            "wordpress://post/0",
            "wordpress://post/012",
            "wordpress://post/-1",
            "wordpress://post/ 1",
            "wordpress://post/1a",
            "wordpress://post/",
            "wordpress://post/1/x",
            "wordpress://posts/1",
            "wordpress://user/1",
            "other://post/1",
            "wordpress://post/99999999999999999999999",
        ] {
            assert_eq!(resource_template_target(uri), None, "{uri}");
        }
    }

    /// テンプレートは、どれも読み取りのツールに対応する（#538と同じ規則）
    #[tokio::test]
    async fn test_every_resource_template_maps_to_a_read_only_tool() {
        let handler = rate_limited_handler(RateLimitConfig::default());
        let tools = handler.list_tools().await.unwrap();
        let templates = handler.list_resource_templates().await.unwrap();
        assert_eq!(templates.len(), 3);
        for template in templates {
            let name = handler.resource_tool(&template.uri_template).unwrap();
            let tool = tools.iter().find(|t| t.name == name).unwrap();
            assert_eq!(
                tool.annotations.as_ref().and_then(|a| a.read_only_hint),
                Some(true),
                "{}",
                template.uri_template
            );
            let filled = template.uri_template.replace("{id}", "5");
            assert_eq!(handler.resource_tool(&filled), Some(name));
        }
    }

    /// 1件を読む。WordPressが404なら、無いリソース（-32002）
    #[tokio::test]
    async fn test_read_post_by_template_and_not_found() {
        let base = spawn_routing_server(vec![
            (
                "/wp-json/wp/v2/pages/7",
                "200 OK",
                post_json_with("publish", "2026-11-10T10:00:00"),
            ),
            (
                "/wp-json/wp/v2/posts/9",
                "404 Not Found",
                r#"{"code":"rest_post_invalid_id","message":"Invalid post ID."}"#.to_string(),
            ),
        ])
        .await;
        let handler = handler_for_local_server(base);
        let page = handler
            .read_resource(ResourceReadParams {
                uri: "wordpress://page/7".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(page["contents"][0]["uri"], "wordpress://page/7");
        let text = page["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"id\": 7"), "{text}");

        let missing = handler
            .read_resource(ResourceReadParams {
                uri: "wordpress://post/9".to_string(),
            })
            .await;
        assert!(
            matches!(missing, Err(McpError::ResourceNotFound(ref uri)) if uri == "wordpress://post/9"),
            "{:?}",
            missing
        );
    }
}
