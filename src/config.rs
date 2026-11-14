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
use tracing::{debug, info};

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
    /// 監査ログ（#416）。省略時は、設定ファイルと同じフォルダの`audit.log`に書く
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditLogSettings>,
    /// データベース（`[database]`、#453）。省略時は使わない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseSettings>,
    /// 読み込んだ設定ファイルのパス（絶対パス）。監査ログの既定の書き込み先や、
    /// 相対パスの基準に使う。設定ファイルには書かない
    #[serde(skip)]
    pub config_file: Option<PathBuf>,
}

/// 監査ログの設定（`[audit]`、#416）
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AuditLogSettings {
    /// `false`でファイルに書かない（メモリにだけ残す）。省略時は`true`
    pub enabled: Option<bool>,
    /// 書き込み先。相対パスは設定ファイルのフォルダを基準にする。省略時は
    /// 設定ファイルと同じフォルダの`audit.log`
    pub file_path: Option<String>,
    /// この大きさ（MB）を超える前に、`audit.log.1`等へローテーションする。
    /// 省略時は100MB。0でローテーションしない
    pub max_file_size_mb: Option<u64>,
    /// ローテーションで残す古いファイルの数。省略時は5
    pub max_files: Option<u32>,
}

/// データベースの設定（`[database]`、#453）。今はPostgreSQLだけ。
///
/// 既定では読み取りだけ: `execute_query`は読み取り専用のトランザクションで
/// 実行し、書き込みのツール（`execute_command`）は出さない。
/// `host`・`database`・`username`・`password`には`${VAR}`（環境変数）を
/// 書ける。`password`は`Debug`に出さない。
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct DatabaseSettings {
    /// `false`で使わない。省略時は`true`（`[database]`を書けば使う）
    pub enabled: Option<bool>,
    /// エンジン。今は`"postgresql"`だけ
    pub engine: String,
    /// 省略時は`localhost`
    pub host: Option<String>,
    /// 省略時は5432
    pub port: Option<u16>,
    pub database: String,
    pub username: String,
    /// 省略時は空（パスワード無し）。`${PGPASSWORD}`のように環境変数で渡せる
    pub password: Option<String>,
    /// TLS: `disable`・`prefer`（省略時）・`require`・`verify-ca`・`verify-full`
    pub ssl_mode: Option<String>,
    /// サーバーの証明書を確かめるCAのファイル（PEM、#539）。非公開のCA
    /// （RDS等）で`verify-ca`・`verify-full`を使うとき。相対パスは設定
    /// ファイルのフォルダを基準にする
    pub ssl_root_cert: Option<String>,
    /// `true`で書き込みのツール（`execute_command`: INSERT・UPDATE・DELETE）を
    /// 出す。省略時は`false`
    pub allow_write: Option<bool>,
    /// `execute_query`が返す行数の上限。省略時は1000
    pub max_rows: Option<u32>,
    /// `execute_query`が返す結果のだいたいのバイト数の上限（#539）。省略時は
    /// 10MB（10485760）
    pub max_result_bytes: Option<u64>,
    /// 1つのSQLの時間の上限（秒）。省略時は30
    pub query_timeout_seconds: Option<u32>,
    /// 接続の数の上限。省略時は5
    pub max_connections: Option<u32>,
}

impl std::fmt::Debug for DatabaseSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseSettings")
            .field("enabled", &self.enabled)
            .field("engine", &self.engine)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("ssl_mode", &self.ssl_mode)
            .field("ssl_root_cert", &self.ssl_root_cert)
            .field("allow_write", &self.allow_write)
            .field("max_rows", &self.max_rows)
            .field("max_result_bytes", &self.max_result_bytes)
            .field("query_timeout_seconds", &self.query_timeout_seconds)
            .field("max_connections", &self.max_connections)
            .finish()
    }
}

impl DatabaseSettings {
    pub const DEFAULT_MAX_ROWS: u32 = 1000;
    pub const DEFAULT_MAX_RESULT_BYTES: u64 = 10 * 1024 * 1024;
    pub const DEFAULT_QUERY_TIMEOUT_SECONDS: u32 = 30;
    pub const DEFAULT_MAX_CONNECTIONS: u32 = 5;

    /// 使う設定か（`enabled`の省略時は`true`）
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn host(&self) -> &str {
        self.host.as_deref().unwrap_or("localhost")
    }

    pub fn port(&self) -> u16 {
        self.port.unwrap_or(5432)
    }

    pub fn allow_write(&self) -> bool {
        self.allow_write.unwrap_or(false)
    }

    pub fn max_rows(&self) -> u32 {
        self.max_rows
            .filter(|n| *n > 0)
            .unwrap_or(Self::DEFAULT_MAX_ROWS)
    }

    pub fn max_result_bytes(&self) -> u64 {
        self.max_result_bytes
            .filter(|n| *n > 0)
            .unwrap_or(Self::DEFAULT_MAX_RESULT_BYTES)
    }

    /// `ssl_root_cert`の相対パスを、`base`（設定ファイルのフォルダ）を基準に
    /// する（#539）
    pub fn resolve_paths(&mut self, base: Option<&std::path::Path>) {
        if let (Some(cert), Some(base)) = (self.ssl_root_cert.as_mut(), base) {
            let path = std::path::Path::new(cert.trim());
            if path.is_relative() {
                *cert = base.join(path).to_string_lossy().into_owned();
            }
        }
    }

    pub fn query_timeout_seconds(&self) -> u32 {
        self.query_timeout_seconds
            .filter(|n| *n > 0)
            .unwrap_or(Self::DEFAULT_QUERY_TIMEOUT_SECONDS)
    }

    pub fn max_connections(&self) -> u32 {
        self.max_connections
            .filter(|n| *n > 0)
            .unwrap_or(Self::DEFAULT_MAX_CONNECTIONS)
    }

    /// `mcp_rs_status`・ログに出す説明（パスワードは含めない）
    pub fn describe(&self) -> String {
        format!(
            "{} {}@{}:{}/{} ({})",
            self.engine,
            self.username,
            self.host(),
            self.port(),
            self.database,
            if self.allow_write() {
                "read-write"
            } else {
                "read-only"
            }
        )
    }

    /// 書いた値を確かめる。起動しない理由を返す
    pub fn validate(&self) -> Result<(), String> {
        if !self.engine.eq_ignore_ascii_case("postgresql") {
            return Err(format!(
                "database.engine = \"{}\" is not supported (supported: \"postgresql\")",
                self.engine
            ));
        }
        if self.database.trim().is_empty() {
            return Err("database.database is empty".to_string());
        }
        if self.username.trim().is_empty() {
            return Err("database.username is empty".to_string());
        }
        if let Some(mode) = &self.ssl_mode {
            const MODES: &[&str] = &[
                "disable",
                "allow",
                "prefer",
                "require",
                "verify-ca",
                "verify-full",
            ];
            if !MODES.contains(&mode.as_str()) {
                return Err(format!(
                    "database.ssl_mode = \"{}\" is not valid (use one of: {})",
                    mode,
                    MODES.join(", ")
                ));
            }
        }
        Ok(())
    }
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
    /// 指定すると、プラグインをKubernetesのJobとして起動する（#413）。
    /// `container`とは同時に指定できない
    pub kubernetes: Option<PluginKubernetesConfig>,
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
            .field("kubernetes", &self.kubernetes)
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

/// プラグインをKubernetesのJobとして起動する設定（#413、
/// `[plugins.kubernetes]`）。`kubectl`で作成・接続する。既定で、root
/// 以外のユーザー・全ケーパビリティ削除・ルートファイルシステム読み取り
/// 専用・通信を拒否するNetworkPolicyの対象・リソースの上限付きで起動する。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginKubernetesConfig {
    /// イメージ
    pub image: String,
    /// 名前空間。省略時はkubeconfigの現在の名前空間
    pub namespace: Option<String>,
    /// kubeconfigのコンテキスト。省略時は現在のコンテキスト
    pub context: Option<String>,
    /// `none`（既定、通信を拒否するNetworkPolicyの対象にする）か`allow`
    pub network: Option<String>,
    /// メモリの上限（例: `"512Mi"`）。省略時は`512Mi`
    pub memory: Option<String>,
    /// CPUの上限（例: `"500m"`）。省略時は`500m`
    pub cpu: Option<String>,
    /// 実行するユーザー（とグループ）のID。省略時は65532。0（root）は不可
    pub run_as_user: Option<u32>,
    /// `false`で、ルートファイルシステムに書き込めるようにする。省略時は
    /// `true`（読み取り専用。`/tmp`だけ書き込める）
    pub read_only: Option<bool>,
    /// `Always`・`IfNotPresent`・`Never`。省略時はKubernetesの既定
    pub image_pull_policy: Option<String>,
    /// Podの起動（イメージの取得を含む）と`initialize`・`tools/list`を
    /// 待つ時間（秒）。省略時は120秒
    pub start_timeout_seconds: Option<u64>,
    /// 使うコマンド。省略時は`kubectl`
    pub kubectl_command: Option<String>,
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
    // `[plugins.container]`・`[plugins.kubernetes]`は、直前の`[[plugins]]`の
    // 設定（#411・#413）
    let has_legacy = headers.iter().any(|h| {
        h == "[plugins]"
            || (h.starts_with("[plugins.")
                && h.ends_with(']')
                && !h.starts_with("[plugins.container")
                && !h.starts_with("[plugins.kubernetes"))
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
    /// OAuth 2.1 access tokens (`[transport.http.oauth]`, #420)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth: Option<crate::transport::oauth::OAuthConfig>,
    /// Origins (e.g. `"https://app.example.com"`) allowed in the `Origin`
    /// header of browser requests, in addition to loopback origins
    /// (`localhost`, `127.0.0.1`, `[::1]`), #484. `"*"` allows any origin.
    /// Requests without an `Origin` header (non-browser clients) are always
    /// accepted. Defaults to empty (loopback origins only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_origins: Option<Vec<String>>,
    /// Maximum size in bytes of a request body and of a WebSocket message
    /// (#540). Defaults to 1048576 (1 MB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_request_size: Option<usize>,
    /// Time limit in seconds for receiving a request (#540): the TLS
    /// handshake and the headers as a whole, and each wait for more of the
    /// body (not the whole body). It does not limit how long a tool call
    /// takes. Defaults to 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_timeout_seconds: Option<u64>,
    /// Maximum number of connections served at the same time, WebSocket
    /// connections included (#540). Defaults to 1024.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<usize>,
    /// Close a `/ws` connection when nothing, not even an answer to the
    /// server's pings, arrives for this many seconds (#540). `0` disables
    /// the limit (and the pings). Defaults to 300.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket_idle_timeout_seconds: Option<u64>,
    /// Start even when listening on a non-loopback address without any
    /// authentication (API keys, OAuth, or mTLS), #540. Only for
    /// deployments where something in front of mcp-rs authenticates
    /// clients. Defaults to `false` (such a configuration does not start).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_unauthenticated_external: Option<bool>,
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
    /// Tools this key may use (#52). Omit to allow every tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
}

impl From<ApiKeyEntryConfig> for crate::transport::http::ApiKeyEntry {
    fn from(entry: ApiKeyEntryConfig) -> Self {
        Self {
            key: entry.key,
            name: entry.name,
            user_id: entry.user_id,
            tools: entry.tools,
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
    /// OSの資格情報ストアのエントリーの名前（#533）。書くと、ユーザー名と
    /// アプリケーションパスワードをそこから読む（`mcp-rs wordpress-keyring
    /// set <名前>`で登録する）。環境変数に頼らないので、Claude Desktop・
    /// Claude Code・ターミナルのどれから起動しても同じに読める。
    /// `username`・`password`・`encrypted_credentials`とは一緒に書けない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyring: Option<String>,
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
            .field("keyring", &self.keyring)
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
            keyring: None,
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

    /// 資格情報を、置き場所（OSの資格情報ストア・`encrypted_credentials`・
    /// 平文）から読んで、`username`・`password`に入れる（#533）。設定の
    /// 読み込みで、`${VAR}`の展開の後に1回呼ぶ
    pub fn resolve_credentials(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.resolve_credentials_with(&crate::security::keyring_store::OsKeyring)
    }

    /// `resolve_credentials`の本体。`store`は、`keyring`のときに読む先
    pub(crate) fn resolve_credentials_with(
        &mut self,
        store: &dyn crate::security::keyring_store::SecretStore,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(name) = self.keyring.clone() else {
            return Ok(self.resolve_encrypted_credentials()?);
        };
        // 使わない設定（enabled = false）では、資格情報ストアに触れない
        // （macOSでは、読むときに許可を求めるダイアログが出ることがある）
        if !self.enabled.unwrap_or(false) {
            return Ok(());
        }
        // どれを使うかが曖昧になるので、ほかの置き場所と一緒には書けない
        let mut others = Vec::new();
        if !self.username.is_empty() {
            others.push("username");
        }
        if !self.password.is_empty() {
            others.push("password");
        }
        if self.encrypted_credentials.is_some() {
            others.push("encrypted_credentials");
        }
        if !others.is_empty() {
            return Err(format!(
                "handlers.wordpress: keyring cannot be combined with {}. Remove {} from the config file (the keyring entry holds the username and password)",
                others.join(", "),
                if others.len() == 1 { "it" } else { "them" },
            )
            .into());
        }
        let credentials = crate::security::keyring_store::load_wordpress_credentials(store, &name)
            .map_err(|e| format!("handlers.wordpress.keyring: {e}"))?;
        // 資格情報を保存したサイトと、設定の`url`が同じときだけ使う（#541）。
        // 別の人が書いた設定ファイルが、この項目の名前と別のサイトの`url`を
        // 書いて、パスワードを受け取れないようにする
        crate::security::keyring_store::check_site(&name, &credentials, &self.url)
            .map_err(|e| format!("handlers.wordpress.keyring: {e}"))?;
        self.username = credentials.username;
        self.password = credentials.application_password.to_string();
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
            audit: None,
            database: None,
            config_file: None,
        }
    }
}

/// 先頭の`~/`をホームフォルダに置き換える（#375）。`~`で始まらない
/// パスはそのまま返す。ホームフォルダが分からない場合は`None`。
/// `--config`が無いときに読む設定ファイル（#541）
const USER_CONFIG_PATH: &str = "~/.config/mcp-rs/config.toml";

/// v0.23.0まで、`--config`が無いときに今のフォルダから読んでいた名前（#541）。
/// 今は読まない。見つけたら、`--config`の付け方を案内する
const FORMER_CWD_CONFIG_NAMES: [&str; 3] = ["mcp-config.toml", "config.toml", "config/mcp.toml"];

/// `dir`（今のフォルダ）にある、以前の名前の設定ファイル
fn former_cwd_config_files(dir: &std::path::Path) -> Vec<String> {
    FORMER_CWD_CONFIG_NAMES
        .iter()
        .filter(|name| dir.join(name).is_file())
        .map(|name| name.to_string())
        .collect()
}

/// 設定ファイルが見つからないときのエラーの文（#541）。`former`は、今の
/// フォルダにあったが読まなかったファイル。案内するパスは絶対パスにする
/// （相対パスの`--config`は、起動したフォルダから読むので、同じ問題が残る。
/// PR #551のレビュー）
fn no_config_file_message(home: Option<&str>, former: &[String]) -> String {
    let user_config =
        expand_home_dir(USER_CONFIG_PATH, home).unwrap_or_else(|| USER_CONFIG_PATH.to_string());
    let mut message =
        format!("no config file: pass one with --config <absolute path>, or create {user_config}");
    if let Some(first) = former.first() {
        let absolute = std::path::absolute(first)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| first.clone());
        message.push_str(&format!(
            ". {} in the current folder is not read without --config (v0.24.0 and later), so that a config file in a folder you open cannot run commands or send your credentials. If it is yours, start mcp-rs with --config {absolute} (an absolute path: a relative one is read from whatever folder mcp-rs starts in)",
            former.join(", ")
        ));
    }
    message
}

/// `--config`のパスの先頭の`~/`（Windowsでは`~\`も）を、ホームフォルダに
/// 展開する（PR #551のレビュー）。シェルを通さないクライアントの設定（JSONの
/// `args`等）では、`~`は展開されずにそのまま渡ってくる。展開しないと、
/// 起動したフォルダの中の`~`という名前のフォルダを探してしまう。ホーム
/// フォルダが分からなければ、そのまま返す（相対パスの警告が出る）
pub fn expand_config_path(path: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok();
    expand_config_path_with(path, home.as_deref())
}

fn expand_config_path_with(path: &str, home: Option<&str>) -> String {
    let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"));
    match rest {
        Some(rest) => expand_home_dir(&format!("~/{}", rest.replace('\\', "/")), home)
            .unwrap_or_else(|| path.to_string()),
        None => path.to_string(),
    }
}

/// `--config`（`expand_config_path`で展開した後）が相対パスなら、警告の
/// 文を返す（PR #551のレビュー）。相対パスは、mcp-rsを起動したフォルダから
/// 読む。Claude Code等は開いたフォルダで起動するので、そのフォルダの同じ
/// 名前のファイルを読んでしまう
pub fn relative_config_path_warning(path: &str) -> Option<String> {
    if std::path::Path::new(path).is_absolute() {
        return None;
    }
    let absolute = std::path::absolute(path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string());
    Some(format!(
        "--config {path} is a relative path, so mcp-rs reads it from the folder it starts in (now {absolute}). Clients such as Claude Code start mcp-rs in the folder you open, where a file with the same name could run commands or send your credentials. Register mcp-rs with an absolute path"
    ))
}

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

/// 文字列の中の`${VAR}`の変数名（`expand_env_vars`と同じ読み方）。
/// `}`が無いところから後は読まない（展開の時にエラーになる）
fn referenced_env_vars(input: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        names.push(&after[..end]);
        rest = &after[end + 1..];
    }
    names
}

/// `--config`無しで起動したときの、`WORDPRESS_URL`・`WORDPRESS_USERNAME`・
/// `WORDPRESS_PASSWORD`による上書き（後方互換性）。`WORDPRESS_URL`がある
/// ときだけ働き、`[handlers.wordpress]`が無ければ環境変数だけで作る。
///
/// `keyring`を書いた設定は、URLも資格情報も上書きしない（#533）。資格情報
/// ストアのエントリーはURLと結び付いていないので、残っていた
/// `WORDPRESS_URL`が別のサイトを指していると、そのサイトへパスワードを
/// 送ってしまうため。`env`は環境変数を読む関数（テストで差し替える）
fn apply_wordpress_env_overrides(
    wordpress: &mut Option<WordPressConfig>,
    env: impl Fn(&str) -> Option<String>,
) {
    let Some(wp_url) = env("WORDPRESS_URL") else {
        return;
    };
    match wordpress {
        None => {
            *wordpress = Some(WordPressConfig {
                url: wp_url,
                username: env("WORDPRESS_USERNAME").unwrap_or_default(),
                password: env("WORDPRESS_PASSWORD").unwrap_or_default(),
                enabled: Some(true),
                timeout_seconds: Some(30),
                rate_limit: Some(RateLimitConfig::default()),
                encrypted_credentials: None, // 環境変数では平文使用
                keyring: None,
                upload_allowed_dirs: Vec::new(),
            });
        }
        Some(wp_config) if wp_config.keyring.is_some() => {}
        Some(wp_config) => {
            wp_config.url = wp_url;
            if let Some(username) = env("WORDPRESS_USERNAME") {
                wp_config.username = username;
            }
            if let Some(password) = env("WORDPRESS_PASSWORD") {
                wp_config.password = password;
            }
        }
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

/// 設定の値の`${VAR}`を展開できなかった理由（#454）。展開しようとした
/// 値（パスワード等）は含めない
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvExpansionError {
    /// 参照した環境変数が無い（またはUnicodeとして読めない）
    Missing(String),
    /// `${`に対応する`}`が無い、または変数名が空
    Malformed,
    /// 参照した環境変数はあるが、値が空（APIキーで使う、#534）
    Empty(String),
}

impl fmt::Display for EnvExpansionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(name) => write!(f, "environment variable '{}' is not set", name),
            Self::Empty(name) => write!(f, "environment variable '{}' is empty", name),
            Self::Malformed => write!(
                f,
                "'${{' has no matching '}}' or an empty variable name (write ${{VAR_NAME}})"
            ),
        }
    }
}

impl std::error::Error for EnvExpansionError {}

/// WordPressの設定の`${VAR}`を展開できなかった（#454）。どの項目か
/// （`url`・`username`・`password`）と理由を持ち、値は含めない
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEnvError {
    /// 項目（例: `handlers.wordpress.password`）
    pub field: String,
    pub reason: EnvExpansionError,
}

impl fmt::Display for ConfigEnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl std::error::Error for ConfigEnvError {}

impl McpConfig {
    /// 文字列内の`${VAR_NAME}`を、環境変数の値で置き換える
    ///
    /// 参照した環境変数が無い場合と、`${`に対応する`}`が無い場合は、エラー
    /// にする（#454。以前は`[ERROR:VAR_NAME]`等の文字列に置き換えて続けて
    /// いたため、パスワードがその文字列のまま使われた）。置き換えた値の中の
    /// `${...}`は、もう一度は展開しない（パスワードに`${`が含まれていても
    /// そのまま使う）。値はログに出さない
    ///
    /// # 例
    /// ```
    /// # use std::env;
    /// # use mcp_rs::config::{EnvExpansionError, McpConfig};
    /// env::set_var("TEST_VAR", "value123");
    /// assert_eq!(
    ///     McpConfig::expand_env_vars("url = ${TEST_VAR}"),
    ///     Ok("url = value123".to_string())
    /// );
    /// assert_eq!(
    ///     McpConfig::expand_env_vars("${NOT_SET_ANYWHERE_454}"),
    ///     Err(EnvExpansionError::Missing("NOT_SET_ANYWHERE_454".to_string()))
    /// );
    /// ```
    pub fn expand_env_vars(input: &str) -> Result<String, EnvExpansionError> {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let end = after.find('}').ok_or(EnvExpansionError::Malformed)?;
            let name = &after[..end];
            if name.is_empty() {
                return Err(EnvExpansionError::Malformed);
            }
            let value =
                std::env::var(name).map_err(|_| EnvExpansionError::Missing(name.to_string()))?;
            // 値（パスワード等）はログに出さない（#454）
            debug!("Expanded environment variable '{}' in the config", name);
            out.push_str(&value);
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        Ok(out)
    }

    /// WordPressConfig の`url`・`username`・`password`の`${VAR}`を展開する。
    /// 展開できなければ、どの項目のどの変数かを返す（#454）。WordPressが
    /// 有効（`enabled = true`）でなければ、展開せずにそのままにする
    /// （使わない設定の環境変数が無いだけで、起動を止めないため）
    pub fn expand_wordpress_config(config: &mut WordPressConfig) -> Result<(), ConfigEnvError> {
        if !config.enabled.unwrap_or(false) {
            return Ok(());
        }
        for (field, value) in [
            ("url", &mut config.url),
            ("username", &mut config.username),
            ("password", &mut config.password),
        ] {
            *value = Self::expand_env_vars(value).map_err(|reason| ConfigEnvError {
                field: format!("handlers.wordpress.{}", field),
                reason,
            })?;
        }
        Ok(())
    }

    /// HTTPモードのAPIキー（`[[transport.http.api_keys]]`の`key`）の`${VAR}`
    /// を展開する（#534）。キーを設定ファイルに平文で書かずに済むように
    /// する。展開できなければ、どのエントリーのどの変数かを返し、起動しない
    /// （WordPressと同じ、#454）。キーを使わない設定（STDIOモード、または
    /// `auth_enabled`が`true`でない）なら展開しない
    pub fn expand_http_api_keys(&mut self) -> Result<(), ConfigEnvError> {
        if self.transport.transport_type.as_deref() != Some("http") {
            return Ok(());
        }
        let Some(http) = self.transport.http.as_mut() else {
            return Ok(());
        };
        if !http.auth_enabled.unwrap_or(false) {
            return Ok(());
        }
        for (index, entry) in http.api_keys.iter_mut().flatten().enumerate() {
            let field = format!(
                "transport.http.api_keys[{}].key (name '{}')",
                index, entry.name
            );
            // 空の変数は、キーを空や短いものにしてしまう。起動時の検査
            // （空のキー）では、どの変数が空かが分からないので、ここで止める
            if let Some(name) = referenced_env_vars(&entry.key)
                .into_iter()
                .find(|name| std::env::var(name).is_ok_and(|v| v.is_empty()))
            {
                return Err(ConfigEnvError {
                    field,
                    reason: EnvExpansionError::Empty(name.to_string()),
                });
            }
            entry.key = Self::expand_env_vars(&entry.key)
                .map_err(|reason| ConfigEnvError { field, reason })?;
        }
        Ok(())
    }

    /// `[database]`の`host`・`database`・`username`・`password`の`${VAR}`を
    /// 展開する（#453）。使わない設定（`enabled = false`）なら展開しない。
    /// 設定の読み込みでは展開せず、ハンドラーを登録するときに展開する（環境
    /// 変数が無くても、データベースだけを飛ばし、mcp-rsは起動するため）
    pub fn expand_database_config(config: &mut DatabaseSettings) -> Result<(), ConfigEnvError> {
        if !config.is_enabled() {
            return Ok(());
        }
        let expand = |field: &str, value: &mut String| -> Result<(), ConfigEnvError> {
            *value = Self::expand_env_vars(value).map_err(|reason| ConfigEnvError {
                field: format!("database.{}", field),
                reason,
            })?;
            Ok(())
        };
        if let Some(host) = config.host.as_mut() {
            expand("host", host)?;
        }
        expand("database", &mut config.database)?;
        expand("username", &mut config.username)?;
        if let Some(password) = config.password.as_mut() {
            expand("password", password)?;
        }
        Ok(())
    }

    /// `[audit]`から、監査ログの設定を作る（#416）。`file_path`が無ければ、
    /// 通常のログと同じユーザー領域の`audit\audit.log`
    /// （`logging::user_audit_dir`、#474）。相対パスの`file_path`は、設定
    /// ファイルのフォルダを基準にする。書き込み先が決まらない場合は、
    /// ファイルに書かない設定になる。
    pub fn audit_config(&self) -> crate::security::AuditConfig {
        self.audit_config_with_default_dir(crate::logging::user_audit_dir())
    }

    /// `file_path`を書いていないときに、以前の既定の場所（設定ファイルの
    /// フォルダの`audit.log`、#416）に監査ログが残っていれば、その場所
    /// （#474）。起動時に案内するため。新しい既定の場所と同じなら`None`。
    pub fn legacy_audit_log_path(&self) -> Option<PathBuf> {
        let settings = self.audit.clone().unwrap_or_default();
        if settings.enabled == Some(false)
            || settings
                .file_path
                .as_deref()
                .is_some_and(|f| !f.trim().is_empty())
        {
            return None;
        }
        let legacy = self
            .config_file
            .as_deref()
            .and_then(std::path::Path::parent)?
            .join("audit.log");
        let current = self.audit_config().log_file_path.map(PathBuf::from);
        (legacy.is_file() && current.as_deref() != Some(legacy.as_path())).then_some(legacy)
    }

    /// `audit_config`の本体。`default_dir`は`file_path`が無いときのフォルダ。
    /// それも無ければ、以前のとおり設定ファイルのフォルダ。
    fn audit_config_with_default_dir(
        &self,
        default_dir: Option<PathBuf>,
    ) -> crate::security::AuditConfig {
        let settings = self.audit.clone().unwrap_or_default();
        let mut audit = crate::security::AuditConfig::default();
        if settings.enabled == Some(false) {
            return audit;
        }
        let base = self
            .config_file
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf);
        let path = match settings.file_path.as_deref().map(str::trim) {
            Some(file) if !file.is_empty() => {
                let file = PathBuf::from(file);
                if file.is_absolute() {
                    Some(file)
                } else {
                    base.map(|base| base.join(file))
                }
            }
            _ => default_dir.or(base).map(|dir| dir.join("audit.log")),
        };
        let Some(path) = path else {
            return audit;
        };
        audit.enable_file_output = true;
        audit.log_file_path = Some(path.to_string_lossy().into_owned());
        match settings.max_file_size_mb {
            Some(0) => audit.rotation_enabled = false,
            Some(mb) => audit.rotation_size = mb.saturating_mul(1024 * 1024),
            None => {}
        }
        if let Some(max_files) = settings.max_files {
            audit.max_files = max_files;
        }
        audit
    }

    /// 設定ファイルから読み込み、環境変数で上書き
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let mut settings = config::Config::builder();

        // デフォルト値を設定
        let default_config = McpConfig::default();
        settings = settings.add_source(config::Config::try_from(&default_config)?);

        // 設定ファイルは、ユーザーのフォルダの1か所だけから読む（#541）。
        // 以前は今のフォルダの`mcp-config.toml`等も読んでいたので、
        // プロジェクトのフォルダで起動するクライアント（Claude Code等）では、
        // 開いたリポジトリの設定ファイルで、任意のコマンド（プラグイン）を
        // 実行させたり、資格情報を別のサイトへ送らせたりできた
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok();

        let mut config_file_found = false;
        let mut loaded_config_file = None;
        // `~`はPathが展開しないため、自前でホームフォルダに置き換える。
        // ホームフォルダが分からない場合は、探さない
        if let Some(path) = expand_home_dir(USER_CONFIG_PATH, home.as_deref()) {
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
                loaded_config_file = std::path::absolute(&path).ok();
            }
        }

        // 設定ファイルが見つからない場合はエラーを返す。今のフォルダに以前の
        // 名前の設定ファイルがあれば、読まずに、`--config`の付け方を案内する
        if !config_file_found && !Self::has_env_config() {
            return Err(no_config_file_message(
                home.as_deref(),
                &former_cwd_config_files(std::path::Path::new(".")),
            )
            .into());
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
            Self::expand_wordpress_config(wp_config).map_err(|e| e.to_string())?;
        }
        // HTTPモードのAPIキーの`${VAR}`（#534）
        final_config
            .expand_http_api_keys()
            .map_err(|e| e.to_string())?;

        // WordPress設定の環境変数上書き
        apply_wordpress_env_overrides(&mut final_config.handlers.wordpress, |name| {
            std::env::var(name).ok()
        });

        // keyring・encrypted_credentialsの解決は上記の平文環境変数上書き
        // より後に行う。そうしないと、暗号化認証情報へ移行した後も環境に
        // 残った古いWORDPRESS_PASSWORD等が、復号済みの値を無言で上書きして
        // しまう
        if let Some(ref mut wp_config) = final_config.handlers.wordpress {
            wp_config.resolve_credentials()?;
        }

        // サーバー設定の環境変数上書き
        if let Ok(bind_addr) = std::env::var("BIND_ADDR") {
            final_config.server.bind_addr = Some(bind_addr);
        }

        if std::env::var("MCP_STDIO").is_ok() {
            final_config.server.stdio = Some(true);
        }

        final_config.config_file = loaded_config_file;
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
                    oauth: None,
                    allowed_origins: None,
                    max_request_size: None,
                    request_timeout_seconds: None,
                    max_connections: None,
                    websocket_idle_timeout_seconds: None,
                    allow_unauthenticated_external: None,
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
                    keyring: None,
                    upload_allowed_dirs: Vec::new(),
                }),
            },
            plugins: Vec::new(),
            audit: None,
            database: None,
            config_file: None,
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
#    mcp-rs は .env ファイルを読みません。Claude Desktop は OS の環境変数を
#    渡さないので、claude_desktop_config.json の env に書きます
#
# 2. または OS の資格情報ストアに登録 (Claude Desktop・Claude Code・
#    ターミナルのどれからでも同じに読める):
#    mcp-rs wordpress-keyring set your-site
#    [handlers.wordpress] には username・password の代わりに
#    keyring = "your-site" と書きます。登録のときに聞かれるサイトの
#    URL は、url と同じにします (違うサイトには資格情報を使いません)
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
#    mcp-rs は .env ファイルを読みません。Claude Desktop は OS の環境変数を
#    渡さないので、claude_desktop_config.json の env に書きます
#
# 2. または OS の資格情報ストアに登録 (Claude Desktop・Claude Code・
#    ターミナルのどれからでも同じに読める):
#    mcp-rs wordpress-keyring set your-site
#    [handlers.wordpress] には username・password の代わりに
#    keyring = "your-site" と書きます。登録のときに聞かれるサイトの
#    URL は、url と同じにします (違うサイトには資格情報を使いません)
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
                max_request_size: http.max_request_size.filter(|n| *n > 0).unwrap_or(1048576),
                timeout_ms: http
                    .request_timeout_seconds
                    .filter(|n| *n > 0)
                    .unwrap_or(30)
                    .saturating_mul(1000),
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
                oauth: http.oauth.clone(),
                allowed_origins: http.allowed_origins.clone().unwrap_or_default(),
                max_connections: http.max_connections.filter(|n| *n > 0).unwrap_or(1024),
                websocket_idle_timeout_secs: http.websocket_idle_timeout_seconds.unwrap_or(300),
                allow_unauthenticated_external: http
                    .allow_unauthenticated_external
                    .unwrap_or(false),
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
        assert!(legacy_plugins_conflict_hint(
            "[[plugins]]\nname = \"a\"\n[plugins.kubernetes]\nimage = \"i\"\n"
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

[[plugins]]
name = "k8s"
command = "python"

[plugins.kubernetes]
image = "example/server:1.0"
namespace = "mcp-plugins"
start_timeout_seconds = 300
"#;

    fn assert_container_plugins(config: &McpConfig) {
        assert_eq!(config.plugins.len(), 3);
        let k8s = config.plugins[2].kubernetes.as_ref().unwrap();
        assert_eq!(k8s.image, "example/server:1.0");
        assert_eq!(k8s.namespace.as_deref(), Some("mcp-plugins"));
        assert_eq!(k8s.start_timeout_seconds, Some(300));
        assert!(config.plugins[2].container.is_none());
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

    fn config_with_audit(config_file: Option<&str>, audit: Option<AuditLogSettings>) -> McpConfig {
        McpConfig {
            audit,
            config_file: config_file.map(PathBuf::from),
            ..McpConfig::default()
        }
    }

    #[test]
    fn test_audit_config_defaults_to_user_audit_dir() {
        let base = std::env::temp_dir().join("mcp-rs-config");
        let config_file = base.join("mcp-config.toml");
        let user_dir = std::env::temp_dir().join("mcp-rs-home").join("audit");

        // 設定ファイルがあっても無くても、ユーザー領域のaudit\audit.log（#474）
        for config_file in [Some(config_file.to_str().unwrap()), None] {
            let audit = config_with_audit(config_file, None)
                .audit_config_with_default_dir(Some(user_dir.clone()));
            assert!(audit.enable_file_output);
            assert_eq!(
                audit.log_file_path.as_deref(),
                Some(user_dir.join("audit.log").to_str().unwrap())
            );
            assert!(audit.rotation_enabled);
            assert_eq!(audit.rotation_size, 100 * 1024 * 1024);
            assert_eq!(audit.max_files, 5);
        }

        // ユーザー領域が分からなければ、以前のとおり設定ファイルのフォルダ
        let audit = config_with_audit(Some(config_file.to_str().unwrap()), None)
            .audit_config_with_default_dir(None);
        assert_eq!(
            audit.log_file_path.as_deref(),
            Some(base.join("audit.log").to_str().unwrap())
        );
    }

    #[test]
    fn test_audit_config_uses_logging_user_audit_dir() {
        let config = config_with_audit(None, None);
        assert_eq!(
            config.audit_config().log_file_path.map(PathBuf::from),
            crate::logging::user_audit_dir().map(|dir| dir.join("audit.log"))
        );
    }

    #[test]
    fn test_legacy_audit_log_path() {
        let base = std::env::temp_dir().join(format!("mcp-rs-legacy-audit-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let config_file = base.join("mcp-config.toml");
        let config_file = config_file.to_str().unwrap();

        // 以前の既定の場所にファイルが無ければ、何も案内しない
        assert_eq!(
            config_with_audit(Some(config_file), None).legacy_audit_log_path(),
            None
        );

        std::fs::write(base.join("audit.log"), "{}\n").unwrap();
        let legacy = config_with_audit(Some(config_file), None).legacy_audit_log_path();
        // 新しい既定の場所が分かる環境なら、残っているファイルを案内する
        if crate::logging::user_audit_dir().is_some() {
            assert_eq!(legacy, Some(base.join("audit.log")));
        }

        // file_pathを書いている・ファイルに書かない設定なら、案内しない
        for settings in [
            AuditLogSettings {
                file_path: Some("audit.log".to_string()),
                ..Default::default()
            },
            AuditLogSettings {
                enabled: Some(false),
                ..Default::default()
            },
        ] {
            assert_eq!(
                config_with_audit(Some(config_file), Some(settings)).legacy_audit_log_path(),
                None
            );
        }
        // 設定ファイルが無ければ、以前の既定の場所も無い
        assert_eq!(config_with_audit(None, None).legacy_audit_log_path(), None);

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_audit_config_resolves_settings() {
        let base = std::env::temp_dir().join("mcp-rs-config");
        let config_file = base.join("mcp-config.toml");
        let config_file = config_file.to_str().unwrap();

        // 相対パスは設定ファイルのフォルダを基準にする
        let audit = config_with_audit(
            Some(config_file),
            Some(AuditLogSettings {
                file_path: Some("logs/audit.jsonl".to_string()),
                max_file_size_mb: Some(10),
                max_files: Some(2),
                ..Default::default()
            }),
        )
        .audit_config();
        assert_eq!(
            audit.log_file_path.as_deref(),
            Some(base.join("logs/audit.jsonl").to_str().unwrap())
        );
        assert_eq!(audit.rotation_size, 10 * 1024 * 1024);
        assert_eq!(audit.max_files, 2);

        // 絶対パスはそのまま。設定ファイルが無くても使える
        let absolute = base.join("abs.log");
        let audit = config_with_audit(
            None,
            Some(AuditLogSettings {
                file_path: Some(absolute.to_str().unwrap().to_string()),
                max_file_size_mb: Some(0),
                ..Default::default()
            }),
        )
        .audit_config();
        assert_eq!(audit.log_file_path.as_deref(), absolute.to_str());
        assert!(!audit.rotation_enabled);

        // enabled = falseならファイルに書かない
        let audit = config_with_audit(
            Some(config_file),
            Some(AuditLogSettings {
                enabled: Some(false),
                ..Default::default()
            }),
        )
        .audit_config();
        assert!(!audit.enable_file_output);

        // 設定ファイルもユーザー領域も無く、絶対パスの指定も無ければ、
        // 書き込み先が決まらない
        let audit = config_with_audit(None, None).audit_config_with_default_dir(None);
        assert!(!audit.enable_file_output);
        assert_eq!(audit.log_file_path, None);

        // 相対パスのfile_pathは、ユーザー領域ではなく設定ファイルのフォルダが基準
        let audit = config_with_audit(
            Some(config_file),
            Some(AuditLogSettings {
                file_path: Some("audit.log".to_string()),
                ..Default::default()
            }),
        )
        .audit_config_with_default_dir(Some(std::env::temp_dir().join("elsewhere")));
        assert_eq!(
            audit.log_file_path.as_deref(),
            Some(base.join("audit.log").to_str().unwrap())
        );
    }

    #[test]
    fn test_database_section_is_read_from_toml() {
        let config: McpConfig = toml::from_str(
            "[transport]\n[server]\n[handlers]\n[database]\nengine = \"postgresql\"\ndatabase = \"app\"\nusername = \"reader\"\npassword = \"s3cret\"\n",
        )
        .unwrap();
        let db = config.database.unwrap();
        assert!(db.is_enabled());
        assert_eq!(db.host(), "localhost");
        assert_eq!(db.port(), 5432);
        assert!(!db.allow_write());
        assert_eq!(db.max_rows(), DatabaseSettings::DEFAULT_MAX_ROWS);
        assert_eq!(
            db.query_timeout_seconds(),
            DatabaseSettings::DEFAULT_QUERY_TIMEOUT_SECONDS
        );
        assert!(db.validate().is_ok());
        // パスワードは説明にもDebugにも出さない
        assert_eq!(
            db.describe(),
            "postgresql reader@localhost:5432/app (read-only)"
        );
        assert!(!format!("{:?}", db).contains("s3cret"));
    }

    /// 配っている設定の例（`configs/database/postgres.toml`）が、そのまま
    /// 読めて、検証も通る（以前の例は`engine`が無く、起動しなかった）
    #[test]
    fn test_database_example_config_is_valid() {
        // Dockerfile.test等、`configs`を含まないビルドでは確かめない
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("configs/database/postgres.toml");
        let Ok(example) = std::fs::read_to_string(&path) else {
            return;
        };
        let config: McpConfig =
            toml::from_str(&format!("[transport]\n[server]\n[handlers]\n{example}")).unwrap();
        let db = config.database.unwrap();
        assert!(db.validate().is_ok());
        assert!(!db.allow_write());
        assert_eq!(db.password.as_deref(), Some("${MCP_DB_PASSWORD}"));
    }

    /// 結果のバイト数の上限の既定と、CAのファイルの相対パス（#539）
    #[test]
    fn test_database_result_bytes_and_ssl_root_cert_path() {
        let mut db = DatabaseSettings::default();
        assert_eq!(db.max_result_bytes(), 10 * 1024 * 1024);
        db.max_result_bytes = Some(0);
        assert_eq!(db.max_result_bytes(), 10 * 1024 * 1024);
        db.max_result_bytes = Some(5000);
        assert_eq!(db.max_result_bytes(), 5000);

        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        db.ssl_root_cert = Some("certs/ca.pem".to_string());
        db.resolve_paths(Some(base));
        assert_eq!(
            db.ssl_root_cert.as_deref().map(std::path::PathBuf::from),
            Some(base.join("certs/ca.pem"))
        );
        let absolute = base.join("ca.pem").to_string_lossy().into_owned();
        db.ssl_root_cert = Some(absolute.clone());
        db.resolve_paths(Some(std::path::Path::new("elsewhere")));
        assert_eq!(db.ssl_root_cert.as_deref(), Some(absolute.as_str()));
    }

    #[test]
    fn test_database_settings_validate() {
        let valid = DatabaseSettings {
            engine: "postgresql".to_string(),
            database: "app".to_string(),
            username: "reader".to_string(),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
        for (invalid, expected) in [
            (
                DatabaseSettings {
                    engine: "mysql".to_string(),
                    ..valid.clone()
                },
                "not supported",
            ),
            (
                DatabaseSettings {
                    database: " ".to_string(),
                    ..valid.clone()
                },
                "database.database",
            ),
            (
                DatabaseSettings {
                    username: String::new(),
                    ..valid.clone()
                },
                "database.username",
            ),
            (
                DatabaseSettings {
                    ssl_mode: Some("strict".to_string()),
                    ..valid.clone()
                },
                "ssl_mode",
            ),
        ] {
            let err = invalid.validate().unwrap_err();
            assert!(err.contains(expected), "{err}");
        }
        // 0や省略は既定値
        let zero = DatabaseSettings {
            max_rows: Some(0),
            query_timeout_seconds: Some(0),
            max_connections: Some(0),
            ..valid
        };
        assert_eq!(zero.max_rows(), DatabaseSettings::DEFAULT_MAX_ROWS);
        assert_eq!(
            zero.max_connections(),
            DatabaseSettings::DEFAULT_MAX_CONNECTIONS
        );
    }

    #[test]
    fn test_expand_database_config() {
        std::env::set_var("MCP_RS_TEST_DB_PASSWORD_453", "from-env");
        let mut db = DatabaseSettings {
            engine: "postgresql".to_string(),
            database: "app".to_string(),
            username: "reader".to_string(),
            password: Some("${MCP_RS_TEST_DB_PASSWORD_453}".to_string()),
            ..Default::default()
        };
        McpConfig::expand_database_config(&mut db).unwrap();
        assert_eq!(db.password.as_deref(), Some("from-env"));

        // 無い変数は、項目と変数の名前を返す（値は出さない）
        let mut missing = DatabaseSettings {
            password: Some("${MCP_RS_TEST_DB_MISSING_453}".to_string()),
            ..db.clone()
        };
        let err = McpConfig::expand_database_config(&mut missing).unwrap_err();
        assert_eq!(err.field, "database.password");

        // 使わない設定なら、展開しない
        let mut disabled = DatabaseSettings {
            enabled: Some(false),
            ..missing
        };
        assert!(McpConfig::expand_database_config(&mut disabled).is_ok());
    }

    #[test]
    fn test_audit_section_is_read_from_toml() {
        let config: McpConfig = toml::from_str(
            "[transport]\n[server]\n[handlers]\n[audit]\nfile_path = \"C:/logs/mcp-audit.log\"\nmax_files = 3\n",
        )
        .unwrap();
        let audit = config.audit.unwrap();
        assert_eq!(audit.file_path.as_deref(), Some("C:/logs/mcp-audit.log"));
        assert_eq!(audit.max_files, Some(3));
        assert_eq!(audit.enabled, None);
    }

    fn http_config_with_keys(transport_type: &str, auth_enabled: bool) -> McpConfig {
        toml::from_str(&format!(
            r#"
[transport]
type = "{transport_type}"
[transport.http]
auth_enabled = {auth_enabled}

[[transport.http.api_keys]]
key = "plain-key"
name = "plain"
user_id = "u1"

[[transport.http.api_keys]]
key = "${{MCP_RS_TEST_API_KEY_534}}"
name = "from-env"
user_id = "u2"

[server]
[handlers]
"#
        ))
        .unwrap()
    }

    /// APIキーの`${VAR}`を展開する（#534）
    #[test]
    fn test_expand_http_api_keys() {
        std::env::set_var("MCP_RS_TEST_API_KEY_534", "key-from-env");
        let mut config = http_config_with_keys("http", true);
        config.expand_http_api_keys().unwrap();
        let http = config.to_transport_config().http;
        assert_eq!(http.api_keys[0].key, "plain-key");
        assert_eq!(http.api_keys[1].key, "key-from-env");
    }

    /// 変数が無ければ、どのエントリーのどの変数かを返す。値は出さない
    #[test]
    fn test_expand_http_api_keys_reports_missing_variable() {
        let mut config = http_config_with_keys("http", true);
        config
            .transport
            .http
            .as_mut()
            .unwrap()
            .api_keys
            .as_mut()
            .unwrap()[1]
            .key = "prefix-${MCP_RS_TEST_API_KEY_534_UNSET}".to_string();
        let err = config.expand_http_api_keys().unwrap_err().to_string();
        assert_eq!(
            err,
            "transport.http.api_keys[1].key (name 'from-env'): environment variable 'MCP_RS_TEST_API_KEY_534_UNSET' is not set"
        );
        assert!(!err.contains("prefix-"));
    }

    /// 変数はあるが空なら、その変数名を出して止める（起動時の「空のキー」
    /// の検査では、どの変数かが分からないため）
    #[test]
    fn test_expand_http_api_keys_reports_empty_variable() {
        std::env::set_var("MCP_RS_TEST_API_KEY_534_EMPTY", "");
        let mut config = http_config_with_keys("http", true);
        config
            .transport
            .http
            .as_mut()
            .unwrap()
            .api_keys
            .as_mut()
            .unwrap()[1]
            .key = "prefix-${MCP_RS_TEST_API_KEY_534_EMPTY}".to_string();
        assert_eq!(
            config.expand_http_api_keys().unwrap_err().to_string(),
            "transport.http.api_keys[1].key (name 'from-env'): environment variable 'MCP_RS_TEST_API_KEY_534_EMPTY' is empty"
        );
    }

    #[test]
    fn test_referenced_env_vars() {
        assert_eq!(referenced_env_vars("${A}-x-${B_2}"), vec!["A", "B_2"]);
        assert_eq!(referenced_env_vars("plain"), Vec::<&str>::new());
        assert_eq!(referenced_env_vars("${A}${unclosed"), vec!["A"]);
    }

    /// キーを使わない設定（STDIO・auth_enabledがfalse）では展開しない
    /// （変数が無くても起動を止めない）
    #[test]
    fn test_expand_http_api_keys_skips_unused_keys() {
        for (transport_type, auth_enabled) in [("stdio", true), ("http", false)] {
            let mut config = http_config_with_keys(transport_type, auth_enabled);
            config
                .transport
                .http
                .as_mut()
                .unwrap()
                .api_keys
                .as_mut()
                .unwrap()[1]
                .key = "${MCP_RS_TEST_API_KEY_534_UNSET}".to_string();
            config.expand_http_api_keys().unwrap();
            assert_eq!(
                config.transport.http.unwrap().api_keys.unwrap()[1].key,
                "${MCP_RS_TEST_API_KEY_534_UNSET}"
            );
        }
    }

    /// APIキーの`tools`と、OAuthの`scope_tools`（#52）を読む
    #[test]
    fn test_tool_permissions_are_read_from_toml() {
        let config: McpConfig = toml::from_str(
            r#"
[transport]
type = "http"
[transport.http]
auth_enabled = true

[[transport.http.api_keys]]
key = "k1"
name = "reader"
user_id = "u1"
tools = ["@read_only", "fs__*"]

[[transport.http.api_keys]]
key = "k2"
name = "admin"
user_id = "u2"

[transport.http.oauth]
issuer = "https://auth.example.com/realms/mcp"
resource = "https://mcp.example.com/mcp"

[transport.http.oauth.scope_tools]
"mcp:read" = ["@read_only"]
"mcp:write" = ["*"]

[server]
[handlers]
"#,
        )
        .unwrap();
        let http = config.to_transport_config().http;
        assert_eq!(
            http.api_keys[0].tools,
            Some(vec!["@read_only".to_string(), "fs__*".to_string()])
        );
        assert_eq!(http.api_keys[1].tools, None);
        let oauth = http.oauth.unwrap();
        assert_eq!(
            oauth.scope_tools["mcp:read"],
            vec!["@read_only".to_string()]
        );
        assert_eq!(oauth.scope_tools["mcp:write"], vec!["*".to_string()]);
        assert!(oauth.validate().is_ok());
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
        assert_eq!(result, Ok("test_value".to_string()));
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
        assert_eq!(result, Ok("value1 and value2".to_string()));
        unsafe {
            std::env::remove_var("VAR1");
            std::env::remove_var("VAR2");
        }
    }

    #[test]
    fn test_expand_env_vars_missing() {
        let result = McpConfig::expand_env_vars("a ${NONEXISTENT_VAR} b");
        assert_eq!(
            result,
            Err(EnvExpansionError::Missing("NONEXISTENT_VAR".to_string()))
        );
    }

    #[test]
    fn test_expand_env_vars_does_not_expand_values_again() {
        // 置き換えた値の中の${...}は展開しない（パスワードに${が含まれて
        // いても、そのまま使う。#454）
        unsafe {
            std::env::set_var("OUTER_454", "pa${ss}word");
        }
        let result = McpConfig::expand_env_vars("${OUTER_454}");
        assert_eq!(result, Ok("pa${ss}word".to_string()));
        unsafe {
            std::env::remove_var("OUTER_454");
        }
    }

    #[test]
    fn test_expand_env_vars_multibyte_value() {
        // 以前は値の先頭20バイトをログに出しており、20バイト目が文字の
        // 途中だとpanicした（#454）
        unsafe {
            std::env::set_var("MULTIBYTE_454", "パスワード日本語のテスト値です");
        }
        let result = McpConfig::expand_env_vars("${MULTIBYTE_454}");
        assert_eq!(result, Ok("パスワード日本語のテスト値です".to_string()));
        unsafe {
            std::env::remove_var("MULTIBYTE_454");
        }
    }

    #[test]
    fn test_expand_env_vars_malformed() {
        assert_eq!(
            McpConfig::expand_env_vars("${INCOMPLETE"),
            Err(EnvExpansionError::Malformed)
        );
        assert_eq!(
            McpConfig::expand_env_vars("${}"),
            Err(EnvExpansionError::Malformed)
        );
        // $だけ、{だけは展開の対象ではない
        assert_eq!(
            McpConfig::expand_env_vars("a$b {c}"),
            Ok("a$b {c}".to_string())
        );
    }

    #[test]
    fn test_expand_wordpress_config_reports_field_without_value() {
        let mut wp: WordPressConfig = toml::from_str(
            r#"
            url = "https://example.com"
            username = "admin"
            password = "${WP_PASSWORD_NOT_SET_454}"
            enabled = true
            "#,
        )
        .unwrap();
        let err = McpConfig::expand_wordpress_config(&mut wp).unwrap_err();
        assert_eq!(err.field, "handlers.wordpress.password");
        assert_eq!(
            err.reason,
            EnvExpansionError::Missing("WP_PASSWORD_NOT_SET_454".to_string())
        );
        assert_eq!(
            err.to_string(),
            "handlers.wordpress.password: environment variable 'WP_PASSWORD_NOT_SET_454' is not set"
        );
    }

    #[test]
    fn test_expand_wordpress_config_skips_disabled() {
        // 使わない（enabledでない）WordPressの設定は、環境変数が無くても
        // 起動を止めない
        let mut wp: WordPressConfig = toml::from_str(
            r#"
            url = "${WP_URL_NOT_SET_454}"
            password = "${WP_PASSWORD_NOT_SET_454}"
            enabled = false
            "#,
        )
        .unwrap();
        assert!(McpConfig::expand_wordpress_config(&mut wp).is_ok());
        assert_eq!(wp.password, "${WP_PASSWORD_NOT_SET_454}");
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
        assert_eq!(result, Ok(String::new()));
    }

    #[test]
    fn test_expand_env_vars_no_variables() {
        let input = "plain text without variables";
        let result = McpConfig::expand_env_vars(input);
        assert_eq!(result, Ok(input.to_string()));
    }

    #[test]
    fn test_expand_env_vars_special_chars() {
        unsafe {
            std::env::set_var("SPECIAL", "test@example.com:8080/path?query=1");
        }
        let result = McpConfig::expand_env_vars("url=${SPECIAL}");
        assert_eq!(
            result,
            Ok("url=test@example.com:8080/path?query=1".to_string())
        );
        unsafe {
            std::env::remove_var("SPECIAL");
        }
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[test]
    fn test_expand_env_vars_default_syntax_is_not_supported() {
        // ${VAR:-default}の書き方には対応していない（変数名として扱う）
        assert_eq!(
            McpConfig::expand_env_vars("${MISSING:-default_value}"),
            Err(EnvExpansionError::Missing(
                "MISSING:-default_value".to_string()
            ))
        );
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
            keyring: None,
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
            keyring: None,
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
            keyring: None,
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
            keyring: None,
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
            keyring: None,
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
            keyring: None,
            upload_allowed_dirs: Vec::new(),
        };

        config.resolve_encrypted_credentials().unwrap();
    }

    /// `keyring = "<名前>"`の設定（#533）。TOMLから読んで、資格情報ストア
    /// からユーザー名・パスワードを入れる
    fn keyring_config(extra: &str) -> WordPressConfig {
        let toml_str = format!(
            "url = \"https://example.com\"\nenabled = true\nkeyring = \"example.com\"\n{extra}"
        );
        toml::from_str(&toml_str).unwrap()
    }

    fn store_with_example() -> crate::security::keyring_store::MemoryStore {
        use crate::security::keyring_store::{
            store_wordpress_credentials, MemoryStore, WordPressKeyringCredentials,
        };
        let store = MemoryStore::default();
        store_wordpress_credentials(
            &store,
            "example.com",
            &WordPressKeyringCredentials {
                site_url: "https://example.com".to_string(),
                username: "editor".to_string(),
                application_password: zeroize::Zeroizing::new("abcd EFGH 1234".to_string()),
            },
        )
        .unwrap();
        store
    }

    #[test]
    fn test_resolve_credentials_reads_keyring() {
        let store = store_with_example();
        let mut config = keyring_config("");
        assert_eq!(config.keyring.as_deref(), Some("example.com"));
        config.resolve_credentials_with(&store).unwrap();
        assert_eq!(config.username, "editor");
        assert_eq!(config.password, "abcd EFGH 1234");
        // 書き出すとき、keyringは残し、無ければ書かない
        assert!(toml::to_string(&config)
            .unwrap()
            .contains("keyring = \"example.com\""));
        let mut plain = keyring_config("");
        plain.keyring = None;
        assert!(!toml::to_string(&plain).unwrap().contains("keyring"));
    }

    /// 今のフォルダの設定ファイルは読まず、見つけたら`--config`を案内する（#541）
    #[test]
    fn test_former_cwd_config_files_are_only_mentioned() {
        let dir = tempfile::tempdir().unwrap();
        assert!(former_cwd_config_files(dir.path()).is_empty());
        let message = no_config_file_message(Some("/home/u"), &[]);
        assert!(message.contains("--config <absolute path>"), "{message}");
        assert!(message.contains("config.toml"), "{message}");
        assert!(!message.contains("current folder"), "{message}");

        std::fs::write(dir.path().join("config.toml"), "").unwrap();
        std::fs::create_dir(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config").join("mcp.toml"), "").unwrap();
        let former = former_cwd_config_files(dir.path());
        assert_eq!(former, vec!["config.toml", "config/mcp.toml"]);
        let message = no_config_file_message(Some("/home/u"), &former);
        assert!(
            message.contains("is not read without --config"),
            "{message}"
        );
        // 案内は絶対パス（相対パスは、起動したフォルダから読むため）
        let absolute = std::path::absolute("config.toml").unwrap();
        assert!(
            message.contains(&format!("--config {}", absolute.display())),
            "{message}"
        );
        assert!(!message.contains("--config config.toml"), "{message}");
    }

    /// `--config`の`~/`は、ホームフォルダに展開する（PR #551のレビュー）
    #[test]
    fn test_expand_config_path() {
        let home = std::env::temp_dir();
        let home_str = home.display().to_string();
        let expanded = expand_config_path_with("~/.config/mcp-rs/config.toml", Some(&home_str));
        assert_eq!(
            std::path::PathBuf::from(&expanded),
            home.join(".config").join("mcp-rs").join("config.toml")
        );
        assert_eq!(relative_config_path_warning(&expanded), None);
        let expanded = expand_config_path_with("~\\mcp-rs\\config.toml", Some(&home_str));
        assert_eq!(
            std::path::PathBuf::from(&expanded),
            home.join("mcp-rs").join("config.toml")
        );
        // ホームが分からなければそのまま（相対パスとして警告する）
        assert_eq!(
            expand_config_path_with("~/config.toml", None),
            "~/config.toml"
        );
        assert!(relative_config_path_warning("~/config.toml").is_some());
        // `~`で始まるほかの形（`~user/`等）は展開しない
        assert_eq!(
            expand_config_path_with("~user/x.toml", Some(&home_str)),
            "~user/x.toml"
        );
        assert_eq!(
            expand_config_path_with("config.toml", Some(&home_str)),
            "config.toml"
        );
    }

    /// 相対パスの`--config`は警告する（PR #551のレビュー）
    #[test]
    fn test_relative_config_path_warning() {
        let warning = relative_config_path_warning("config.toml").unwrap();
        assert!(warning.contains("relative path"), "{warning}");
        assert!(warning.contains("absolute path"), "{warning}");
        let absolute = std::path::absolute("x.toml").unwrap();
        assert_eq!(
            relative_config_path_warning(&absolute.display().to_string()),
            None
        );
    }

    /// 設定の`url`が、資格情報を保存したサイトと違えば使わない（#541）
    #[test]
    fn test_resolve_credentials_refuses_another_site() {
        let store = store_with_example();
        let mut config: WordPressConfig = toml::from_str(
            "url = \"https://attacker.example\"\nenabled = true\nkeyring = \"example.com\"\n",
        )
        .unwrap();
        let err = config
            .resolve_credentials_with(&store)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("handlers.wordpress.keyring: "), "{err}");
        assert!(err.contains("are for https://example.com"), "{err}");
        assert!(!err.contains("abcd"), "{err}");
        assert!(config.password.is_empty());
    }

    #[test]
    fn test_resolve_credentials_reports_missing_entry() {
        let store = crate::security::keyring_store::MemoryStore::default();
        let err = keyring_config("")
            .resolve_credentials_with(&store)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("handlers.wordpress.keyring: "));
        assert!(err.contains("mcp-rs wordpress-keyring set example.com"));
    }

    #[test]
    fn test_resolve_credentials_rejects_keyring_with_other_sources() {
        let store = store_with_example();
        let err = keyring_config("username = \"u\"\npassword = \"secret-in-file\"")
            .resolve_credentials_with(&store)
            .unwrap_err()
            .to_string();
        assert!(err.contains("keyring cannot be combined with username, password"));
        assert!(!err.contains("secret-in-file"));

        let err = keyring_config(
            "[encrypted_credentials]\nusername = \"u\"\nencrypted_password = \"ZGVhZGJlZWY=\"\nnonce = \"AAAAAAAAAAAAAAAA\"\nsalt = \"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"",
        )
        .resolve_credentials_with(&store)
        .unwrap_err()
        .to_string();
        assert!(err.contains("encrypted_credentials"));
    }

    #[test]
    fn test_resolve_credentials_skips_keyring_when_disabled() {
        // 使わない設定では、資格情報ストアを読まない（エントリーが無くても
        // 起動を止めない）
        let store = crate::security::keyring_store::MemoryStore::default();
        let mut config: WordPressConfig = toml::from_str(
            "url = \"https://example.com\"\nenabled = false\nkeyring = \"example.com\"",
        )
        .unwrap();
        config.resolve_credentials_with(&store).unwrap();
        assert_eq!(config.username, "");
    }

    #[test]
    fn test_resolve_credentials_without_keyring_keeps_plaintext() {
        let store = crate::security::keyring_store::MemoryStore::default();
        let mut config: WordPressConfig = toml::from_str(
            "url = \"https://example.com\"\nenabled = true\nusername = \"u\"\npassword = \"p\"",
        )
        .unwrap();
        config.resolve_credentials_with(&store).unwrap();
        assert_eq!(
            (config.username.as_str(), config.password.as_str()),
            ("u", "p")
        );
    }

    fn env_from<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    const STALE_ENV: &[(&str, &str)] = &[
        ("WORDPRESS_URL", "https://other.example"),
        ("WORDPRESS_USERNAME", "other-user"),
        ("WORDPRESS_PASSWORD", "other-password"),
    ];

    #[test]
    fn test_env_overrides_leave_keyring_config_alone() {
        // 残っていたWORDPRESS_URLで、資格情報ストアのパスワードを別の
        // サイトへ送らない（#533）
        let mut wordpress = Some(keyring_config(""));
        apply_wordpress_env_overrides(&mut wordpress, env_from(STALE_ENV));
        let wp = wordpress.unwrap();
        assert_eq!(wp.url, "https://example.com");
        assert_eq!((wp.username.as_str(), wp.password.as_str()), ("", ""));
    }

    #[test]
    fn test_env_overrides_apply_without_keyring() {
        let mut wordpress = Some(
            toml::from_str::<WordPressConfig>(
                "url = \"https://example.com\"\nenabled = true\nusername = \"u\"\npassword = \"p\"",
            )
            .unwrap(),
        );
        apply_wordpress_env_overrides(&mut wordpress, env_from(STALE_ENV));
        let wp = wordpress.unwrap();
        assert_eq!(wp.url, "https://other.example");
        assert_eq!(
            (wp.username.as_str(), wp.password.as_str()),
            ("other-user", "other-password")
        );

        // [handlers.wordpress]が無ければ、環境変数だけで作る
        let mut wordpress = None;
        apply_wordpress_env_overrides(&mut wordpress, env_from(STALE_ENV));
        assert_eq!(wordpress.unwrap().url, "https://other.example");

        // WORDPRESS_URLが無ければ、何もしない
        let mut wordpress = None;
        apply_wordpress_env_overrides(&mut wordpress, env_from(&[("WORDPRESS_PASSWORD", "x")]));
        assert!(wordpress.is_none());
    }
}
