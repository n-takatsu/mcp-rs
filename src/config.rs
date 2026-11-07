//! Configuration Management for MCP-RS
//!
//! This module provides configuration structures and utilities for managing
//! MCP (Model Context Protocol) server settings
//! and dynamic configuration management.
//!
//! # Examples
//!
//! ## Basic Configuration Usage
//!
//! ```rust
//! use mcp_rs::config::McpConfig;
//!
//! // Use default configuration
//! let config = McpConfig::default();
//! println!("Server bind address: {:?}", config.server.bind_addr);
//! println!("Log level: {:?}", config.server.log_level);
//! ```

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

pub mod dynamic;
pub use dynamic::DynamicConfigManager;

/// `WORDPRESS_MASTER_PASSWORD`環境変数を読み書きするテストを直列化する
/// ためのロック。`cargo test`は同一プロセス内で全テストを並列実行する
/// ため、環境変数はプロセス全体で共有される。このロックを取らずに複数
/// のテストが同じ環境変数を同時にset/removeすると、無関係なテストが
/// 「環境変数が見つからない」という形で不安定に失敗する
/// （`src/config.rs`・`src/config/dynamic.rs`両方のテストで使用）。
#[cfg(test)]
pub(crate) static WORDPRESS_MASTER_PASSWORD_ENV_LOCK: std::sync::Mutex<()> =
    std::sync::Mutex::new(());

// 前方宣言用の型エイリアス
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

// セキュリティ機能用のimport
use crate::security::{EncryptedCredentials, EncryptionError, SecureCredentials};
use secrecy::ExposeSecret;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    pub server: ServerConfig,
    pub transport: TransportConfig,
    pub handlers: HandlersConfig,
    /// プラグイン（#394）。`[[plugins]]`で1つずつ指定する。既定ではなし。
    /// v0.18.1以前の`[plugins]`（テーブル）は読み込まずに無視する
    #[serde(
        default,
        deserialize_with = "deserialize_plugins",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub plugins: Vec<PluginConfig>,
}

/// プラグイン1つ分の設定（#394）。プラグインは、別プロセスで動くMCP
/// サーバー（標準入出力で通信する）として起動する。
#[derive(Clone, Deserialize, Serialize)]
pub struct PluginConfig {
    /// プラグイン名。ツール名の前に`<name>__`として付ける。英小文字・
    /// 数字・`_`・`-`のみ
    pub name: String,
    /// 起動するコマンド（実行ファイルのパス）。`container`を指定した
    /// 場合は、コンテナの中で実行するコマンド（省略時はイメージのCMD）
    #[serde(default)]
    pub command: String,
    /// `command`に渡す引数。`container`を指定して`command`を省略した
    /// 場合は、イメージのENTRYPOINTに渡る
    #[serde(default)]
    pub args: Vec<String>,
    /// プラグインに渡す環境変数。本体の環境変数は引き継がず、ここに
    /// 書いたものだけを渡す。値の`${VAR}`は本体の環境変数で展開する
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// 子プロセスの作業フォルダ。省略時は本体と同じ。`container`を
    /// 指定した場合は、コンテナ内の作業フォルダ
    pub working_dir: Option<String>,
    /// 指定すると、プラグインをDockerコンテナの中で起動する（#411）。
    /// 省略時は子プロセスとして起動する
    pub container: Option<PluginContainerConfig>,
    /// `false`で起動しない。省略時は`true`
    pub enabled: Option<bool>,
    /// 起動（`initialize`・`tools/list`）とツール呼び出しの待ち時間の上限
    /// （秒）。省略時は30秒
    pub call_timeout_seconds: Option<u64>,
    /// プラグインから受け取る1メッセージの大きさの上限（バイト）。
    /// 省略時は1MB
    pub max_response_bytes: Option<usize>,
    /// ツール呼び出しのレート制限。省略時は既定値（WordPressと同じ）
    pub rate_limit: Option<RateLimitConfig>,
}

impl fmt::Debug for PluginConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // envにはAPIキー等が入りうるため、値は出さずに名前だけ示す
        let mut env_keys: Vec<&String> = self.env.keys().collect();
        env_keys.sort();
        f.debug_struct("PluginConfig")
            .field("name", &self.name)
            .field("command", &self.command)
            .field("args", &self.args)
            .field("env_keys", &env_keys)
            .field("working_dir", &self.working_dir)
            .field("container", &self.container)
            .field("enabled", &self.enabled)
            .field("call_timeout_seconds", &self.call_timeout_seconds)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("rate_limit", &self.rate_limit)
            .finish()
    }
}

/// プラグインをDockerコンテナで起動する設定（#411、`[plugins.container]`）。
/// 既定で、ネットワークなし・全ケーパビリティ削除・ルートファイル
/// システム読み取り専用・メモリとプロセス数の上限付きで起動する。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginContainerConfig {
    /// イメージ。起動時には取得しない（`--pull=never`）ため、事前に
    /// `docker pull`しておく
    pub image: String,
    /// コンテナのネットワーク。省略時は`none`（外部と通信できない）
    pub network: Option<String>,
    /// コンテナに見せるホストのフォルダ。ここに書いたものだけをマウントする
    #[serde(default)]
    pub mounts: Vec<PluginMountConfig>,
    /// メモリの上限（例: `"512m"`）。省略時は`512m`
    pub memory: Option<String>,
    /// CPUの上限（例: `"1.0"`）。省略時は上限なし
    pub cpus: Option<String>,
    /// プロセス数の上限。省略時は256
    pub pids_limit: Option<u32>,
    /// `false`で、コンテナのルートファイルシステムに書き込めるように
    /// する。省略時は`true`（読み取り専用。`/tmp`だけ書き込める）
    pub read_only: Option<bool>,
    /// コンテナ内で実行するユーザー（例: `"1000:1000"`）。省略時は
    /// イメージの既定
    pub user: Option<String>,
    /// 使うコマンド。省略時は`docker`（`podman`等も指定できる）
    pub docker_command: Option<String>,
}

/// コンテナにマウントするホストのフォルダ
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginMountConfig {
    /// ホストのフォルダ（絶対パス）
    pub source: String,
    /// コンテナ内のパス（`/`で始まる）
    pub target: String,
    /// `false`で書き込めるようにする。省略時は`true`（読み取り専用）
    pub read_only: Option<bool>,
}

/// 設定ファイルに、v0.18.1以前の`[plugins]`（テーブル）と`[[plugins]]`が
/// 両方ある場合のヒント。両方あるとTOMLとして読めず（同じキーを2通りに
/// 定義することになる）、serdeの段階で読み飛ばすこともできないため、
/// 読み込みエラーのメッセージに添える。
pub fn legacy_plugins_conflict_hint(content: &str) -> Option<&'static str> {
    let headers: Vec<String> = content
        .lines()
        .map(|line| {
            line.split('#')
                .next()
                .unwrap_or("")
                .replace([' ', '\t'], "")
        })
        .collect();
    let has_new = headers.iter().any(|h| h == "[[plugins]]");
    // `[plugins.container]`は、直前の`[[plugins]]`のコンテナ設定（#411）
    let has_legacy = headers.iter().any(|h| {
        h == "[plugins]"
            || (h.starts_with("[plugins.")
                && h.ends_with(']')
                && !h.starts_with("[plugins.container"))
    });
    (has_new && has_legacy).then_some(
        "The config file has both the old [plugins] section (v0.18.1 and earlier, never used by mcp-rs) and [[plugins]] entries. Delete the old [plugins] section, including [plugins.*] subsections",
    )
}

/// 読み込みエラーに、上記のヒントを添える
pub fn with_config_hint(error: impl fmt::Display, content: &str) -> String {
    match legacy_plugins_conflict_hint(content) {
        Some(hint) => format!("{}\n{}", error, hint),
        None => error.to_string(),
    }
}

/// `plugins`を読み込む。`[[plugins]]`（配列）だけを受け付ける。
///
/// v0.18.1以前の`[plugins]`（テーブル、`PluginsConfig`）は本体が一度も
/// 使っておらず、v0.18.2で「残っていても無視される」とした。その約束を
/// 守るため、テーブルは警告を出して読み飛ばす。
fn deserialize_plugins<'de, D>(deserializer: D) -> Result<Vec<PluginConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(_) => serde_json::from_value(value)
            .map_err(|e| serde::de::Error::custom(format!("invalid [[plugins]] entry: {}", e))),
        serde_json::Value::Object(_) => {
            // ログはまだ初期化されていないことがあるため、標準エラー出力に書く
            eprintln!(
                "[mcp-rs] warning: the [plugins] table from v0.18.1 and earlier is ignored. To add a plugin, use [[plugins]] entries"
            );
            Ok(Vec::new())
        }
        _ => Err(serde::de::Error::custom(
            "plugins must be [[plugins]] entries",
        )),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub bind_addr: Option<String>,
    pub stdio: Option<bool>,
    pub log_level: Option<String>,
    /// ログ保持ポリシー設定
    pub log_retention: Option<LogRetentionConfig>,
    /// ログモジュール分離設定
    pub log_module: Option<LogModuleConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogRetentionConfig {
    /// 保持ポリシータイプ: "external", "days", "count", "size"
    pub policy: Option<String>,
    /// 日数（days用）
    pub days: Option<u32>,
    /// ファイル数（count用）
    pub count: Option<u32>,
    /// サイズ（MB単位、size用）
    pub size_mb: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LogModuleConfig {
    /// モジュール分離タイプ: "single", "separated", "hybrid"
    pub separation: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransportConfig {
    /// Transport type: "stdio" or "http". WebSocket is not a separate
    /// transport type - set `transport_type = "http"` and
    /// `transport.http.enable_websocket_upgrade = true` to expose `/ws` on
    /// the HTTP(S) listener instead.
    /// TOMLでは `type = "stdio"` 表記も受け付ける（configs/production/*.toml 等で使用）。
    #[serde(alias = "type")]
    pub transport_type: Option<String>,
    /// Stdio transport configuration
    pub stdio: Option<StdioTransportConfig>,
    /// HTTP transport configuration
    pub http: Option<HttpTransportConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StdioTransportConfig {
    pub buffer_size: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub content_length_header: Option<bool>,
    pub framing_method: Option<String>, // "content-length" | "line-based"
    pub max_message_size: Option<usize>,
    pub pretty_print: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpTransportConfig {
    pub addr: Option<String>,
    pub port: Option<u16>,
    pub enable_cors: Option<bool>,
    pub tls_enabled: Option<bool>,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub mtls_enabled: Option<bool>,
    pub mtls_ca_cert_path: Option<String>,
    pub enforce_https: Option<bool>,
    pub min_tls_version: Option<String>,
    pub hsts_enabled: Option<bool>,
    pub hsts_max_age_seconds: Option<u64>,
    pub hsts_include_subdomains: Option<bool>,
    pub hsts_preload: Option<bool>,
    pub certificate_pinning_enabled: Option<bool>,
    pub pinned_certificates_sha256: Option<Vec<String>>,
    pub certificate_pin_header: Option<String>,
    /// Enforce nonce/timestamp replay protection (requires clients to send
    /// `X-Nonce`/`X-Timestamp` headers). Defaults to `false`.
    pub anti_replay_enabled: Option<bool>,
    /// Run incoming requests through the intrusion detection/prevention
    /// system and automatically block a source IP that triggers it.
    /// Defaults to `false`.
    pub ids_enabled: Option<bool>,
    /// When IDS/IPS is enabled, trust `X-Forwarded-For`/`X-Real-IP` for the
    /// client IP instead of the raw TCP peer address. Only safe when this
    /// transport sits directly behind a trusted reverse proxy. Defaults to
    /// `false`.
    pub ids_trust_forwarded_for: Option<bool>,
    /// Reject binding/connections from non-loopback addresses. Defaults to
    /// `true` (see `configs/security/network-policy.toml`).
    pub network_policy_reject_external_connections: Option<bool>,
    /// Log a warning when binding to a non-loopback address that is still
    /// permitted. Defaults to `true`.
    pub network_policy_warn_on_external_bind: Option<bool>,
    /// IP addresses or CIDR ranges (e.g. `"192.168.1.0/24"`) exempted from
    /// `reject_external_connections`. Defaults to empty.
    pub network_policy_ip_whitelist: Option<Vec<String>>,
    /// Mount a `/ws` WebSocket upgrade endpoint on this HTTP listener,
    /// sharing its TLS/HSTS/certificate-pinning settings. Defaults to
    /// `false`.
    pub enable_websocket_upgrade: Option<bool>,
    /// Maximum concurrent `/ws` connections before new upgrades are
    /// rejected. Defaults to `1000`.
    pub websocket_max_connections: Option<usize>,
    /// Require a valid API key on every HTTP/WebSocket request. STDIO
    /// transport is unaffected. Defaults to `false`, but strongly
    /// recommended whenever `addr` is not loopback-only.
    pub auth_enabled: Option<bool>,
    /// Valid API keys when `auth_enabled = true`.
    pub api_keys: Option<Vec<ApiKeyEntryConfig>>,
}

/// TOML-facing shape for a single API key entry under
/// `transport.http.api_keys`. Mirrors `transport::http::ApiKeyEntry`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiKeyEntryConfig {
    /// The raw key value clients must present.
    pub key: String,
    /// Human-readable label for this key.
    pub name: String,
    /// Identifier for whoever this key was issued to.
    pub user_id: String,
}

impl From<ApiKeyEntryConfig> for crate::transport::http::ApiKeyEntry {
    fn from(entry: ApiKeyEntryConfig) -> Self {
        Self {
            key: entry.key,
            name: entry.name,
            user_id: entry.user_id,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HandlersConfig {
    pub wordpress: Option<WordPressConfig>,
    // 将来の拡張用
    // pub github: Option<GitHubConfig>,
    // pub custom: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct WordPressConfig {
    pub url: String,
    /// `encrypted_credentials`のみを指定する設定も許容するため、省略可能
    /// （省略時は空文字列。`resolve_encrypted_credentials`が復号結果で
    /// 上書きする想定）
    #[serde(default)]
    pub username: String,
    /// Application Password (平文 - 後方互換性のため)。`username`と同様に
    /// `encrypted_credentials`のみの設定を許容するため省略可能
    #[serde(default)]
    pub password: String,
    pub enabled: Option<bool>,
    pub timeout_seconds: Option<u64>,
    /// レート制限設定
    pub rate_limit: Option<RateLimitConfig>,
    /// 暗号化された認証情報（オプション）
    pub encrypted_credentials: Option<EncryptedCredentials>,
    /// `upload_media`の`file_path`で読み込みを許可するフォルダ。空（既定）
    /// なら`file_path`は使えない。mcp-rsが動いているマシン上のファイルを
    /// 読んでWordPressへ送る機能のため、明示的に許可したフォルダの中だけに
    /// 限定する（起動時に存在しないフォルダが指定されていればエラー）。
    #[serde(default)]
    pub upload_allowed_dirs: Vec<String>,
}

// Debugトレイトでパスワードを露出しないようにカスタム実装
// （SecureCredentials::fmt::Debugと同じ方針）
impl fmt::Debug for WordPressConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WordPressConfig")
            .field("url", &self.url)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("enabled", &self.enabled)
            .field("timeout_seconds", &self.timeout_seconds)
            .field("rate_limit", &self.rate_limit)
            .field("encrypted_credentials", &self.encrypted_credentials)
            .field("upload_allowed_dirs", &self.upload_allowed_dirs)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    /// 最大リクエスト数/秒
    pub requests_per_second: u32,
    /// バーストリクエスト許可数
    pub burst_size: u32,
    /// レート制限有効化フラグ
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10, // 10 requests/sec
            burst_size: 20,          // 20 burst requests
            enabled: true,
        }
    }
}

impl WordPressConfig {
    /// 平文パスワードからセキュア認証情報を作成
    pub fn create_secure_credentials(&self) -> SecureCredentials {
        SecureCredentials::new(self.username.clone(), self.password.clone())
    }

    /// 暗号化された認証情報から新しいWordPressConfigを作成
    #[allow(dead_code)]
    pub fn from_encrypted(
        url: String,
        encrypted_credentials: EncryptedCredentials,
        master_password: &str,
        enabled: Option<bool>,
        timeout_seconds: Option<u64>,
        rate_limit: Option<RateLimitConfig>,
    ) -> Result<Self, EncryptionError> {
        // 復号化して平文認証情報を取得（後方互換性のため）
        let secure_creds =
            SecureCredentials::from_encrypted(&encrypted_credentials, master_password)?;

        Ok(Self {
            url,
            username: secure_creds.username.clone(),
            password: secure_creds.get_password().expose_secret().to_string(),
            enabled,
            timeout_seconds,
            rate_limit,
            encrypted_credentials: Some(encrypted_credentials),
            upload_allowed_dirs: Vec::new(),
        })
    }

    /// 認証情報を暗号化して保存
    #[allow(dead_code)]
    pub fn encrypt_credentials(&mut self, master_password: &str) -> Result<(), EncryptionError> {
        let secure_creds = self.create_secure_credentials();
        self.encrypted_credentials = Some(secure_creds.encrypt(master_password)?);
        Ok(())
    }

    /// セキュア認証情報を取得（暗号化されている場合は復号化）
    #[allow(dead_code)]
    pub fn get_secure_credentials(
        &self,
        master_password: Option<&str>,
    ) -> Result<SecureCredentials, EncryptionError> {
        if let Some(encrypted) = &self.encrypted_credentials {
            let master_pw = master_password.ok_or_else(|| {
                EncryptionError::InvalidInput(
                    "暗号化されたデータにはマスターパスワードが必要です".to_string(),
                )
            })?;
            SecureCredentials::from_encrypted(encrypted, master_pw)
        } else {
            // 平文データからセキュア認証情報を作成
            Ok(self.create_secure_credentials())
        }
    }

    /// 暗号化されているかどうかを確認
    #[allow(dead_code)]
    pub fn is_encrypted(&self) -> bool {
        self.encrypted_credentials.is_some()
    }

    /// `encrypted_credentials`が設定されていれば復号し、平文の
    /// `username`/`password`フィールドを復号結果で上書きする。設定読み込み
    /// 時に一度呼ぶことで、以降は`WordPressHandler::try_new`等の既存の
    /// 平文経路がそのまま動く。
    ///
    /// `encrypted_credentials`もplaintextの`username`/`password`も
    /// どちらも無い場合はエラーを返す。`username`/`password`は
    /// `encrypted_credentials`のみの設定を許容するために
    /// `#[serde(default)]`（省略時は空文字列）にしているため、
    /// この関数を呼ばないと「認証情報が一切無い」設定が検証なしで
    /// 空パスワードのまま起動してしまう。
    ///
    /// マスターパスワードは`WORDPRESS_MASTER_PASSWORD`環境変数からのみ
    /// 取得する。暗号化データを設定ファイルに置けても、復号鍵まで同じ
    /// ファイルに書いたら暗号化の意味がないため、設定ファイル経由では
    /// 受け付けない。
    pub fn resolve_encrypted_credentials(&mut self) -> Result<(), EncryptionError> {
        let Some(encrypted) = self.encrypted_credentials.clone() else {
            // enabled=falseの場合は実際にWordPressHandlerを起動しないため
            // （main.rs/dynamic.rsの`wp_config.enabled.unwrap_or(false)`
            // チェック参照）、認証情報が空でも検証エラーにしない。プレース
            // ホルダーとして空のusername/passwordのままセクションだけ
            // 残しておく既存の使い方を壊さないようにするため
            if self.enabled.unwrap_or(false)
                && (self.username.is_empty() || self.password.is_empty())
            {
                return Err(EncryptionError::InvalidInput(
                    "WordPress認証情報が設定されていません。username/passwordまたはencrypted_credentialsのいずれかを指定してください".to_string(),
                ));
            }
            return Ok(());
        };

        let master_password = std::env::var("WORDPRESS_MASTER_PASSWORD").map_err(|_| {
            EncryptionError::InvalidInput(
                "encrypted_credentialsが設定されていますが、WORDPRESS_MASTER_PASSWORD環境変数が見つかりません".to_string(),
            )
        })?;

        let secure_creds = SecureCredentials::from_encrypted(&encrypted, &master_password)?;
        self.username = secure_creds.username.clone();
        self.password = secure_creds.get_password().expose_secret().to_string();
        Ok(())
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_addr: Some("127.0.0.1:8080".to_string()),
                stdio: Some(false),
                log_level: Some("info".to_string()),
                log_retention: Some(LogRetentionConfig {
                    policy: Some("external".to_string()), // 業界標準：外部管理
                    days: Some(30),
                    count: Some(10),
                    size_mb: Some(100),
                }),
                log_module: Some(LogModuleConfig {
                    separation: Some("separated".to_string()), // 本番推奨：モジュール分離
                }),
            },
            transport: TransportConfig {
                transport_type: Some("stdio".to_string()),
                stdio: Some(StdioTransportConfig {
                    buffer_size: Some(8192),
                    timeout_ms: Some(30000),
                    content_length_header: Some(true),
                    framing_method: Some("content-length".to_string()),
                    max_message_size: Some(1048576),
                    pretty_print: Some(false),
                }),
                http: None,
            },
            handlers: HandlersConfig { wordpress: None },
            plugins: Vec::new(),
        }
    }
}

/// 先頭の`~/`をホームフォルダに置き換える（#375）。`~`で始まらない
/// パスはそのまま返す。ホームフォルダが分からない場合は`None`。
fn expand_home_dir(path: &str, home: Option<&str>) -> Option<String> {
    match path.strip_prefix("~/") {
        Some(rest) => home.filter(|h| !h.is_empty()).map(|h| {
            // 要素ごとにjoinし、WindowsでもOSの区切り文字にそろえる
            rest.split('/')
                .fold(PathBuf::from(h), |p, part| p.join(part))
                .to_string_lossy()
                .into_owned()
        }),
        None => Some(path.to_string()),
    }
}

/// 設定ファイルの`[transport] type = ...`を`transport_type`に書き換える（#375）。
///
/// `TransportConfig::transport_type`は`#[serde(alias = "type")]`で`type`も
/// 受け付けるが、`McpConfig::load()`は既定値（`transport_type`）と設定
/// ファイルを`config`クレートで重ねるため、ファイルが`type`で書かれて
/// いると両方のキーが残り、`duplicate field`で読み込みに失敗していた。
/// キー名をそろえておけば、ファイルの値が既定値を上書きする。
fn normalize_transport_type_alias(content: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut table: toml::Table = content.parse()?;
    if let Some(toml::Value::Table(transport)) = table.get_mut("transport") {
        if let Some(value) = transport.remove("type") {
            if transport.contains_key("transport_type") {
                return Err(
                    "[transport] に type と transport_type が両方指定されています。どちらか一方にしてください"
                        .into(),
                );
            }
            transport.insert("transport_type".to_string(), value);
        }
    }
    Ok(toml::to_string(&table)?)
}

impl McpConfig {
    /// 文字列内の環境変数を展開する
    ///
    /// # 形式
    /// `${VAR_NAME}` の形式で環境変数を参照します。
    ///
    /// # 例
    /// ```
    /// # use std::env;
    /// # use mcp_rs::config::McpConfig;
    /// env::set_var("TEST_VAR", "value123");
    /// let result = McpConfig::expand_env_vars("url = ${TEST_VAR}");
    /// assert_eq!(result, "url = value123");
    /// ```
    ///
    /// # 注意
    /// - 環境変数が見つからない場合は `[ERROR:VAR_NAME]` に置換されます
    /// - 最大100回まで再帰的に展開します（無限ループ防止）
    pub fn expand_env_vars(input: &str) -> String {
        let mut result = input.to_string();
        let mut processed_vars = std::collections::HashSet::new();
        let max_iterations = 100; // 無限ループ防止
        let mut iteration_count = 0;

        // ${VAR_NAME} パターンを検索して置換
        loop {
            iteration_count += 1;
            if iteration_count > max_iterations {
                warn!(
                    "環境変数展開で最大反復回数({})に達しました。処理を停止します。",
                    max_iterations
                );
                break;
            }

            if let Some(start) = result.find("${") {
                if let Some(end_pos) = result[start..].find('}') {
                    let end = start + end_pos;
                    let var_name = &result[start + 2..end];

                    // 既に処理済みで値が見つからなかった変数は再処理しない
                    let var_pattern = format!("${{{}}}", var_name);
                    if processed_vars.contains(&var_pattern) {
                        warn!(
                            "環境変数 '{}' は既に処理済みで値が見つかりません。スキップします。",
                            var_name
                        );
                        // この変数をスキップして次を探す - より安全な方法で処理停止
                        break;
                    }

                    match std::env::var(var_name) {
                        Ok(env_value) => {
                            debug!(
                                "環境変数展開成功: {} = {}",
                                var_name,
                                &env_value[..env_value.len().min(20)]
                            );
                            result.replace_range(start..end + 1, &env_value);
                            // 成功した場合は続行
                        }
                        Err(_) => {
                            warn!("環境変数 '{}' が設定されていません。", var_name);
                            processed_vars.insert(var_pattern.clone());

                            // 環境変数が見つからない場合の処理選択肢：
                            // 1. エラーとして処理を停止
                            // 2. 空文字に置換
                            // 3. プレースホルダーに置換

                            // Option 1: エラーとして停止（推奨）
                            return result.replace(&var_pattern, &format!("[ERROR:{}]", var_name));

                            // Option 2: 空文字に置換（コメントアウト）
                            // result.replace_range(start..end + 1, "");

                            // Option 3: 分かりやすいプレースホルダー（コメントアウト）
                            // result.replace_range(start..end + 1, &format!("[MISSING:{}]", var_name));
                        }
                    }
                } else {
                    warn!("無効な環境変数形式が検出されました。開始位置: {}", start);
                    // 無効な形式の場合、その部分をエラーマーカーに置換
                    result.replace_range(start..start + 2, "[INVALID_ENV_VAR]");
                    break;
                }
            } else {
                break; // ${がない場合は正常終了
            }
        }

        debug!("環境変数展開完了。反復回数: {}", iteration_count);
        result
    }

    /// WordPressConfig の環境変数を展開
    pub fn expand_wordpress_config(config: &mut WordPressConfig) {
        config.url = Self::expand_env_vars(&config.url);
        config.username = Self::expand_env_vars(&config.username);
        config.password = Self::expand_env_vars(&config.password);
    }

    /// 設定ファイルから読み込み、環境変数で上書き
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let mut settings = config::Config::builder();

        // デフォルト値を設定
        let default_config = McpConfig::default();
        settings = settings.add_source(config::Config::try_from(&default_config)?);

        // 設定ファイルを読み込み（複数の場所を試行）
        let config_paths = [
            "mcp-config.toml",
            "config.toml",
            "config/mcp.toml",
            "~/.config/mcp-rs/config.toml",
        ];

        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok();

        let mut config_file_found = false;
        for path in &config_paths {
            // `~`はPathが展開しないため、自前でホームフォルダに置き換える。
            // ホームフォルダが分からない場合、その候補は飛ばす
            let Some(path) = expand_home_dir(path, home.as_deref()) else {
                continue;
            };
            if std::path::Path::new(&path).exists() {
                // 標準出力はSTDIOモードでJSON-RPCの通信専用のため、
                // 標準エラー出力に書く（ログはこの時点で未初期化）
                eprintln!("[mcp-rs] config file: {}", path);
                let content = std::fs::read_to_string(&path)?;
                let content = normalize_transport_type_alias(&content)
                    .map_err(|e| with_config_hint(e, &content))?;
                settings =
                    settings.add_source(config::File::from_str(&content, config::FileFormat::Toml));
                config_file_found = true;
                break;
            }
        }

        // 設定ファイルが見つからない場合はエラーを返す
        if !config_file_found && !Self::has_env_config() {
            return Err("設定ファイルが見つかりません".into());
        }

        // 環境変数で上書き (MCP_で始まる変数)
        settings = settings.add_source(
            config::Environment::with_prefix("MCP")
                .separator("_")
                .try_parsing(true),
        );

        // 特定の環境変数も直接対応（後方互換性）
        settings = settings.add_source(config::Environment::default().try_parsing(true));

        let config: McpConfig = settings.build()?.try_deserialize()?;

        // 環境変数による個別上書き
        let mut final_config = config;

        // WordPressConfig の環境変数展開を適用
        if let Some(ref mut wp_config) = final_config.handlers.wordpress {
            info!("WordPress設定で環境変数展開を適用中...");
            Self::expand_wordpress_config(wp_config);
        }

        // WordPress設定の環境変数上書き
        if let Ok(wp_url) = std::env::var("WORDPRESS_URL") {
            if final_config.handlers.wordpress.is_none() {
                final_config.handlers.wordpress = Some(WordPressConfig {
                    url: wp_url,
                    username: std::env::var("WORDPRESS_USERNAME").unwrap_or_default(),
                    password: std::env::var("WORDPRESS_PASSWORD").unwrap_or_default(),
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                    rate_limit: Some(RateLimitConfig::default()),
                    encrypted_credentials: None, // 環境変数では平文使用
                    upload_allowed_dirs: Vec::new(),
                });
            } else if let Some(ref mut wp_config) = final_config.handlers.wordpress {
                wp_config.url = wp_url;
                if let Ok(username) = std::env::var("WORDPRESS_USERNAME") {
                    wp_config.username = username;
                }
                if let Ok(password) = std::env::var("WORDPRESS_PASSWORD") {
                    wp_config.password = password;
                }
            }
        }

        // encrypted_credentialsの解決は上記の平文環境変数上書きより後に
        // 行う。そうしないと、暗号化認証情報へ移行した後も環境に残った
        // 古いWORDPRESS_PASSWORD等が、復号済みの値を無言で上書きしてしまう
        if let Some(ref mut wp_config) = final_config.handlers.wordpress {
            wp_config.resolve_encrypted_credentials()?;
        }

        // サーバー設定の環境変数上書き
        if let Ok(bind_addr) = std::env::var("BIND_ADDR") {
            final_config.server.bind_addr = Some(bind_addr);
        }

        if std::env::var("MCP_STDIO").is_ok() {
            final_config.server.stdio = Some(true);
        }

        Ok(final_config)
    }

    /// 環境変数による設定があるかチェック
    fn has_env_config() -> bool {
        // 重要な環境変数が設定されているかチェック
        std::env::var("WORDPRESS_URL").is_ok()
            || std::env::var("MCP_WORDPRESS_URL").is_ok()
            || std::env::var("MCP_SERVER_BIND_ADDR").is_ok()
    }

    /// サンプル設定ファイルを生成
    pub fn generate_sample_config() -> Result<(), Box<dyn std::error::Error>> {
        let sample_config = McpConfig {
            server: ServerConfig {
                bind_addr: Some("127.0.0.1:8080".to_string()),
                stdio: Some(false),
                log_level: Some("info".to_string()),
                log_retention: Some(LogRetentionConfig {
                    policy: Some("external".to_string()), // OS/ログ管理ツール任せ（推奨）
                    days: Some(30),                       // 開発環境用：30日後削除
                    count: Some(10),                      // 簡易環境用：10ファイル保持
                    size_mb: Some(100),                   // リソース制約環境用：100MB制限
                }),
                log_module: Some(LogModuleConfig {
                    separation: Some("separated".to_string()), // 本番推奨：モジュール分離
                }),
            },
            transport: TransportConfig {
                transport_type: Some("stdio".to_string()),
                stdio: Some(StdioTransportConfig {
                    buffer_size: Some(8192),
                    timeout_ms: Some(30000),
                    content_length_header: Some(true),
                    framing_method: Some("content-length".to_string()),
                    max_message_size: Some(1048576),
                    pretty_print: Some(false),
                }),
                http: Some(HttpTransportConfig {
                    addr: Some("127.0.0.1".to_string()),
                    port: Some(8080),
                    enable_cors: Some(true),
                    tls_enabled: Some(false),
                    tls_cert_path: Some("./certs/server.crt".to_string()),
                    tls_key_path: Some("./certs/server.key".to_string()),
                    mtls_enabled: Some(false),
                    mtls_ca_cert_path: Some("./certs/ca.crt".to_string()),
                    enforce_https: Some(true),
                    min_tls_version: Some("1.3".to_string()),
                    hsts_enabled: Some(true),
                    hsts_max_age_seconds: Some(31536000),
                    hsts_include_subdomains: Some(true),
                    hsts_preload: Some(false),
                    certificate_pinning_enabled: Some(false),
                    pinned_certificates_sha256: Some(vec![]),
                    certificate_pin_header: Some("x-tls-cert-sha256".to_string()),
                    anti_replay_enabled: Some(false),
                    ids_enabled: Some(false),
                    ids_trust_forwarded_for: Some(false),
                    network_policy_reject_external_connections: Some(true),
                    network_policy_warn_on_external_bind: Some(true),
                    network_policy_ip_whitelist: Some(vec![]),
                    enable_websocket_upgrade: Some(false),
                    websocket_max_connections: Some(1000),
                    auth_enabled: Some(false),
                    api_keys: Some(vec![]),
                }),
            },
            handlers: HandlersConfig {
                wordpress: Some(WordPressConfig {
                    url: "${WORDPRESS_URL}".to_string(),
                    username: "${WORDPRESS_USERNAME}".to_string(),
                    password: "${WORDPRESS_PASSWORD}".to_string(),
                    enabled: Some(true),
                    timeout_seconds: Some(30),
                    rate_limit: Some(RateLimitConfig::default()),
                    encrypted_credentials: None, // デフォルトでは平文
                    upload_allowed_dirs: Vec::new(),
                }),
            },
            plugins: Vec::new(),
        };

        let toml_content = toml::to_string_pretty(&sample_config)?;

        let sample_content = format!(
            r#"# MCP-RS Configuration File
#
# このファイルは mcp-config.toml として保存してください
# セキュリティのため、認証情報は環境変数を使用することを推奨します

{}

# 🔒 セキュリティ設定説明:
#
# 環境変数を使用した安全な設定方法:
#
# 1. 環境変数を設定:
#    export WORDPRESS_URL="https://your-wordpress-site.com"
#    export WORDPRESS_USERNAME="your_username"
#    export WORDPRESS_PASSWORD="your_app_password"
#
# 2. または .env ファイルを使用:
#    WORDPRESS_URL=https://your-wordpress-site.com
#    WORDPRESS_USERNAME=your_username
#    WORDPRESS_PASSWORD=your_app_password
#
# 3. 設定ファイルでは環境変数参照 (ドル記号+波括弧形式):
#    url = "${{WORDPRESS_URL}}"
#    username = "${{WORDPRESS_USERNAME}}"
#    password = "${{WORDPRESS_PASSWORD}}"
#
# [server]
# bind_addr = TCP サーバーのバインドアドレス (stdio=false の場合)
# stdio = true にすると標準入出力モードで動作 (MCP クライアント用)
# log_level = ログレベル (trace, debug, info, warn, error)
#
# [handlers.wordpress]
# url = WordPress サイトの URL
# username = WordPress ユーザー名
# password = Application Password (WordPress管理画面で生成)
# enabled = このハンドラーを有効にするか
#
# 🔑 Application Password の生成方法:
# 1. WordPress管理画面 > ユーザー > プロフィール
# 2. 'アプリケーションパスワード' セクション
# 3. 新しいアプリケーション名を入力 (例: "MCP-RS")
# 4. 'Add New Application Password' をクリック
# 5. 生成されたパスワードを環境変数に設定
#
# ⚠️  注意: パスワードを設定ファイルに直接記載しないでください！
"#,
            toml_content
        );

        std::fs::write("mcp-config.toml.example", sample_content)?;
        // load()と同じく、標準出力（STDIOモードの通信路）には書かない
        eprintln!("📝 サンプル設定ファイルを生成しました: mcp-config.toml.example");
        eprintln!("💡 このファイルを mcp-config.toml にコピーして編集してください");

        Ok(())
    }

    /// 設定をファイルに保存
    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let toml_content = toml::to_string_pretty(self)?;

        let content = format!(
            r#"# MCP-RS Configuration File
#
# This configuration was generated by MCP-RS setup wizard
# セキュリティのため、認証情報は環境変数を使用することを推奨します

{}

# 🔒 セキュリティ設定説明:
#
# 環境変数を使用した安全な設定方法:
#
# 1. 環境変数を設定:
#    $env:WORDPRESS_URL="https://your-wordpress-site.com"
#    $env:WORDPRESS_USERNAME="your_username"
#    $env:WORDPRESS_PASSWORD="your_app_password"
#
# 2. または .env ファイルを使用:
#    WORDPRESS_URL=https://your-wordpress-site.com
#    WORDPRESS_USERNAME=your_username
#    WORDPRESS_PASSWORD=your_app_password
#
# 3. Application Password の生成方法:
#    - WordPress管理画面 > ユーザー > プロフィール
#    - 'アプリケーションパスワード' セクション
#    - 新しいアプリケーション名を入力 (例: "MCP-RS")
#    - 'Add New Application Password' をクリック
#    - 生成されたパスワードを使用
#
# ⚠️  注意: パスワードを設定ファイルに直接記載しないでください！
"#,
            toml_content
        );

        std::fs::write(path, content)?;
        Ok(())
    }

    /// Convert to transport configuration
    pub fn to_transport_config(&self) -> crate::transport::TransportConfig {
        use crate::transport::{stdio::StdioConfig, FramingMethod, TransportType};

        let transport_type = match self.transport.transport_type.as_deref() {
            Some("stdio") => TransportType::Stdio,
            Some("http") => {
                let addr = self
                    .transport
                    .http
                    .as_ref()
                    .and_then(|h| h.addr.as_deref())
                    .unwrap_or("127.0.0.1");
                let port = self
                    .transport
                    .http
                    .as_ref()
                    .and_then(|h| h.port)
                    .unwrap_or(8080);
                TransportType::Http {
                    addr: format!("{}:{}", addr, port)
                        .parse()
                        .unwrap_or_else(|_| "127.0.0.1:8080".parse().unwrap()),
                }
            }
            _ => TransportType::Stdio, // Default fallback
        };

        let stdio_config = if let Some(stdio) = &self.transport.stdio {
            StdioConfig {
                buffer_size: stdio.buffer_size.unwrap_or(8192),
                timeout_ms: stdio.timeout_ms.unwrap_or(30000),
                content_length_header: stdio.content_length_header.unwrap_or(true),
                framing_method: match stdio.framing_method.as_deref() {
                    Some("line-based") => FramingMethod::LineBased,
                    _ => FramingMethod::ContentLength,
                },
                max_message_size: stdio.max_message_size.unwrap_or(1048576),
                pretty_print: stdio.pretty_print.unwrap_or(false),
            }
        } else {
            StdioConfig::default()
        };

        let http_config = if let Some(ref http) = self.transport.http {
            let addr = http.addr.as_deref().unwrap_or("127.0.0.1");
            let port = http.port.unwrap_or(8081);
            let bind_addr_str = format!("{}:{}", addr, port);
            crate::transport::http::HttpConfig {
                bind_addr: bind_addr_str
                    .parse()
                    .unwrap_or("127.0.0.1:8081".parse().unwrap()),
                cors_enabled: http.enable_cors.unwrap_or(true),
                max_request_size: 1048576,
                timeout_ms: 30000,
                network_policy: crate::security::NetworkPolicy {
                    reject_external_connections: http
                        .network_policy_reject_external_connections
                        .unwrap_or(true),
                    warn_on_external_bind: http
                        .network_policy_warn_on_external_bind
                        .unwrap_or(true),
                    ip_whitelist: http.network_policy_ip_whitelist.clone().unwrap_or_default(),
                },
                tls_enabled: http.tls_enabled.unwrap_or(false),
                tls_cert_path: http.tls_cert_path.clone(),
                tls_key_path: http.tls_key_path.clone(),
                mtls_enabled: http.mtls_enabled.unwrap_or(false),
                mtls_ca_cert_path: http.mtls_ca_cert_path.clone(),
                enforce_https: http.enforce_https.unwrap_or(false),
                min_tls_version: http.min_tls_version.clone().or(Some("1.3".to_string())),
                hsts_enabled: http.hsts_enabled.unwrap_or(true),
                hsts_max_age_seconds: http.hsts_max_age_seconds.unwrap_or(31536000),
                hsts_include_subdomains: http.hsts_include_subdomains.unwrap_or(true),
                hsts_preload: http.hsts_preload.unwrap_or(false),
                certificate_pinning_enabled: http.certificate_pinning_enabled.unwrap_or(false),
                pinned_certificates_sha256: http
                    .pinned_certificates_sha256
                    .clone()
                    .unwrap_or_default(),
                certificate_pin_header: http
                    .certificate_pin_header
                    .clone()
                    .unwrap_or_else(|| "x-tls-cert-sha256".to_string()),
                anti_replay_enabled: http.anti_replay_enabled.unwrap_or(false),
                ids_enabled: http.ids_enabled.unwrap_or(false),
                ids_trust_forwarded_for: http.ids_trust_forwarded_for.unwrap_or(false),
                enable_websocket_upgrade: http.enable_websocket_upgrade.unwrap_or(false),
                websocket_max_connections: http.websocket_max_connections.unwrap_or(1000),
                auth_enabled: http.auth_enabled.unwrap_or(false),
                api_keys: http
                    .api_keys
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(crate::transport::http::ApiKeyEntry::from)
                    .collect(),
            }
        } else {
            crate::transport::http::HttpConfig::default()
        };

        crate::transport::TransportConfig {
            transport_type,
            stdio: stdio_config,
            http: http_config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLUGINS_TOML: &str = r#"
[transport]
type = "stdio"

[server]
log_level = "warn"

[handlers]

[[plugins]]
name = "example"
command = "C:/tools/example-mcp.exe"
args = ["--stdio"]
env = { EXAMPLE_API_KEY = "${EXAMPLE_API_KEY}", Mixed_Case = "x" }

[[plugins]]
name = "second"
command = "second-mcp"
enabled = false
call_timeout_seconds = 5
"#;

    fn assert_example_plugins(config: &McpConfig) {
        assert_eq!(config.plugins.len(), 2);
        let first = &config.plugins[0];
        assert_eq!(first.name, "example");
        assert_eq!(first.args, vec!["--stdio"]);
        // 環境変数名の大文字・小文字を保つ
        assert_eq!(
            first.env.get("EXAMPLE_API_KEY").map(String::as_str),
            Some("${EXAMPLE_API_KEY}")
        );
        assert_eq!(first.env.get("Mixed_Case").map(String::as_str), Some("x"));
        assert_eq!(config.plugins[1].enabled, Some(false));
        assert_eq!(config.plugins[1].call_timeout_seconds, Some(5));
    }

    #[test]
    fn test_plugins_are_read_from_toml() {
        let config: McpConfig = toml::from_str(PLUGINS_TOML).unwrap();
        assert_example_plugins(&config);
    }

    /// `McpConfig::load()`は既定値と設定ファイルを`config`クレートで
    /// 重ねる。その経路でも`[[plugins]]`と、`env`のキーの大文字・小文字が
    /// 保たれること
    #[test]
    fn test_plugins_are_read_through_config_crate() {
        let content = normalize_transport_type_alias(PLUGINS_TOML).unwrap();
        let config: McpConfig = config::Config::builder()
            .add_source(config::Config::try_from(&McpConfig::default()).unwrap())
            .add_source(config::File::from_str(&content, config::FileFormat::Toml))
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();
        assert_example_plugins(&config);
    }

    #[test]
    fn test_no_plugins_by_default() {
        let config: McpConfig = toml::from_str("[transport]\n[server]\n[handlers]\n").unwrap();
        assert!(config.plugins.is_empty());
        assert!(McpConfig::default().plugins.is_empty());
    }

    /// v0.18.2で「残っていても無視される」とした、v0.18.1以前の`[plugins]`
    /// （テーブル）は、エラーにせず読み飛ばす
    #[test]
    fn test_legacy_plugins_table_is_ignored() {
        let legacy = r#"
[transport]
[server]
[handlers]

[plugins]
enabled = true
plugin_dirs = ["./plugins"]
auto_load = true

[plugins.isolation]
enabled = true
"#;
        let config: McpConfig = toml::from_str(legacy).unwrap();
        assert!(config.plugins.is_empty());

        let content = normalize_transport_type_alias(legacy).unwrap();
        let config: McpConfig = config::Config::builder()
            .add_source(config::Config::try_from(&McpConfig::default()).unwrap())
            .add_source(config::File::from_str(&content, config::FileFormat::Toml))
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();
        assert!(config.plugins.is_empty());
    }

    #[test]
    fn test_legacy_plugins_conflict_gets_a_hint() {
        let conflict = r#"
[handlers]

[plugins]
search_paths = ["./plugins"]

[plugins.plugins]

[[plugins]]
name = "fs"
command = "node"
"#;
        // TOMLとして読めないこと（serdeでは救えない）を前提として確かめる
        let error = toml::from_str::<McpConfig>(conflict).unwrap_err();
        let message = with_config_hint(&error, conflict);
        assert!(
            message.contains("Delete the old [plugins] section"),
            "{}",
            message
        );
        assert!(normalize_transport_type_alias(conflict).is_err());

        // 片方だけなら、ヒントは付けない
        assert!(legacy_plugins_conflict_hint("[plugins]\nauto_load = true\n").is_none());
        assert!(legacy_plugins_conflict_hint("[[plugins]]\nname = \"a\"\n").is_none());
        // コメントアウトした見出しは数えない
        assert!(legacy_plugins_conflict_hint("# [plugins]\n[[plugins]]\n").is_none());
        // `[plugins.container]`は`[[plugins]]`のコンテナ設定（#411）
        assert!(legacy_plugins_conflict_hint(
            "[[plugins]]\nname = \"a\"\n[plugins.container]\nimage = \"i\"\n"
        )
        .is_none());
    }

    #[test]
    fn test_invalid_plugin_entry_is_an_error() {
        let missing_name = "[transport]\n[server]\n[handlers]\n[[plugins]]\ncommand = \"x\"\n";
        let error = toml::from_str::<McpConfig>(missing_name)
            .unwrap_err()
            .to_string();
        assert!(error.contains("name"), "{}", error);
    }

    const CONTAINER_PLUGIN_TOML: &str = r#"
[transport]
[server]
[handlers]

[[plugins]]
name = "fs"
args = ["/projects"]
env = { API_KEY = "${FS_API_KEY}" }

[plugins.container]
image = "mcp/filesystem"
memory = "256m"
mounts = [
  { source = "C:/Users/me/Documents", target = "/projects" },
  { source = "C:/Users/me/out", target = "/out", read_only = false },
]

[[plugins]]
name = "local"
command = "local-mcp"
"#;

    fn assert_container_plugins(config: &McpConfig) {
        assert_eq!(config.plugins.len(), 2);
        let fs = &config.plugins[0];
        // コンテナではcommandを省略できる（イメージのENTRYPOINTを使う）
        assert_eq!(fs.command, "");
        let container = fs.container.as_ref().unwrap();
        assert_eq!(container.image, "mcp/filesystem");
        assert_eq!(container.memory.as_deref(), Some("256m"));
        assert_eq!(container.network, None);
        assert_eq!(container.mounts.len(), 2);
        assert_eq!(container.mounts[0].target, "/projects");
        assert_eq!(container.mounts[0].read_only, None);
        assert_eq!(container.mounts[1].read_only, Some(false));
        assert!(config.plugins[1].container.is_none());
    }

    #[test]
    fn test_container_plugins_are_read_from_toml() {
        let config: McpConfig = toml::from_str(CONTAINER_PLUGIN_TOML).unwrap();
        assert_container_plugins(&config);

        let content = normalize_transport_type_alias(CONTAINER_PLUGIN_TOML).unwrap();
        let config: McpConfig = config::Config::builder()
            .add_source(config::Config::try_from(&McpConfig::default()).unwrap())
            .add_source(config::File::from_str(&content, config::FileFormat::Toml))
            .build()
            .unwrap()
            .try_deserialize()
            .unwrap();
        assert_container_plugins(&config);
    }

    #[test]
    fn test_plugin_config_debug_hides_env_values() {
        let config: McpConfig = toml::from_str(
            "[transport]\n[server]\n[handlers]\n[[plugins]]\nname = \"x\"\ncommand = \"c\"\nenv = { API_KEY = \"super-secret-value\" }\n",
        )
        .unwrap();
        let debug = format!("{:?}", config.plugins[0]);
        assert!(debug.contains("API_KEY"));
        assert!(!debug.contains("super-secret-value"));
    }

    #[test]
    fn test_expand_home_dir() {
        let expected = std::path::Path::new("/home/alice")
            .join(".config")
            .join("mcp-rs")
            .join("config.toml")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            expand_home_dir("~/.config/mcp-rs/config.toml", Some("/home/alice")),
            Some(expected)
        );
        // ホームフォルダが分からなければ、その候補は使わない
        assert_eq!(expand_home_dir("~/.config/x.toml", None), None);
        assert_eq!(expand_home_dir("~/.config/x.toml", Some("")), None);
        // `~`で始まらないパスはそのまま
        assert_eq!(
            expand_home_dir("mcp-config.toml", None),
            Some("mcp-config.toml".to_string())
        );
    }

    #[test]
    fn test_normalize_transport_type_alias() {
        let normalized = normalize_transport_type_alias("[transport]\ntype = \"stdio\"\n").unwrap();
        let table: toml::Table = normalized.parse().unwrap();
        let transport = table["transport"].as_table().unwrap();
        assert_eq!(transport["transport_type"].as_str(), Some("stdio"));
        assert!(!transport.contains_key("type"));

        // transport_typeで書かれていればそのまま
        let normalized =
            normalize_transport_type_alias("[transport]\ntransport_type = \"http\"\n").unwrap();
        let table: toml::Table = normalized.parse().unwrap();
        assert_eq!(table["transport"]["transport_type"].as_str(), Some("http"));

        // 両方あれば曖昧なのでエラー
        assert!(normalize_transport_type_alias(
            "[transport]\ntype = \"stdio\"\ntransport_type = \"http\"\n"
        )
        .is_err());
    }

    #[test]
    fn test_expand_env_vars_basic() {
        unsafe {
            std::env::set_var("TEST_VAR", "test_value");
        }
        let result = McpConfig::expand_env_vars("${TEST_VAR}");
        assert_eq!(result, "test_value");
        unsafe {
            std::env::remove_var("TEST_VAR");
        }
    }

    #[test]
    fn test_expand_env_vars_multiple() {
        unsafe {
            std::env::set_var("VAR1", "value1");
            std::env::set_var("VAR2", "value2");
        }
        let result = McpConfig::expand_env_vars("${VAR1} and ${VAR2}");
        assert_eq!(result, "value1 and value2");
        unsafe {
            std::env::remove_var("VAR1");
            std::env::remove_var("VAR2");
        }
    }

    #[test]
    fn test_expand_env_vars_missing() {
        let result = McpConfig::expand_env_vars("${NONEXISTENT_VAR}");
        assert_eq!(result, "[ERROR:NONEXISTENT_VAR]");
    }

    #[test]
    fn test_expand_env_vars_nested() {
        unsafe {
            std::env::set_var("OUTER", "prefix_${INNER}_suffix");
            std::env::set_var("INNER", "middle");
        }
        let result = McpConfig::expand_env_vars("${OUTER}");
        assert_eq!(result, "prefix_middle_suffix");
        unsafe {
            std::env::remove_var("OUTER");
            std::env::remove_var("INNER");
        }
    }

    #[test]
    fn test_transport_type_accepts_type_alias() {
        // configs/production/claude-desktop.toml (and main.toml/web-ui.toml/
        // development/http-transport.toml) write `type = "stdio"` instead of
        // `transport_type = "stdio"`; the alias must resolve both forms.
        let via_alias: TransportConfig = toml::from_str(r#"type = "stdio""#).unwrap();
        assert_eq!(via_alias.transport_type.as_deref(), Some("stdio"));

        let via_field_name: TransportConfig =
            toml::from_str(r#"transport_type = "stdio""#).unwrap();
        assert_eq!(via_field_name.transport_type.as_deref(), Some("stdio"));
    }

    #[test]
    fn test_expand_env_vars_empty_input() {
        let result = McpConfig::expand_env_vars("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_expand_env_vars_no_variables() {
        let input = "plain text without variables";
        let result = McpConfig::expand_env_vars(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_expand_env_vars_special_chars() {
        unsafe {
            std::env::set_var("SPECIAL", "test@example.com:8080/path?query=1");
        }
        let result = McpConfig::expand_env_vars("url=${SPECIAL}");
        assert_eq!(result, "url=test@example.com:8080/path?query=1");
        unsafe {
            std::env::remove_var("SPECIAL");
        }
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[test]
    fn test_expand_env_vars_with_defaults() {
        let result = McpConfig::expand_env_vars("${MISSING:-default_value}");
        assert!(result.contains("default") || result.contains("ERROR"));
    }

    #[test]
    fn test_expand_env_vars_recursive() {
        unsafe {
            std::env::set_var("A", "${B}");
            std::env::set_var("B", "final");
        }
        let result = McpConfig::expand_env_vars("${A}");
        assert_eq!(result, "final");
        unsafe {
            std::env::remove_var("A");
            std::env::remove_var("B");
        }
    }

    #[test]
    fn test_expand_env_vars_malformed() {
        let result = McpConfig::expand_env_vars("${INCOMPLETE");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_wordpress_config_accepts_encrypted_credentials_without_plaintext() {
        // encrypted_credentialsのみを指定した設定もデシリアライズできる
        // こと（username/passwordは#[serde(default)]により省略可）
        let toml_str = r#"
            url = "https://example.com"

            [encrypted_credentials]
            username = "wp-user"
            encrypted_password = "ZGVhZGJlZWY="
            nonce = "AAAAAAAAAAAAAAAA"
            salt = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        "#;
        let config: WordPressConfig =
            toml::from_str(toml_str).expect("should deserialize without username/password");
        assert_eq!(config.username, "");
        assert_eq!(config.password, "");
        assert!(config.encrypted_credentials.is_some());
    }

    #[test]
    fn test_wordpress_config_debug_redacts_password() {
        let config = WordPressConfig {
            url: "https://example.com".to_string(),
            username: "wp-user".to_string(),
            password: "super-secret-plaintext".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
            upload_allowed_dirs: Vec::new(),
        };

        let debug_output = format!("{:?}", config);
        assert!(!debug_output.contains("super-secret-plaintext"));
        assert!(debug_output.contains("[REDACTED]"));
        // パスワード以外のフィールドは引き続き見えること
        assert!(debug_output.contains("wp-user"));
    }

    #[test]
    fn test_resolve_encrypted_credentials_decrypts_and_requires_master_password_env_var() {
        let _env_guard = WORDPRESS_MASTER_PASSWORD_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let plaintext =
            SecureCredentials::new("wp-user".to_string(), "wp-app-password".to_string());
        let encrypted = plaintext.encrypt("correct-master-password").unwrap();

        let mut config = WordPressConfig {
            url: "https://example.com".to_string(),
            username: String::new(),
            password: String::new(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: Some(encrypted),
            upload_allowed_dirs: Vec::new(),
        };

        // マスターパスワード環境変数が無い場合はエラーになり、
        // username/passwordは変更されない
        unsafe {
            std::env::remove_var("WORDPRESS_MASTER_PASSWORD");
        }
        assert!(config.resolve_encrypted_credentials().is_err());
        assert_eq!(config.username, "");
        assert_eq!(config.password, "");

        // マスターパスワードがあれば復号されてusername/passwordが
        // 上書きされる
        unsafe {
            std::env::set_var("WORDPRESS_MASTER_PASSWORD", "correct-master-password");
        }
        config
            .resolve_encrypted_credentials()
            .expect("should decrypt successfully");
        assert_eq!(config.username, "wp-user");
        assert_eq!(config.password, "wp-app-password");

        unsafe {
            std::env::remove_var("WORDPRESS_MASTER_PASSWORD");
        }
    }

    #[test]
    fn test_resolve_encrypted_credentials_is_noop_without_encrypted_credentials() {
        let mut config = WordPressConfig {
            url: "https://example.com".to_string(),
            username: "plain-user".to_string(),
            password: "plain-password".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
            upload_allowed_dirs: Vec::new(),
        };

        config.resolve_encrypted_credentials().unwrap();
        assert_eq!(config.username, "plain-user");
        assert_eq!(config.password, "plain-password");
    }

    #[test]
    fn test_resolve_encrypted_credentials_errors_when_no_credentials_configured() {
        // username/passwordを#[serde(default)]にしたことで
        // 両方省略した設定ファイルもデシリアライズ自体は成功するように
        // なったが、encrypted_credentialsも無ければ認証情報が一切無い
        // ということなので、空パスワードで無言で起動するのではなく
        // ここで検証エラーにする必要がある。
        let mut config = WordPressConfig {
            url: "https://example.com".to_string(),
            username: String::new(),
            password: String::new(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
            upload_allowed_dirs: Vec::new(),
        };

        assert!(config.resolve_encrypted_credentials().is_err());
    }

    #[test]
    fn test_resolve_encrypted_credentials_allows_empty_when_disabled() {
        // enabled=falseの場合は実際にWordPressHandlerが起動しないため、
        // プレースホルダーとして認証情報を空のままセクションだけ残して
        // おく既存の使い方（配布用テンプレート等）を壊さないこと。
        let mut config = WordPressConfig {
            url: "https://example.com".to_string(),
            username: String::new(),
            password: String::new(),
            enabled: Some(false),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
            upload_allowed_dirs: Vec::new(),
        };

        config.resolve_encrypted_credentials().unwrap();
        assert_eq!(config.username, "");
        assert_eq!(config.password, "");
    }

    #[test]
    fn test_resolve_encrypted_credentials_allows_empty_when_enabled_is_unset() {
        // enabledを省略した設定（Noneはunwrap_or(false)で未有効扱い）も
        // 同様に認証情報の空チェックをスキップすること。
        let mut config = WordPressConfig {
            url: "https://example.com".to_string(),
            username: String::new(),
            password: String::new(),
            enabled: None,
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
            upload_allowed_dirs: Vec::new(),
        };

        config.resolve_encrypted_credentials().unwrap();
    }
}
