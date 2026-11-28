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
use std::collections::{BTreeMap, HashMap};
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
    /// 起動はするが使えないもの（名前、理由）。`mcp_rs_status`に出す。
    /// 拡張機能で、サイトがまだ入力されていないとき等（#632）
    #[serde(skip)]
    pub startup_notes: Vec<(String, String)>,
    /// 設定ファイルが無いまま、設定なし（ハンドラー無し）で起動したときの
    /// 案内（#667）。`None`は、設定ファイルか環境変数で設定した
    #[serde(skip)]
    pub missing_config_file: Option<String>,
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

/// データベースの設定（`[database]`、#453）。PostgreSQLと、MySQL・MariaDB（#527）、
/// SQLite（#528）、Redis（#529）。SQLiteは、`database`にファイルのパスを書き、
/// `host`・`port`・`username`・`password`・`keyring`・`ssl_mode`・`ssl_root_cert`は
/// 書かない。Redisは、`database`がデータベースの番号（省略時は0）で、
/// `username`は省略できる（既定のユーザー）。MongoDB（#530）は、`database`が
/// データベースの名前で、`username`は省略できる（認証しない）。
///
/// 既定では読み取りだけ: `execute_query`は読み取り専用のトランザクションで
/// 実行し、書き込みのツール（`execute_command`）は出さない。
/// `host`・`database`・`username`・`password`には`${VAR}`（環境変数）を
/// 書ける。`password`は`Debug`に出さない。
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct DatabaseSettings {
    /// `false`で使わない。省略時は`true`（`[database]`を書けば使う）
    pub enabled: Option<bool>,
    /// エンジン: `"postgresql"`・`"mysql"`・`"mariadb"`（#527）・`"sqlite"`（#528）・
    /// `"redis"`（#529）・`"mongodb"`（#530）
    pub engine: String,
    /// 省略時は`localhost`
    pub host: Option<String>,
    /// 省略時は、PostgreSQLは5432、MySQL・MariaDBは3306
    pub port: Option<u16>,
    /// データベースの名前。SQLiteは、データベースのファイルのパス（#528）。
    /// 相対パスは設定ファイルのフォルダを基準にする。Redisは、データベースの
    /// 番号（#529。省略時は0）
    #[serde(default)]
    pub database: String,
    /// SQLiteでは書かない（#528）。Redisでは省略できる（#529。既定の
    /// ユーザー）。MongoDBでも省略できる（#530。認証せずにつなぐ。#810）。
    /// ほかのエンジンでは必須
    #[serde(default)]
    pub username: String,
    /// 省略時は空（パスワード無し）。`${PGPASSWORD}`のように環境変数で渡せる
    pub password: Option<String>,
    /// OSの資格情報ストアの、パスワードの項目の名前（#590）。`mcp-rs init`が
    /// 保存する。項目は`host`・`port`・`username`と結び付いていて、
    /// `[database]`の値が違えば使わない。`password`とは一緒に書けない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyring: Option<String>,
    /// MongoDBの、ユーザーを作ったデータベース（認証のデータベース、#530）。
    /// 省略時は`admin`。`username`を書いたときだけ使う
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<String>,
    /// TLS: `disable`・`prefer`・`require`・`verify-full`。省略時は`prefer`だが、
    /// ループバック（PostgreSQLはUnixソケットも）以外では省略できない
    /// （#722。PostgreSQLは`PGSSLMODE`でもよい）。
    /// PostgreSQLは`allow`も使う。MySQL・MariaDBは、`ssl_root_cert`があれば
    /// `verify-ca`も使える（#594。PostgreSQLの`verify-ca`は使えない）。
    /// `verify-full`は、MySQLの`VERIFY_IDENTITY`
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
            .field("keyring", &self.keyring)
            .field("auth_source", &self.auth_source)
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
        self.port.unwrap_or(if self.is_mysql() {
            3306
        } else if self.is_redis() {
            6379
        } else if self.is_mongodb() {
            27017
        } else {
            5432
        })
    }

    /// `engine = "mysql"`か`"mariadb"`か（#527）。どちらも同じ実装・
    /// 設定（ポート3306、`ssl_mode`に`allow`が無い）を使う
    pub fn is_mysql(&self) -> bool {
        self.engine.eq_ignore_ascii_case("mysql") || self.is_mariadb()
    }

    /// `engine = "mariadb"`か（#527）
    pub fn is_mariadb(&self) -> bool {
        self.engine.eq_ignore_ascii_case("mariadb")
    }

    /// `engine = "sqlite"`か（#528）。`database`はファイルのパス
    pub fn is_sqlite(&self) -> bool {
        self.engine.eq_ignore_ascii_case("sqlite")
    }

    /// `engine = "redis"`か（#529）。`database`はデータベースの番号
    pub fn is_redis(&self) -> bool {
        self.engine.eq_ignore_ascii_case("redis")
    }

    /// `engine = "mongodb"`か（#530）。`database`はデータベースの名前
    pub fn is_mongodb(&self) -> bool {
        self.engine.eq_ignore_ascii_case("mongodb")
    }

    /// Redisのデータベースの番号（#529）。`database`を省略すれば0。
    /// 数でなければ`None`
    pub fn redis_database(&self) -> Option<u16> {
        let database = self.database.trim();
        if database.is_empty() {
            Some(0)
        } else {
            database.parse().ok()
        }
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
    /// する（#539）。SQLiteの`database`（ファイルのパス）も（#528）。
    /// `${VAR}`で始まるパスは、環境変数の値をそのまま使う（展開の前なので）
    pub fn resolve_paths(&mut self, base: Option<&std::path::Path>) {
        let Some(base) = base else {
            return;
        };
        let resolve = |value: &mut String| {
            // 空白だけの値は、書いていないのと同じ（PR #799のレビュー）
            if value.trim().is_empty() {
                return;
            }
            let path = std::path::Path::new(value.trim());
            if path.is_relative() && !value.trim().starts_with("${") {
                *value = base.join(path).to_string_lossy().into_owned();
            }
        };
        if let Some(cert) = self.ssl_root_cert.as_mut() {
            resolve(cert);
        }
        // `:memory:`・`file:`のURIは、パスにつなぐと、検証で断れなくなる
        // （PR #798のレビュー）。そのまま残し、`validate`で断る
        if self.is_sqlite() && !self.database.trim().is_empty() && !is_sqlite_uri(&self.database) {
            resolve(&mut self.database);
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
        let access = if self.allow_write() {
            "read-write"
        } else {
            "read-only"
        };
        if self.is_sqlite() {
            return format!("{} {} ({})", self.engine, self.database, access);
        }
        if self.is_redis() || self.is_mongodb() {
            let user = if self.username.trim().is_empty() {
                String::new()
            } else {
                format!("{}@", self.username)
            };
            return format!(
                "{} {}{}:{}/{} ({})",
                self.engine,
                user,
                self.host(),
                self.port(),
                if self.is_redis() {
                    self.redis_database().unwrap_or(0).to_string()
                } else {
                    self.database.clone()
                },
                access
            );
        }
        format!(
            "{} {}@{}:{}/{} ({})",
            self.engine,
            self.username,
            self.host(),
            self.port(),
            self.database,
            access
        )
    }

    /// 書いた値を確かめる。起動しない理由を返す
    pub fn validate(&self) -> Result<(), String> {
        if !self.engine.eq_ignore_ascii_case("postgresql")
            && !self.is_mysql()
            && !self.is_sqlite()
            && !self.is_redis()
            && !self.is_mongodb()
        {
            return Err(format!(
                "database.engine = \"{}\" is not supported (supported: \"postgresql\", \"mysql\", \"mariadb\", \"sqlite\", \"redis\", \"mongodb\")",
                self.engine
            ));
        }
        // SQLite・Redisは、下で専用の検証に入って戻るので、その前に見る
        if !self.is_mongodb() && self.auth_source.is_some() {
            return Err(
                "database.auth_source is used only with engine = \"mongodb\". Remove auth_source"
                    .to_string(),
            );
        }
        if self.is_sqlite() {
            return self.validate_sqlite();
        }
        if self.is_redis() {
            return self.validate_redis();
        }
        if self.is_mongodb() {
            return self.validate_mongodb();
        }
        if self.database.trim().is_empty() {
            return Err("database.database is empty".to_string());
        }
        if self.username.trim().is_empty() {
            return Err("database.username is empty".to_string());
        }
        if let Some(mode) = &self.ssl_mode {
            // PostgreSQLの`verify-ca`は使えない（#594。sqlxが、ssl_root_certが
            // あっても公開のCAを信頼し、ホスト名を見ないため）
            const MODES: &[&str] = &["disable", "allow", "prefer", "require", "verify-full"];
            // MySQLには、`allow`（暗号化せずに試し、だめならTLS）が無い
            const MYSQL_MODES: &[&str] =
                &["disable", "prefer", "require", "verify-ca", "verify-full"];
            let modes = if self.is_mysql() { MYSQL_MODES } else { MODES };
            if !modes.contains(&mode.as_str()) {
                let why = if mode == "verify-ca" {
                    ". For PostgreSQL, verify-ca would accept a certificate for any host: use verify-full"
                } else {
                    ""
                };
                return Err(format!(
                    "database.ssl_mode = \"{}\" is not valid (use one of: {}){}",
                    mode,
                    modes.join(", "),
                    why
                ));
            }
            // MySQL・MariaDBの`verify-ca`は、ssl_root_certが要る（#594）
            let has_root_cert = self
                .ssl_root_cert
                .as_deref()
                .is_some_and(|cert| !cert.trim().is_empty());
            if self.is_mysql() && mode == "verify-ca" && !has_root_cert {
                return Err(
                    "database.ssl_mode = \"verify-ca\" needs ssl_root_cert (the CA that signed the \
                     server certificate): without it, a certificate for any host from a public CA \
                     would be accepted. Set ssl_root_cert, or use ssl_mode = \"verify-full\""
                        .to_string(),
                );
            }
        }
        Ok(())
    }

    /// MongoDBの`[database]`を確かめる（#530）。`database`はデータベースの名前、
    /// `ssl_mode`は`disable`・`require`・`verify-full`。`ssl_root_cert`は
    /// `verify-full`のときだけ使う
    fn validate_mongodb(&self) -> Result<(), String> {
        let name = self.database.trim();
        if name.is_empty() {
            return Err(
                "database.database is empty: for engine = \"mongodb\", write the name of the database"
                    .to_string(),
            );
        }
        // どのOSのサーバーにも共通する制限だけを確かめる（#808）。Windowsの
        // サーバーでだけ使えない`*<>:|?`は、サーバーに任せる
        if name.len() > 63 || name.chars().any(|c| "/\\. \"$\0".contains(c)) {
            return Err(format!(
                "database.database = \"{name}\" is not a MongoDB database name"
            ));
        }
        if self.auth_source.is_some() && self.username.trim().is_empty() {
            return Err(
                "database.auth_source is used only with username. Remove auth_source".to_string(),
            );
        }
        self.validate_tls_modes("MongoDB")
    }

    /// Redis・MongoDBの`ssl_mode`（`disable`・`require`・`verify-full`）と、
    /// `ssl_root_cert`（`verify-full`のときだけ）を確かめる
    fn validate_tls_modes(&self, engine: &str) -> Result<(), String> {
        const MODES: &[&str] = &["disable", "require", "verify-full"];
        if let Some(mode) = &self.ssl_mode {
            if !MODES.contains(&mode.as_str()) {
                return Err(format!(
                    "database.ssl_mode = \"{mode}\" is not valid for {engine} (use one of: {})",
                    MODES.join(", ")
                ));
            }
        }
        let has_root_cert = self
            .ssl_root_cert
            .as_deref()
            .is_some_and(|cert| !cert.trim().is_empty());
        if has_root_cert && self.ssl_mode.as_deref() != Some("verify-full") {
            return Err(format!(
                "database.ssl_root_cert is used only with ssl_mode = \"verify-full\" for {engine}"
            ));
        }
        Ok(())
    }

    /// Redisの`[database]`を確かめる（#529）。`database`はデータベースの番号、
    /// `ssl_mode`は`disable`・`require`・`verify-full`。`ssl_root_cert`は
    /// `verify-full`のときだけ使う
    fn validate_redis(&self) -> Result<(), String> {
        if self.redis_database().is_none() {
            return Err(format!(
                "database.database = \"{}\" is not a Redis database number (for engine = \"redis\", write it as a string, such as database = \"0\", or leave it out)",
                self.database.trim()
            ));
        }
        self.validate_tls_modes("Redis")
    }

    /// SQLiteの`[database]`を確かめる（#528）。`database`はファイルのパス。
    /// サーバーにつなぐための項目は使わないので、書いてあれば断る（黙って
    /// 無視すると、別のデータベースのつもりで書いた設定に気付けない）
    fn validate_sqlite(&self) -> Result<(), String> {
        let path = self.database.trim();
        if path.is_empty() {
            return Err(
                "database.database is empty: for engine = \"sqlite\", write the path of the database file"
                    .to_string(),
            );
        }
        if is_sqlite_uri(path) {
            return Err(format!(
                "database.database = \"{path}\" is not a file path: for engine = \"sqlite\", write the path of an existing database file"
            ));
        }
        let unused = [
            ("host", self.host.is_some()),
            ("port", self.port.is_some()),
            ("username", !self.username.trim().is_empty()),
            ("password", self.password.is_some()),
            ("keyring", self.keyring.is_some()),
            ("ssl_mode", self.ssl_mode.is_some()),
            ("ssl_root_cert", self.ssl_root_cert.is_some()),
        ];
        if let Some((name, _)) = unused.iter().find(|(_, given)| *given) {
            return Err(format!(
                "database.{name} is not used with engine = \"sqlite\" (database is the path of the database file). Remove {name}"
            ));
        }
        Ok(())
    }
}

/// SQLiteの、ファイルのパスではない書き方（`:memory:`・`file:`・`sqlite:`のURI）か
fn is_sqlite_uri(value: &str) -> bool {
    let value = value.trim();
    value == ":memory:" || value.starts_with("file:") || value.starts_with("sqlite:")
}

impl PluginConfig {
    /// 相対パスの`command`・`working_dir`を、`base`（設定ファイルのフォルダ）
    /// を基準にする（#598）。以前は、mcp-rsを起動したフォルダ（Claude Code
    /// では開いたフォルダ）を基準にしていた。`command`は、区切り文字を含む
    /// とき（`./server`・`scripts/run.sh`等）だけ。`node`のような名前だけの
    /// ものは、今までどおり`PATH`から探す。コンテナ・Kubernetesでは、
    /// どちらもコンテナの中のパスなので変えない
    pub fn resolve_paths(&mut self, base: Option<&std::path::Path>) {
        let Some(base) = base else {
            return;
        };
        if self.container.is_some() || self.kubernetes.is_some() {
            return;
        }
        let command = self.command.trim();
        let has_separator = command.contains('/') || command.contains('\\');
        if has_separator && std::path::Path::new(command).is_relative() {
            self.command = base.join(command).to_string_lossy().into_owned();
        }
        if let Some(dir) = self.working_dir.as_mut() {
            if std::path::Path::new(dir.trim()).is_relative() {
                *dir = base.join(dir.trim()).to_string_lossy().into_owned();
            }
        }
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
    /// `true`で、プラグインがツールに付けた「読み取りだけ」の印
    /// （`readOnlyHint`）を、APIキー・トークンの`@read_only`の判定に使う
    /// （#595）。省略時は`false`: プラグインのツールは、`@read_only`に
    /// 含めない。印はプラグイン自身が付けるので、書き込むツールに付いて
    /// いても、mcp-rsには分からないため。AIのクライアントに見せる印は
    /// 変えない
    #[serde(default)]
    pub trust_annotations: Option<bool>,
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
            .field("trust_annotations", &self.trust_annotations)
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
    /// `network = "allowlist"`で、つないでよい行き先（`"api.example.com:443"`、
    /// `"*.example.com:443"`。#615）。`CONNECT`だけを受け付け、つないだ後は中身を確認しないTCPのトンネル
    pub allowed_hosts: Option<Vec<String>>,
    /// `network = "allowlist"`で、中継のプロキシに使うイメージ。省略時は、
    /// このmcp-rsと同じ版のmcp-rsのイメージ（`ghcr.io/n-takatsu/mcp-rs:<版>`）。
    /// プラグインのイメージと同じく、事前に`docker pull`しておく
    pub egress_proxy_image: Option<String>,
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
    let first_new = headers.iter().position(|h| h == "[[plugins]]");
    let has_new = first_new.is_some();
    // `[[plugins]]`より後の`[plugins.*]`（`container`・`kubernetes`・`env`・
    // `rate_limit`等）は、直前の`[[plugins]]`の設定（#411・#413・#750）
    let has_legacy = headers.iter().enumerate().any(|(i, h)| {
        h == "[plugins]"
            || (h.starts_with("[plugins.")
                && h.ends_with(']')
                && first_new.is_none_or(|first| i < first))
    });
    (has_new && has_legacy).then_some(
        "The config file has both the old [plugins] section (v0.18.1 and earlier, never used by mcp-rs) and [[plugins]] entries. Delete the old [plugins] section, including [plugins.*] subsections",
    )
}

/// 引用符（`"`・`'`）の中を`…`に伏せる（#543）。設定の読み込みのエラーは、
/// `invalid type: string "…", expected u16`のように、書いた値を含むことが
/// ある。値がパスワードのこともあり、標準エラーはClaude Desktopのログに残る
///
/// 引用符の中の`\`は、次の1文字とあわせて飛ばす（PR #552のレビュー）。
/// serde・configのエラーは値を`{:?}`で表示し、値の中の`"`を`\"`と書く
/// ので、そこで伏せるのを終えると、値の残りが出てしまう。英単語の中の
/// `'`（`can't`等）は、引用符とみなさない
pub fn redact_quoted(message: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut previous: Option<char> = None;
    for c in message.chars() {
        match quote {
            Some(_) if escaped => escaped = false,
            Some(_) if c == '\\' => escaped = true,
            Some(q) if c == q => {
                out.push('…');
                out.push(c);
                quote = None;
            }
            Some(_) => {}
            None => {
                out.push(c);
                let in_word = c == '\'' && previous.is_some_and(char::is_alphanumeric);
                if (c == '"' || c == '\'' || c == '`') && !in_word {
                    quote = Some(c);
                }
            }
        }
        previous = Some(c);
    }
    if quote.is_some() {
        out.push('…');
    }
    out
}

/// 項目の名前として出す長さの上限（#617）
const FIELD_NAME_MAX_CHARS: usize = 64;

/// 設定の読み込みのエラーの理由を、値を伏せて表す（#543）。ただし、
/// `missing field`・`unknown field`の項目の名前は残す（#617）。伏せると、
/// どこを直せばよいか分からない。項目の名前はmcp-rsの設定の型が決める
/// もので、値ではない。`unknown field`の名前と、`expected one of …`の
/// 候補は残すが、名前はユーザーが書いたものなので、長さを切り、制御文字を
/// エスケープする。configクレートが付ける` for key `…``（項目の場所）も
/// 残す。前に引用符の無いときだけ扱う（値の中の同じ文字列で、値を出さない
/// ように）
pub fn redact_config_error(message: &str) -> String {
    for (prefix, keep_candidates) in [("missing field `", false), ("unknown field `", true)] {
        let Some(start) = message.find(prefix) else {
            continue;
        };
        let before = &message[..start];
        if before.contains(['"', '\'', '`']) {
            continue;
        }
        let after_prefix = &message[start + prefix.len()..];
        let Some(name_len) = after_prefix.find('`') else {
            continue;
        };
        let name = &after_prefix[..name_len];
        let rest = &after_prefix[name_len + 1..];
        let rest = if keep_candidates || rest.starts_with(" for key `") {
            keep_field_names(rest)
        } else {
            redact_quoted(rest)
        };
        return format!("{before}{prefix}{}`{rest}", sanitize_field_name(name));
    }
    // 型の誤り等（`invalid type: string "…", expected u16 for key `a.b``）も、
    // 最後の` for key `…``（項目の場所）は残す（#724。伏せると、どの項目を
    // 直せばよいか分からない）。値は伏せる。最後の1つだけを、項目の名前に
    // 使う文字だけのときに残す（値の中の同じ文字列は、最後にならない）
    if let Some(at) = message.rfind(" for key `") {
        let key = &message[at + " for key `".len()..];
        if let Some(key) = key.strip_suffix('`') {
            if !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-.[]".contains(c))
            {
                return format!("{} for key `{key}`", redact_quoted(&message[..at]));
            }
        }
    }
    redact_quoted(message)
}

/// `, expected one of `a`, `b``の候補（`` ` ``の中）を残し、ほかの引用符の
/// 中は伏せる。`` ` ``が対にならなければ、すべて伏せる
fn keep_field_names(text: &str) -> String {
    let parts: Vec<&str> = text.split('`').collect();
    if parts.len().is_multiple_of(2) {
        return redact_quoted(text);
    }
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 {
                sanitize_field_name(part)
            } else {
                redact_quoted(part)
            }
        })
        .collect::<Vec<_>>()
        .join("`")
}

/// ユーザーが書いた項目の名前を、エラーに出せる形にする（#617）
fn sanitize_field_name(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if i == FIELD_NAME_MAX_CHARS {
            out.push('…');
            break;
        }
        if c.is_control() {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}

/// TOMLの読み込みエラーを、行と列の番号と、値を伏せた理由だけで表す
/// （#543）。tomlのエラーの表示は、誤りのある行をそのまま引用するので、
/// `password = "…"x`のような誤りで、パスワードが標準エラーに出ていた
pub fn describe_toml_error(error: &toml::de::Error, content: &str) -> String {
    let reason = redact_config_error(error.message().trim());
    match error.span() {
        Some(span) => {
            let before = &content[..span.start.min(content.len())];
            let line = before.matches('\n').count() + 1;
            let column = before
                .rsplit('\n')
                .next()
                .map_or(0, |text| text.chars().count())
                + 1;
            format!("TOML parse error at line {line}, column {column}: {reason}")
        }
        None => format!("TOML parse error: {reason}"),
    }
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
///
/// `serde_json::Value`を経由すると、`enabled = nan`等のJSONに無い値が
/// `null`（省略）になり、既定の値で動いてしまう（#750）。型を保つ
/// `toml::Value`から`PluginConfig`として読み、不正な値は断る。
fn deserialize_plugins<'de, D>(deserializer: D) -> Result<Vec<PluginConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserializer.deserialize_any(PluginsVisitor)
}

struct PluginsVisitor;

impl<'de> serde::de::Visitor<'de> for PluginsVisitor {
    type Value = Vec<PluginConfig>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("plugins must be [[plugins]] entries")
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        // 1つずつ`toml::Value`（型を保つ）として受け取ってから読む。
        // `config`クレートから直接読むと、`nan`・`1`・`"yes"`も`bool`に、
        // `nan`も数に変換されてしまう
        let mut plugins = Vec::new();
        while let Some(entry) = seq.next_element::<toml::Value>()? {
            // `toml::de::Error`の表示は改行で終わるので、理由だけを使う
            let plugin = PluginConfig::deserialize(entry).map_err(|e| {
                serde::de::Error::custom(format!(
                    "invalid [[plugins]] entry: {}",
                    e.message().trim()
                ))
            })?;
            plugins.push(plugin);
        }
        Ok(plugins)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        while map
            .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
            .is_some()
        {}
        // ログはまだ初期化されていないことがあるため、標準エラー出力に書く
        eprintln!(
            "[mcp-rs] warning: the [plugins] table from v0.18.1 and earlier is ignored. To add a plugin, use [[plugins]] entries"
        );
        Ok(Vec::new())
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
    /// 新しい版があるかを、GitHubに問い合わせて確かめるか（#635）。
    /// `mcp-rs check`と、STDIOモードの`mcp_rs_status`が使う。省略時は`true`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_updates: Option<bool>,
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
    // `[transport.stdio]`（`framing_method`等）は、v0.23.0まではここで読んで
    // いたが、サーバー（`McpServer::run_stdio`）は使っていなかったので、
    // 削除した（#557）。書いてあっても、無視する
    /// HTTP transport configuration
    pub http: Option<HttpTransportConfig>,
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
    /// IP addresses or CIDR ranges of the reverse proxies in front of
    /// mcp-rs (#543). Headers that a proxy sets (`X-Forwarded-Proto`,
    /// `Forwarded`, the TLS version, `certificate_pin_header`,
    /// `X-Forwarded-For`, `X-Real-IP`) are used only on connections from
    /// these addresses. Required by `enforce_https` and certificate pinning
    /// without `tls_enabled`, and by `ids_trust_forwarded_for`. Defaults to
    /// empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_proxies: Option<Vec<String>>,
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
    /// takes. It also closes a connection with no request in progress for
    /// this long, and a connection whose response body stops being sent:
    /// each part of the body must be sent within this time plus the time it
    /// takes at 64 KiB/s (#759, #779). Defaults to 30.
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

impl HttpTransportConfig {
    /// APIキーの`tools`か、OAuthの`scope_tools`に、`preset`（`@read_only`
    /// 等）を書いたか
    pub fn uses_tool_preset(&self, preset: &str) -> bool {
        let key_items = self
            .api_keys
            .iter()
            .flatten()
            .filter_map(|key| key.tools.as_ref())
            .flatten();
        let scope_items = self
            .oauth
            .iter()
            .flat_map(|oauth| oauth.scope_tools.values())
            .flatten();
        key_items
            .chain(scope_items)
            .any(|item| item.trim() == preset)
    }
}

/// TOML-facing shape for a single API key entry under
/// `transport.http.api_keys`. Mirrors `transport::http::ApiKeyEntry`.
/// `Debug` does not show `key` (#543).
#[derive(Clone, Deserialize, Serialize)]
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

impl fmt::Debug for ApiKeyEntryConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiKeyEntryConfig")
            .field("key", &"<redacted>")
            .field("name", &self.name)
            .field("user_id", &self.user_id)
            .field("tools", &self.tools)
            .finish()
    }
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

/// `[handlers.wordpress]`と、`[handlers.wordpress.sites.<名前>]`の各サイト。
///
/// サイトが1つなら、`[handlers.wordpress]`に`url`・資格情報を直接書ける
/// （今までの形。サイトの名前は`default`）。複数のサイト（#640）は、すべて
/// `sites`の下に対等に書き、`[handlers.wordpress]`には、全体の`enabled`・
/// 既定のサイト（`default_site`）・全サイト共通の設定（`timeout_seconds`・
/// `rate_limit`・`upload_allowed_dirs`。サイトごとに上書きできる）だけを書く
#[derive(Clone, Deserialize, Serialize)]
pub struct WordPressConfig {
    /// サイトのURL。`sites`を使うときは、各サイトに書く
    #[serde(default)]
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
    /// `upload_media`の`file_path`で読み込みを許可するフォルダ。省略（既定）
    /// か空なら`file_path`は使えない。mcp-rsが動いているマシン上のファイルを
    /// 読んでWordPressへ送る機能のため、明示的に許可したフォルダの中だけに
    /// 限定する（起動時に存在しないフォルダが指定されていればエラー）。
    /// `sites`では、サイトで省くと`[handlers.wordpress]`のものを使い、
    /// `[]`と書くと、そのサイトでは`file_path`を使わない（#684）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_allowed_dirs: Option<Vec<String>>,
    /// `post`・`page`のほかに使ってよい、カスタム投稿タイプ（#642。例:
    /// `["product"]`）。省略か空なら`post`・`page`だけ。AIに商品等を
    /// 書き換えさせるかは、ここで決める。`sites`では、サイトで省くと
    /// `[handlers.wordpress]`のものを使い、`[]`と書くと、そのサイトでは
    /// `post`・`page`だけにする（#684）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_types: Option<Vec<String>>,
    /// ツールの`site`の引数を省いたときに使うサイトの名前（#640）。
    /// `[handlers.wordpress]`にだけ書く。使うサイトが1つなら省ける
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_site: Option<String>,
    /// WordPressのサイト（#640）。`[handlers.wordpress.sites.<名前>]`に、
    /// `url`・資格情報等を書く。`enabled = false`で、そのサイトだけを止める。
    /// ツールは増やさず、`site`の引数で選ぶ
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sites: BTreeMap<String, WordPressConfig>,
}

/// `sites`を使わない（今までの形の）サイトの名前（#640）
pub const DEFAULT_WORDPRESS_SITE: &str = "default";

/// WordPressのサイトの名前の長さの上限（#640）
const MAX_WORDPRESS_SITE_NAME_LEN: usize = 32;

/// WordPressのサイトの名前を確かめる（#640）。英小文字・数字・`_`・`-`で、
/// 32文字まで
pub(crate) fn check_wordpress_site_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= MAX_WORDPRESS_SITE_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "WordPress site name '{name}' is not valid: use lowercase letters, digits, '_' and '-', up to {MAX_WORDPRESS_SITE_NAME_LEN} characters"
        ))
    }
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
            .field("post_types", &self.post_types)
            .field("default_site", &self.default_site)
            .field("sites", &self.sites)
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
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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
                    "WordPress認証情報が設定されていません。keyring・username/password・encrypted_credentialsのいずれかを指定してください".to_string(),
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
        // 今までの形（サイトが1つ）
        if self.sites.is_empty() {
            return self.resolve_site_credentials(store, "handlers.wordpress");
        }
        // 複数のサイト（#640）。全体が使われないときは、読まない
        if !self.enabled.unwrap_or(false) {
            return Ok(());
        }
        self.check_sites_layout()?;
        self.inherit_into_sites();
        for (name, site) in self.sites.iter_mut() {
            // 使わないサイトの資格情報は読まない。`encrypted_credentials`の
            // 復号に失敗して、ほかのサイトまで起動できなくなるのを防ぐ（#684）
            if !site.enabled.unwrap_or(false) {
                continue;
            }
            let label = format!("handlers.wordpress.sites.{name}");
            site.resolve_site_credentials(store, &label).map_err(
                |e| -> Box<dyn std::error::Error> {
                    let message = e.to_string();
                    if message.starts_with(&label) {
                        message.into()
                    } else {
                        format!("{label}: {message}").into()
                    }
                },
            )?;
        }
        Ok(())
    }

    /// 各サイトの`enabled`を決める（#640）。全体（`[handlers.wordpress]`）が
    /// 使われなければ、どのサイトも使わない。使われるなら、`enabled = false`
    /// と書いたサイト以外を使う
    fn inherit_into_sites(&mut self) {
        let enabled = self.enabled.unwrap_or(false);
        for site in self.sites.values_mut() {
            site.enabled = Some(enabled && site.enabled.unwrap_or(true));
        }
    }

    /// `sites`を使うときの、表の書き方を確かめる（#640）。サイトの`url`・
    /// 資格情報は、`[handlers.wordpress]`ではなく各サイトに書く。
    /// `default_site`・`sites`は、`[handlers.wordpress]`にだけ書く
    fn check_sites_layout(&self) -> Result<(), String> {
        let site_keys: Vec<&str> = [
            ("url", !self.url.is_empty()),
            ("username", !self.username.is_empty()),
            ("password", !self.password.is_empty()),
            ("keyring", self.keyring.is_some()),
            (
                "encrypted_credentials",
                self.encrypted_credentials.is_some(),
            ),
        ]
        .into_iter()
        .filter_map(|(key, set)| set.then_some(key))
        .collect();
        if !site_keys.is_empty() {
            return Err(format!(
                "handlers.wordpress: with [handlers.wordpress.sites.<site>], write {} in each site's table, not in [handlers.wordpress]",
                site_keys.join(", ")
            ));
        }
        for (name, site) in &self.sites {
            let label = format!("handlers.wordpress.sites.{name}");
            check_wordpress_site_name(name).map_err(|e| format!("{label}: {e}"))?;
            if site.default_site.is_some() || !site.sites.is_empty() {
                return Err(format!(
                    "{label}: default_site and sites can be written only in [handlers.wordpress]"
                ));
            }
        }
        Ok(())
    }

    /// 使うWordPressのサイトの一覧（#640）。最初が既定のサイト。
    ///
    /// - `sites`が無ければ、`[handlers.wordpress]`の1つ（名前は`default`）
    /// - `sites`があれば、使うサイトを、既定のサイト、残りを名前の順に。
    ///   全サイト共通の設定（`timeout_seconds`・`rate_limit`・
    ///   `upload_allowed_dirs`・`post_types`）は、サイトに無ければ
    ///   `[handlers.wordpress]`のものを使う（`[]`と書いたサイトは受け継がない）。
    ///   使うサイトが2つ以上なら、`default_site`が要る
    ///
    /// 返す設定には、`sites`・`default_site`を含めない
    pub fn site_list(&self) -> Result<Vec<(String, WordPressConfig)>, String> {
        if self.sites.is_empty() {
            if let Some(default_site) = &self.default_site {
                return Err(format!(
                    "handlers.wordpress.default_site: there is no [handlers.wordpress.sites.{default_site}]. default_site is used with [handlers.wordpress.sites.<site>]"
                ));
            }
            if self.url.trim().is_empty() {
                return Err("handlers.wordpress.url is not set".to_string());
            }
            return Ok(vec![(DEFAULT_WORDPRESS_SITE.to_string(), self.clone())]);
        }
        self.check_sites_layout()?;
        let mut list = Vec::new();
        for (name, site) in &self.sites {
            if !(self.enabled.unwrap_or(false) && site.enabled.unwrap_or(true)) {
                continue;
            }
            if site.url.trim().is_empty() {
                return Err(format!("handlers.wordpress.sites.{name}.url is not set"));
            }
            let mut site = site.clone();
            site.enabled = Some(true);
            site.timeout_seconds = site.timeout_seconds.or(self.timeout_seconds);
            if site.rate_limit.is_none() {
                site.rate_limit = self.rate_limit.clone();
            }
            // 省いたときだけ受け継ぐ。`[]`と書けば、受け継がない（#684）
            if site.upload_allowed_dirs.is_none() {
                site.upload_allowed_dirs = self.upload_allowed_dirs.clone();
            }
            if site.post_types.is_none() {
                site.post_types = self.post_types.clone();
            }
            list.push((name.clone(), site));
        }
        if list.is_empty() {
            return Err(
                "handlers.wordpress: every site in [handlers.wordpress.sites.<site>] has enabled = false"
                    .to_string(),
            );
        }
        match &self.default_site {
            Some(default_site) => {
                let index = list
                    .iter()
                    .position(|(name, _)| name == default_site)
                    .ok_or_else(|| {
                        let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
                        format!(
                            "handlers.wordpress.default_site: '{default_site}' is not a site in use. Use one of: {}",
                            names.join(", ")
                        )
                    })?;
                let default = list.remove(index);
                list.insert(0, default);
            }
            None if list.len() > 1 => {
                let names: Vec<&str> = list.iter().map(|(n, _)| n.as_str()).collect();
                return Err(format!(
                    "handlers.wordpress.default_site is required with more than one site: write default_site = \"<site>\", one of: {}",
                    names.join(", ")
                ));
            }
            None => {}
        }
        Ok(list)
    }

    /// 1つのサイトの資格情報を読む。`label`は、エラーに出す表の名前
    fn resolve_site_credentials(
        &mut self,
        store: &dyn crate::security::keyring_store::SecretStore,
        label: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(name) = self.keyring.clone() else {
            // 使わない設定（enabled = false）では、`encrypted_credentials`も
            // 復号しない。`WORDPRESS_MASTER_PASSWORD`が無いだけで、ほかの
            // 機能まで起動できなくなるのを防ぐ（#755。複数のサイトの形は#684）
            if !self.enabled.unwrap_or(false) {
                return Ok(());
            }
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
                "{}: keyring cannot be combined with {}. Remove {} from the config file (the keyring entry holds the username and password)",
                label,
                others.join(", "),
                if others.len() == 1 { "it" } else { "them" },
            )
            .into());
        }
        let credentials = crate::security::keyring_store::load_wordpress_credentials(store, &name)
            .map_err(|e| format!("{label}.keyring: {e}"))?;
        // 資格情報を保存したサイトと、設定の`url`が同じときだけ使う（#541）。
        // 別の人が書いた設定ファイルが、この項目の名前と別のサイトの`url`を
        // 書いて、パスワードを受け取れないようにする
        crate::security::keyring_store::check_site(&name, &credentials, &self.url)
            .map_err(|e| format!("{label}.keyring: {e}"))?;
        self.username = credentials.username;
        self.password = credentials.application_password.to_string();
        Ok(())
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                check_updates: None,
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
                http: None,
            },
            handlers: HandlersConfig { wordpress: None },
            plugins: Vec::new(),
            audit: None,
            database: None,
            config_file: None,
            startup_notes: Vec::new(),
            missing_config_file: None,
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
/// フォルダ（`dir`）にあったが読まなかったファイル。案内するパスは絶対パスに
/// する（相対パスの`--config`は、起動したフォルダから読むので、同じ問題が
/// 残る。PR #551のレビュー）
fn no_config_file_message(home: Option<&str>, dir: &std::path::Path, former: &[String]) -> String {
    let user_config =
        expand_home_dir(USER_CONFIG_PATH, home).unwrap_or_else(|| USER_CONFIG_PATH.to_string());
    let mut message = format!(
        "no config file: run `mcp-rs init` to create {user_config}, or pass one with --config <absolute path>"
    );
    if let Some(first) = former.first() {
        let absolute = std::path::absolute(dir.join(first))
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| first.clone());
        message.push_str(&format!(
            ". {} in the current folder is not read without --config (v0.24.0 and later), so that a config file in a folder you open cannot run commands or send your credentials. If it is yours, start mcp-rs with --config {absolute} (an absolute path: a relative one is read from whatever folder mcp-rs starts in)",
            former.join(", ")
        ));
    }
    message
}

/// `--config`無しで読む設定ファイル（`~/.config/mcp-rs/config.toml`）の、
/// Claude Desktopの拡張機能（`.mcpb`）から起動したことを示す環境変数（#632）。
/// 値は`1`
pub const EXTENSION_ENV_VAR: &str = "MCP_RS_MCPB";

/// 拡張機能から起動したか（#632）
pub fn started_by_extension() -> bool {
    std::env::var(EXTENSION_ENV_VAR).is_ok_and(|v| v == "1")
}

/// `--config`の代わりに使う設定ファイルの環境変数（#632）。Claude Desktopの
/// 拡張機能（`.mcpb`）は、画面で選んだ設定ファイルをこれで渡す
pub const CONFIG_ENV_VAR: &str = "MCP_RS_CONFIG";

/// `MCP_RS_CONFIG`の値から、設定ファイルのパスを決める（#632）。空・空白、
/// 拡張機能が埋めなかった`${user_config.…}`は、無いものとする。値の全体が
/// `${...}`のときだけ埋まらなかったとみなし、フォルダ名等に`${`を含む
/// パスは使う（#774）。`${a}/b-${c}`のように、中に`{`・`}`があれば、1つの
/// `${...}`ではないので使う（#779）
pub fn config_path_from_env(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|path| !path.is_empty() && !is_placeholder(path))
        .map(str::to_string)
}

/// 値の全体が、1つの`${...}`か（#779）
fn is_placeholder(value: &str) -> bool {
    value
        .strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
        .is_some_and(|inner| !inner.contains(['{', '}']))
}

/// ホームフォルダを展開したパス。`mcp-rs init`の既定の書き先（#575）。
/// ホームフォルダが分からなければ`None`
pub fn user_config_path() -> Option<String> {
    let home = home_dir_from_env()?;
    expand_home_dir(USER_CONFIG_PATH, Some(&home)).filter(|path| !path.starts_with('~'))
}

/// ホームフォルダ（`HOME`、無ければ`USERPROFILE`）。空の値は飛ばす（#710。
/// 以前は`HOME`が空の文字列だと、`USERPROFILE`を見なかった）
pub fn home_dir_from_env() -> Option<String> {
    home_dir_from(|name| std::env::var(name).ok())
}

fn home_dir_from(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(var)
        .find(|home| !home.trim().is_empty())
}

/// `--config`のパスの先頭の`~/`（Windowsでは`~\`も）を、ホームフォルダに
/// 展開する（PR #551のレビュー）。シェルを通さないクライアントの設定（JSONの
/// `args`等）では、`~`は展開されずにそのまま渡ってくる。展開しないと、
/// 起動したフォルダの中の`~`という名前のフォルダを探してしまう。ホーム
/// フォルダが分からなければ、そのまま返す（相対パスの警告が出る）
pub fn expand_config_path(path: &str) -> String {
    let home = home_dir_from_env();
    expand_config_path_with(path, home.as_deref())
}

fn expand_config_path_with(path: &str, home: Option<&str>) -> String {
    expand_config_path_for(path, home, cfg!(windows))
}

/// `windows`のときだけ、`~\`と、区切り文字の`\`を扱う（#724。Unixでは、
/// `\`は名前の文字なので、`/`に変えると別のファイルになる）
fn expand_config_path_for(path: &str, home: Option<&str>, windows: bool) -> String {
    let rest = path
        .strip_prefix("~/")
        .or_else(|| path.strip_prefix("~\\").filter(|_| windows));
    match rest {
        Some(rest) => {
            let rest = if windows {
                rest.replace('\\', "/")
            } else {
                rest.to_string()
            };
            expand_home_dir(&format!("~/{rest}"), home).unwrap_or_else(|| path.to_string())
        }
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
                upload_allowed_dirs: None,
                post_types: None,
                default_site: None,
                sites: Default::default(),
            });
        }
        Some(wp_config) if wp_config.keyring.is_some() => {}
        // 複数のサイト（#640）は、どのサイトかが決まらないので、上書きしない
        Some(wp_config) if !wp_config.sites.is_empty() => {}
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
    let mut table: toml::Table = content
        .parse()
        .map_err(|e| describe_toml_error(&e, content))?;
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
        config.inherit_into_sites();
        Self::expand_wordpress_fields(config, "handlers.wordpress")?;
        // ほかのサイト（#640）
        for (name, site) in config.sites.iter_mut() {
            if site.enabled.unwrap_or(false) {
                Self::expand_wordpress_fields(site, &format!("handlers.wordpress.sites.{name}"))?;
            }
        }
        Ok(())
    }

    /// 1つのサイトの`url`・`username`・`password`の`${VAR}`を展開する。
    /// `prefix`は、エラーに出す表の名前
    fn expand_wordpress_fields(
        config: &mut WordPressConfig,
        prefix: &str,
    ) -> Result<(), ConfigEnvError> {
        // `encrypted_credentials`があれば、`username`・`password`は復号した
        // 値で上書きするので、展開しない（#746。古い`${VAR}`が残っていても、
        // その変数が無いことで起動を止めない）
        let encrypted = config.encrypted_credentials.is_some();
        for (field, value) in [
            ("url", &mut config.url),
            ("username", &mut config.username),
            ("password", &mut config.password),
        ] {
            if encrypted && field != "url" {
                continue;
            }
            *value = Self::expand_env_vars(value).map_err(|reason| ConfigEnvError {
                field: format!("{prefix}.{field}"),
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

    /// `[database]`の`host`・`database`・`username`・`password`・
    /// `ssl_root_cert`（PR #822のレビュー）の`${VAR}`を展開する（#453）。使わない設定（`enabled = false`）なら展開しない。
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
        // `resolve_paths`は`${`で始まるパスを、展開の後の値をそのまま使うよう
        // 残す。ここで展開しないと、`${VAR}`の文字のままファイルを開いていた
        // （PR #822のレビュー）
        if let Some(cert) = config.ssl_root_cert.as_mut() {
            expand("ssl_root_cert", cert)?;
        }
        Ok(())
    }

    /// プラグインの相対パスを、設定ファイルのフォルダを基準にする（#598）
    fn resolve_plugin_paths(&mut self) {
        let base = self
            .config_file
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf);
        for plugin in &mut self.plugins {
            plugin.resolve_paths(base.as_deref());
        }
    }

    /// `[database]`の`${VAR}`を展開し（`expand_database_config`）、`keyring`が
    /// あれば、OSの資格情報ストアからパスワードを読む（#590）。起動と
    /// `mcp-rs check`で使う。使わない設定（`enabled = false`）なら何もしない
    pub fn resolve_database_credentials(config: &mut DatabaseSettings) -> Result<(), String> {
        Self::resolve_database_credentials_with(config, &crate::security::keyring_store::OsKeyring)
    }

    /// `resolve_database_credentials`の本体。`store`は、`keyring`のときに
    /// 読む先
    pub fn resolve_database_credentials_with(
        config: &mut DatabaseSettings,
        store: &dyn crate::security::keyring_store::SecretStore,
    ) -> Result<(), String> {
        if !config.is_enabled() {
            return Ok(());
        }
        Self::expand_database_config(config).map_err(|e| e.to_string())?;
        // SQLiteは資格情報を使わないので、書いてはいけない項目（`keyring`等）を、
        // 資格情報ストアを読む前に断る（PR #798のレビュー）
        if config.is_sqlite() {
            return config.validate();
        }
        let Some(name) = config.keyring.clone() else {
            return Ok(());
        };
        if config.password.is_some() {
            return Err(
                "database: keyring cannot be combined with password. Remove password from [database] (the keyring entry holds it)"
                    .to_string(),
            );
        }
        let credentials = crate::security::keyring_store::load_database_credentials(
            store,
            &name,
            config.host(),
            config.port(),
            &config.username,
        )
        .map_err(|e| format!("database.keyring: {e}"))?;
        config.password = Some(credentials.password.to_string());
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

    /// `--config`で指定したファイルを読む。ファイルが無いとき・TOMLの誤りは、
    /// 値を伏せた理由を返す（#543）。`${VAR}`の展開と資格情報の解決は、
    /// `resolve_wordpress_credentials`・`expand_http_api_keys`で行う。
    /// 起動と`mcp-rs check`（#565）が同じ順で呼び、checkは段階ごとに結果を
    /// 出す
    pub fn read_file(path: &str) -> Result<Self, String> {
        if !std::path::Path::new(path).exists() {
            return Err(format!("設定ファイルが存在しません: {}", path));
        }
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
        let mut config: McpConfig = toml::from_str(&content)
            .map_err(|e| with_config_hint(describe_toml_error(&e, &content), &content))?;
        // 監査ログの既定の書き込み先（設定ファイルと同じフォルダ）に使う（#416）
        config.config_file = std::path::absolute(path).ok();
        config.resolve_plugin_paths();
        Ok(config)
    }

    /// 拡張機能（#632）で、サイトのURL・ユーザー名・アプリケーションパスワードが
    /// 入力されていなければ、WordPressを使わずに起動し、何を入力するかを
    /// `mcp_rs_status`に出す（拡張機能では、起動に失敗すると、Claude Desktopには
    /// 「切断された」としか出ないため）。拡張機能が埋めなかった
    /// `${user_config.…}`は、入力されていないものとする
    /// 拡張機能（#632）から起動したときの設定。Claude DesktopはSTDIOで
    /// つなぐので、選んだ設定ファイルがHTTPモードでも、STDIOで動かす
    /// （PR #666のレビュー）。サイトが未入力なら`allow_unset_extension_site`
    pub fn prepare_for_extension(&mut self) {
        if self.transport.transport_type.as_deref() != Some("stdio") {
            info!(
                "Started by the Claude Desktop extension: using STDIO instead of [transport] type = {:?}",
                self.transport.transport_type.as_deref().unwrap_or("(not set)")
            );
            self.transport.transport_type = Some("stdio".to_string());
        }
        self.allow_unset_extension_site();
    }

    pub fn allow_unset_extension_site(&mut self) {
        let Some(wordpress) = self.handlers.wordpress.as_mut() else {
            return;
        };
        if !wordpress.enabled.unwrap_or(false) || !wordpress.sites.is_empty() {
            return;
        }
        let unset = |value: &str| {
            Self::expand_env_vars(value)
                .map(|v| v.trim().is_empty() || v.contains("${user_config."))
                .unwrap_or(true)
        };
        let missing: Vec<&str> = [
            ("the site URL", unset(&wordpress.url)),
            (
                "the username",
                wordpress.keyring.is_none()
                    && wordpress.encrypted_credentials.is_none()
                    && unset(&wordpress.username),
            ),
            (
                "the Application Password",
                wordpress.keyring.is_none()
                    && wordpress.encrypted_credentials.is_none()
                    && unset(&wordpress.password),
            ),
        ]
        .into_iter()
        .filter_map(|(name, missing)| missing.then_some(name))
        .collect();
        if missing.is_empty() {
            return;
        }
        wordpress.enabled = Some(false);
        self.startup_notes.push((
            "wordpress".to_string(),
            format!(
                "not set up: enter {} in Claude Desktop (Settings > Extensions > mcp-rs > Configure), or choose an mcp-rs config file there, then restart the extension",
                missing.join(", ")
            ),
        ));
    }

    /// `read_file`で読んだ設定の、WordPressの`${VAR}`を展開し、資格情報
    /// （`keyring`・`encrypted_credentials`・平文）を解決する。参照した
    /// 環境変数が無ければ、項目と変数名を返す（#454）
    pub fn resolve_wordpress_credentials(&mut self) -> Result<(), String> {
        if let Some(wp_config) = self.handlers.wordpress.as_mut() {
            Self::expand_wordpress_config(wp_config).map_err(|e| e.to_string())?;
            wp_config.resolve_credentials().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 設定ファイルから読み込み、環境変数で上書き。設定ファイルが無ければ
    /// エラー（`mcp-rs check`が使う）
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_with(false)
    }

    /// `--config`無しで起動するときの読み込み（#667）。設定ファイルが無ければ、
    /// エラーで終わらず、設定なし（ハンドラー無し、`mcp_rs_status`だけ）で
    /// 起動できる設定を返し、`missing_config_file`に案内を入れる。STDIO
    /// モードだけ。HTTPモードは、認証等が既定のまま待ち受けないよう、
    /// 今までどおりエラー
    pub fn load_for_startup() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_with(true)
    }

    fn load_with(allow_missing_file: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let mut settings = config::Config::builder();

        // デフォルト値を設定
        let default_config = McpConfig::default();
        settings = settings.add_source(config::Config::try_from(&default_config)?);

        // 設定ファイルは、ユーザーのフォルダの1か所だけから読む（#541）。
        // 以前は今のフォルダの`mcp-config.toml`等も読んでいたので、
        // プロジェクトのフォルダで起動するクライアント（Claude Code等）では、
        // 開いたリポジトリの設定ファイルで、任意のコマンド（プラグイン）を
        // 実行させたり、資格情報を別のサイトへ送らせたりできた
        let home = home_dir_from_env();

        let mut config_file_found = false;
        let mut loaded_config_file = None;
        // `~`はPathが展開しないため、自前でホームフォルダに置き換える。
        // ホームフォルダが分からない場合は、探さない
        if let Some(path) = expand_home_dir(USER_CONFIG_PATH, home.as_deref()) {
            // 権限等で確かめられないときは、無いものとせず、理由を返す（#774。
            // 無いものとすると、設定なしで起動し、理由が分からない）
            let exists = std::path::Path::new(&path)
                .try_exists()
                .map_err(|e| format!("cannot check the config file {path}: {e}"))?;
            if exists {
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

        // 設定ファイルが見つからない場合はエラーを返す（起動では、STDIOモード
        // なら設定なしで続ける、#667）。今のフォルダに以前の名前の設定ファイルが
        // あれば、読まずに、`--config`の付け方を案内する
        let missing_config_file = (!config_file_found && !Self::has_env_config()).then(|| {
            // 今のフォルダ（空のパスに名前をつなぐと、以前と同じ相対パスになる）
            let current = std::path::Path::new("");
            no_config_file_message(home.as_deref(), current, &former_cwd_config_files(current))
        });
        if let Some(message) = &missing_config_file {
            if !allow_missing_file {
                return Err(message.clone().into());
            }
        }

        // 環境変数で上書き (MCP_で始まる変数)
        settings = settings.add_source(
            config::Environment::with_prefix("MCP")
                .separator("_")
                .try_parsing(true),
        );

        // 特定の環境変数も直接対応（後方互換性）
        settings = settings.add_source(config::Environment::default().try_parsing(true));

        // 値の型の誤り等のエラーは、書いた値を含むことがあるので伏せる（#543）
        let config: McpConfig = settings
            .build()
            .and_then(|built| built.try_deserialize())
            .map_err(|e| redact_config_error(&e.to_string()))?;

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
        final_config.resolve_plugin_paths();
        if let Some(message) = missing_config_file {
            if !final_config.uses_stdio() {
                return Err(message.into());
            }
            final_config.missing_config_file = Some(message);
        }
        Ok(final_config)
    }

    /// STDIOモードで動くか。`[transport] type`が`http`以外（知らない値も
    /// STDIOで動く）
    pub fn uses_stdio(&self) -> bool {
        !matches!(
            self.to_transport_config().transport_type,
            crate::transport::TransportType::Http { .. }
        )
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
                check_updates: None,
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
                    trusted_proxies: Some(vec!["127.0.0.1".to_string()]),
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
                    upload_allowed_dirs: None,
                    post_types: None,
                    default_site: None,
                    sites: Default::default(),
                }),
            },
            plugins: Vec::new(),
            audit: None,
            database: None,
            config_file: None,
            startup_notes: Vec::new(),
            missing_config_file: None,
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

    /// ファイルに書く形。`keyring`のサイトは、資格情報ストアから読んだ
    /// ユーザー名・パスワード（`resolve_site_credentials`が入れる）を書かない
    /// （#724。以前は平文で書き、`keyring`と一緒にあるので読み直せなかった）
    fn without_keyring_secrets(&self) -> Self {
        let mut config = self.clone();
        // `encrypted_credentials`から復号した値も、平文で書かない（#755）
        let strip = |site: &mut WordPressConfig| {
            if site.keyring.is_some() || site.encrypted_credentials.is_some() {
                site.username.clear();
                site.password.clear();
            }
        };
        if let Some(wordpress) = config.handlers.wordpress.as_mut() {
            strip(wordpress);
            wordpress.sites.values_mut().for_each(strip);
        }
        config
    }

    /// 設定をファイルに保存
    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let toml_content = toml::to_string_pretty(&self.without_keyring_secrets())?;

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
        use crate::transport::TransportType;

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
                trusted_proxies: http.trusted_proxies.clone().unwrap_or_default(),
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
        // `[[plugins]]`より後の`[plugins.env]`・`[plugins.rate_limit]`も、
        // その`[[plugins]]`の設定（#750）
        let entry_tables = "[[plugins]]\nname = \"a\"\ntype = \"x\"\ntransport_type = \"y\"\n[plugins.env]\nA = \"1\"\n[plugins.rate_limit]\nrequests_per_minute = 1\n";
        assert!(legacy_plugins_conflict_hint(entry_tables).is_none());
        // `[[plugins]]`より前の`[plugins.*]`は古い形式
        assert!(legacy_plugins_conflict_hint(
            "[plugins.isolation]\nenabled = true\n[[plugins]]\nname = \"a\"\n"
        )
        .is_some());
    }

    /// `enabled`・`call_timeout_seconds`等の不正な値（`nan`等）を、省略と
    /// 同じに扱わず、読み込みのエラーにする（#750）
    #[test]
    fn test_invalid_plugin_values_are_rejected() {
        for (field, value) in [
            ("enabled", "nan"),
            ("enabled", "inf"),
            ("enabled", "1"),
            ("call_timeout_seconds", "nan"),
            ("call_timeout_seconds", "-1"),
            ("max_response_bytes", "1.5"),
        ] {
            let content = format!(
                "[transport]\n[server]\n[handlers]\n[[plugins]]\nname = \"x\"\ncommand = \"c\"\n{} = {}\n",
                field, value
            );
            assert!(
                toml::from_str::<McpConfig>(&content).is_err(),
                "{} = {}",
                field,
                value
            );

            let content = normalize_transport_type_alias(&content).unwrap();
            let result = config::Config::builder()
                .add_source(config::Config::try_from(&McpConfig::default()).unwrap())
                .add_source(config::File::from_str(&content, config::FileFormat::Toml))
                .build()
                .unwrap()
                .try_deserialize::<McpConfig>();
            assert!(result.is_err(), "{} = {} (config crate)", field, value);
        }

        // 省略すれば起動し、`false`なら起動しない（今までどおり）
        let config: McpConfig = toml::from_str(
            "[transport]\n[server]\n[handlers]\n[[plugins]]\nname = \"x\"\ncommand = \"c\"\n[[plugins]]\nname = \"y\"\ncommand = \"c\"\nenabled = false\n",
        )
        .unwrap();
        assert_eq!(config.plugins[0].enabled, None);
        assert_eq!(config.plugins[1].enabled, Some(false));
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

    /// SQLiteの`[database]`（#528）。`database`はファイルのパスで、サーバーに
    /// つなぐための項目は書かない。相対パスは設定ファイルのフォルダを基準にする
    #[test]
    fn test_sqlite_database_settings() {
        let config: McpConfig = toml::from_str(
            "[transport]\n[server]\n[handlers]\n[database]\nengine = \"sqlite\"\ndatabase = \"data/app.db\"\n",
        )
        .unwrap();
        let mut db = config.database.unwrap();
        assert!(db.is_sqlite());
        assert!(db.validate().is_ok());
        let base = std::path::Path::new("configs");
        db.resolve_paths(Some(base));
        assert_eq!(
            std::path::PathBuf::from(&db.database),
            base.join("data/app.db")
        );
        assert_eq!(db.describe(), format!("sqlite {} (read-only)", db.database));

        // 配っている設定の例（`configs/database/sqlite.toml`）も、そのまま読めて
        // 検証を通る
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("configs/database/sqlite.toml");
        if let Ok(example) = std::fs::read_to_string(&path) {
            let config: McpConfig =
                toml::from_str(&format!("[transport]\n[server]\n[handlers]\n{example}")).unwrap();
            let example = config.database.unwrap();
            assert!(example.is_sqlite());
            assert!(example.validate().is_ok());
            assert!(!example.allow_write());
        }

        // `:memory:`・URIは、パスにつながずに断る（PR #798のレビュー）
        for uri in [":memory:", "file:app.db?mode=rwc"] {
            let mut memory = DatabaseSettings {
                engine: "sqlite".to_string(),
                database: uri.to_string(),
                ..Default::default()
            };
            memory.resolve_paths(Some(base));
            assert_eq!(memory.database, uri);
            assert!(memory
                .validate()
                .unwrap_err()
                .contains("is not a file path"));
        }

        // `keyring`は、資格情報ストアを読む前に断る（PR #798のレビュー）
        let mut with_keyring = DatabaseSettings {
            engine: "sqlite".to_string(),
            database: "/srv/app.db".to_string(),
            keyring: Some("app".to_string()),
            ..Default::default()
        };
        let store = crate::security::keyring_store::MemoryStore::default();
        let err =
            McpConfig::resolve_database_credentials_with(&mut with_keyring, &store).unwrap_err();
        assert!(err.contains("database.keyring is not used"), "{err}");

        // 環境変数で渡すパスは、展開の後の値をそのまま使う
        let mut from_env = DatabaseSettings {
            engine: "sqlite".to_string(),
            database: "${APP_DB}".to_string(),
            ..Default::default()
        };
        from_env.resolve_paths(Some(base));
        assert_eq!(from_env.database, "${APP_DB}");

        let valid = DatabaseSettings {
            engine: "SQLite".to_string(),
            database: "/srv/app.db".to_string(),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
        for (invalid, expected) in [
            (
                DatabaseSettings {
                    database: " ".to_string(),
                    ..valid.clone()
                },
                "database.database is empty",
            ),
            (
                DatabaseSettings {
                    database: ":memory:".to_string(),
                    ..valid.clone()
                },
                "is not a file path",
            ),
            (
                DatabaseSettings {
                    database: "file:app.db?mode=rwc".to_string(),
                    ..valid.clone()
                },
                "is not a file path",
            ),
            (
                DatabaseSettings {
                    host: Some("db.example.com".to_string()),
                    ..valid.clone()
                },
                "database.host is not used",
            ),
            (
                DatabaseSettings {
                    username: "reader".to_string(),
                    ..valid.clone()
                },
                "database.username is not used",
            ),
            (
                DatabaseSettings {
                    password: Some("x".to_string()),
                    ..valid.clone()
                },
                "database.password is not used",
            ),
            (
                DatabaseSettings {
                    ssl_mode: Some("require".to_string()),
                    ..valid.clone()
                },
                "database.ssl_mode is not used",
            ),
        ] {
            let err = invalid.validate().unwrap_err();
            assert!(err.contains(expected), "{err}");
        }
    }

    /// Redisの`[database]`（#529）。`database`はデータベースの番号（省略時は0）、
    /// `username`は省略できる。`ssl_mode`は`disable`・`require`・`verify-full`
    #[test]
    fn test_redis_database_settings() {
        let config: McpConfig = toml::from_str(
            "[transport]\n[server]\n[handlers]\n[database]\nengine = \"redis\"\nhost = \"localhost\"\n",
        )
        .unwrap();
        let db = config.database.unwrap();
        assert!(db.is_redis());
        assert!(db.validate().is_ok());
        assert_eq!(db.port(), 6379);
        assert_eq!(db.redis_database(), Some(0));
        assert_eq!(db.describe(), "redis localhost:6379/0 (read-only)");
        let with_user = DatabaseSettings {
            engine: "redis".to_string(),
            database: "3".to_string(),
            username: "reader".to_string(),
            ..Default::default()
        };
        assert_eq!(
            with_user.describe(),
            "redis reader@localhost:6379/3 (read-only)"
        );
        for (invalid, expected) in [
            (
                DatabaseSettings {
                    database: "app".to_string(),
                    ..with_user.clone()
                },
                "not a Redis database number",
            ),
            (
                DatabaseSettings {
                    ssl_mode: Some("prefer".to_string()),
                    ..with_user.clone()
                },
                "not valid for Redis",
            ),
            (
                DatabaseSettings {
                    ssl_mode: Some("require".to_string()),
                    ssl_root_cert: Some("ca.pem".to_string()),
                    ..with_user.clone()
                },
                "only with ssl_mode",
            ),
        ] {
            let err = invalid.validate().unwrap_err();
            assert!(err.contains(expected), "{err}");
        }
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("configs/database/redis.toml");
        if let Ok(example) = std::fs::read_to_string(&path) {
            let config: McpConfig =
                toml::from_str(&format!("[transport]\n[server]\n[handlers]\n{example}")).unwrap();
            let example = config.database.unwrap();
            assert!(example.is_redis());
            assert!(example.validate().is_ok());
        }
    }

    /// MongoDBの`[database]`（#530）。`database`はデータベースの名前、`auth_source`は
    /// `username`があるときだけ。`ssl_mode`は`disable`・`require`・`verify-full`
    #[test]
    fn test_mongodb_database_settings() {
        let config: McpConfig = toml::from_str(
            "[transport]\n[server]\n[handlers]\n[database]\nengine = \"mongodb\"\ndatabase = \"app\"\n",
        )
        .unwrap();
        let db = config.database.unwrap();
        assert!(db.is_mongodb());
        assert!(db.validate().is_ok());
        assert_eq!(db.port(), 27017);
        assert_eq!(db.describe(), "mongodb localhost:27017/app (read-only)");
        let with_user = DatabaseSettings {
            engine: "mongodb".to_string(),
            database: "app".to_string(),
            username: "reader".to_string(),
            auth_source: Some("app".to_string()),
            ..Default::default()
        };
        assert!(with_user.validate().is_ok());
        assert_eq!(
            with_user.describe(),
            "mongodb reader@localhost:27017/app (read-only)"
        );
        // Windowsのサーバーでだけ使えない文字は、サーバーに任せる（#808）
        assert!(DatabaseSettings {
            database: "sales:2026".to_string(),
            ..with_user.clone()
        }
        .validate()
        .is_ok());
        for (invalid, expected) in [
            (
                DatabaseSettings {
                    database: String::new(),
                    ..with_user.clone()
                },
                "database.database is empty",
            ),
            (
                DatabaseSettings {
                    database: "a.b".to_string(),
                    ..with_user.clone()
                },
                "not a MongoDB database name",
            ),
            (
                DatabaseSettings {
                    username: String::new(),
                    ..with_user.clone()
                },
                "auth_source is used only with username",
            ),
            (
                DatabaseSettings {
                    ssl_mode: Some("prefer".to_string()),
                    ..with_user.clone()
                },
                "not valid for MongoDB",
            ),
            (
                DatabaseSettings {
                    engine: "postgresql".to_string(),
                    ..with_user.clone()
                },
                "auth_source is used only with engine = \"mongodb\"",
            ),
            (
                DatabaseSettings {
                    engine: "redis".to_string(),
                    ..with_user.clone()
                },
                "auth_source is used only with engine = \"mongodb\"",
            ),
            (
                DatabaseSettings {
                    engine: "sqlite".to_string(),
                    ..with_user.clone()
                },
                "auth_source is used only with engine = \"mongodb\"",
            ),
        ] {
            let err = invalid.validate().unwrap_err();
            assert!(err.contains(expected), "{err}");
        }
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("configs/database/mongodb.toml");
        if let Ok(example) = std::fs::read_to_string(&path) {
            let config: McpConfig =
                toml::from_str(&format!("[transport]\n[server]\n[handlers]\n{example}")).unwrap();
            let example = config.database.unwrap();
            assert!(example.is_mongodb());
            assert!(example.validate().is_ok());
        }
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
                    engine: "sqlite3".to_string(),
                    ..valid.clone()
                },
                "not supported",
            ),
            (
                DatabaseSettings {
                    engine: "mysql".to_string(),
                    ssl_mode: Some("allow".to_string()),
                    ..valid.clone()
                },
                "ssl_mode",
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
        // MySQL（#527）: 既定のポートは3306。`allow`以外のssl_modeを使える
        let mysql = DatabaseSettings {
            engine: "MySQL".to_string(),
            ssl_mode: Some("verify-full".to_string()),
            ..valid.clone()
        };
        assert!(mysql.validate().is_ok());
        assert!(mysql.is_mysql());
        assert_eq!(mysql.port(), 3306);
        assert_eq!(valid.port(), 5432);
        assert!(mysql
            .describe()
            .starts_with("MySQL reader@localhost:3306/app"));
        // MariaDB（#527）も、MySQLと同じ既定・ssl_mode
        let mariadb = DatabaseSettings {
            engine: "mariadb".to_string(),
            ..valid.clone()
        };
        assert!(mariadb.validate().is_ok());
        assert!(mariadb.is_mysql() && mariadb.is_mariadb());
        assert!(!mysql.is_mariadb());
        assert_eq!(mariadb.port(), 3306);
        let mariadb_allow = DatabaseSettings {
            ssl_mode: Some("allow".to_string()),
            ..mariadb
        };
        assert!(mariadb_allow.validate().is_err());
        // verify-ca（#594）: PostgreSQLでは使えない。MySQL・MariaDBでは
        // ssl_root_certが要る
        let verify_ca = |engine: &str, cert: Option<&str>| DatabaseSettings {
            engine: engine.to_string(),
            ssl_mode: Some("verify-ca".to_string()),
            ssl_root_cert: cert.map(str::to_string),
            ..valid.clone()
        };
        let err = verify_ca("postgresql", Some("ca.pem"))
            .validate()
            .unwrap_err();
        assert!(err.contains("verify-full"), "{err}");
        for engine in ["mysql", "mariadb"] {
            let err = verify_ca(engine, None).validate().unwrap_err();
            assert!(err.contains("needs ssl_root_cert"), "{err}");
            assert!(verify_ca(engine, Some("ca.pem")).validate().is_ok());
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
        std::env::set_var("MCP_RS_TEST_DB_CA_822", "/etc/ssl/db-ca.pem");
        let mut db = DatabaseSettings {
            engine: "postgresql".to_string(),
            database: "app".to_string(),
            username: "reader".to_string(),
            password: Some("${MCP_RS_TEST_DB_PASSWORD_453}".to_string()),
            ssl_root_cert: Some("${MCP_RS_TEST_DB_CA_822}".to_string()),
            ..Default::default()
        };
        McpConfig::expand_database_config(&mut db).unwrap();
        assert_eq!(db.password.as_deref(), Some("from-env"));
        assert_eq!(db.ssl_root_cert.as_deref(), Some("/etc/ssl/db-ca.pem"));

        // 無い変数は、項目と変数の名前を返す（値は出さない）。テストを
        // 動かす環境に同じ名前があっても、無い状態にする（#737。名前は
        // このテストだけが使う）
        std::env::remove_var("MCP_RS_TEST_DB_MISSING_453");
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

    /// `@read_only`を書いたかを見分ける（PR #604のレビュー）
    #[test]
    fn test_uses_tool_preset() {
        let keys: HttpTransportConfig = toml::from_str(
            r#"
[[api_keys]]
key = "k"
name = "reader"
user_id = "u"
tools = ["@read_only"]
"#,
        )
        .unwrap();
        assert!(keys.uses_tool_preset("@read_only"));
        let scopes: HttpTransportConfig = toml::from_str(
            r#"
[oauth]
issuer = "https://auth.example.com"
resource = "https://mcp.example.com/mcp"
[oauth.scope_tools]
"mcp:read" = ["get_*"]
"mcp:list" = [" @read_only "]
"#,
        )
        .unwrap();
        assert!(scopes.uses_tool_preset("@read_only"));
        let names_only: HttpTransportConfig = toml::from_str(
            r#"
[[api_keys]]
key = "k"
name = "writer"
user_id = "u"
tools = ["get_posts"]
"#,
        )
        .unwrap();
        assert!(!names_only.uses_tool_preset("@read_only"));
        let empty: HttpTransportConfig = toml::from_str("").unwrap();
        assert!(!empty.uses_tool_preset("@read_only"));
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

    /// `encrypted_credentials`があれば、上書きされる`username`・`password`は
    /// 展開しない。`url`は展開する（#746）
    #[test]
    fn test_expand_wordpress_config_skips_credentials_with_encrypted_credentials() {
        let mut wp: WordPressConfig = toml::from_str(
            r#"
            url = "${WP_URL_NOT_SET_746}"
            username = "${WP_USERNAME_NOT_SET_746}"
            password = "${WP_PASSWORD_NOT_SET_746}"
            enabled = true

            [encrypted_credentials]
            username = "admin"
            encrypted_password = "x"
            nonce = "x"
            salt = "x"
            "#,
        )
        .unwrap();
        let err = McpConfig::expand_wordpress_config(&mut wp).unwrap_err();
        assert_eq!(err.field, "handlers.wordpress.url");
        wp.url = "https://example.com".to_string();
        assert!(McpConfig::expand_wordpress_config(&mut wp).is_ok());
        assert_eq!(wp.password, "${WP_PASSWORD_NOT_SET_746}");
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

    /// v0.23.0までの`[transport.stdio]`は、書いてあっても読み込める（無視
    /// する、#557）
    #[test]
    fn test_former_transport_stdio_section_is_ignored() {
        let config: McpConfig = toml::from_str(
            "[server]\n[transport]\ntype = \"stdio\"\n\n[transport.stdio]\nbuffer_size = 8192\nframing_method = \"content-length\"\n\n[handlers]\n",
        )
        .unwrap();
        assert_eq!(config.transport.transport_type.as_deref(), Some("stdio"));
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

    /// `keyring`のサイトは、資格情報ストアから読んだユーザー名・パスワードを
    /// ファイルに書かない（#724）
    #[test]
    fn test_save_to_file_leaves_out_keyring_secrets() {
        let site = |keyring: Option<&str>| WordPressConfig {
            url: "https://example.com".to_string(),
            username: "wp-user".to_string(),
            password: "s3cret-from-keyring".to_string(),
            enabled: Some(true),
            timeout_seconds: None,
            rate_limit: None,
            encrypted_credentials: None,
            keyring: keyring.map(str::to_string),
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
        };
        let mut config = McpConfig::default();
        config.handlers.wordpress = Some(site(Some("example.com")));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        config.save_to_file(path.to_str().unwrap()).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains("s3cret-from-keyring"), "{written}");
        assert!(!written.contains("wp-user"), "{written}");
        assert!(written.contains("keyring = \"example.com\""), "{written}");
        // 書いたものを読み直せる
        McpConfig::read_file(path.to_str().unwrap()).unwrap();
        // `keyring`の無いサイトは、今までどおり書く
        let mut sites = site(None);
        sites.url.clear();
        sites.username.clear();
        sites.password.clear();
        sites.sites.insert("blog".to_string(), site(Some("blog")));
        sites.sites.insert("plain".to_string(), site(None));
        config.handlers.wordpress = Some(sites);
        let saved = config.without_keyring_secrets();
        let wordpress = saved.handlers.wordpress.unwrap();
        assert!(wordpress.sites["blog"].password.is_empty());
        assert_eq!(wordpress.sites["plain"].password, "s3cret-from-keyring");
    }

    /// `encrypted_credentials`から復号したユーザー名・パスワードも、
    /// ファイルに書かない（#755）
    #[test]
    fn test_save_to_file_leaves_out_decrypted_secrets() {
        let encrypted = SecureCredentials::new("wp-user".to_string(), "decrypted-pw".to_string())
            .encrypt("correct-master-password")
            .unwrap();
        let mut config = McpConfig::default();
        config.handlers.wordpress = Some(WordPressConfig {
            url: "https://example.com".to_string(),
            username: "wp-user".to_string(),
            password: "decrypted-pw".to_string(),
            enabled: Some(true),
            timeout_seconds: None,
            rate_limit: None,
            encrypted_credentials: Some(encrypted),
            keyring: None,
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
        });
        let saved = config.without_keyring_secrets();
        let wordpress = saved.handlers.wordpress.unwrap();
        assert!(wordpress.username.is_empty());
        assert!(wordpress.password.is_empty());
        assert!(wordpress.encrypted_credentials.is_some());
    }

    /// 使わない設定（`enabled = false`・省略）では、`encrypted_credentials`を
    /// 復号しない。`WORDPRESS_MASTER_PASSWORD`が無くても読み込める（#755）
    #[test]
    fn test_disabled_wordpress_does_not_decrypt_encrypted_credentials() {
        let _env_guard = WORDPRESS_MASTER_PASSWORD_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("WORDPRESS_MASTER_PASSWORD");
        }
        let encrypted = SecureCredentials::new("wp-user".to_string(), "pw".to_string())
            .encrypt("correct-master-password")
            .unwrap();
        for enabled in [Some(false), None] {
            let mut config = WordPressConfig {
                url: "https://example.com".to_string(),
                username: String::new(),
                password: String::new(),
                enabled,
                timeout_seconds: None,
                rate_limit: None,
                encrypted_credentials: Some(encrypted.clone()),
                keyring: None,
                upload_allowed_dirs: None,
                post_types: None,
                default_site: None,
                sites: Default::default(),
            };
            config.resolve_credentials().unwrap();
            assert!(config.password.is_empty());
        }
        // 使う設定では、今までどおり鍵が無ければエラー
        let mut config = WordPressConfig {
            url: "https://example.com".to_string(),
            username: String::new(),
            password: String::new(),
            enabled: Some(true),
            timeout_seconds: None,
            rate_limit: None,
            encrypted_credentials: Some(encrypted),
            keyring: None,
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
        };
        assert!(config.resolve_credentials().is_err());
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
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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
            upload_allowed_dirs: None,
            post_types: None,
            default_site: None,
            sites: Default::default(),
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

    // ---- #640: 複数のWordPressのサイト ----

    fn sites_config(toml_str: &str) -> WordPressConfig {
        toml::from_str(toml_str).unwrap()
    }

    const TWO_SITES: &str = "enabled = true\ndefault_site = \"shop\"\ntimeout_seconds = 60\n\
         upload_allowed_dirs = [\"/shared\"]\npost_types = [\"product\"]\n\
         [sites.blog]\nurl = \"https://blog.example.com\"\nusername = \"u\"\npassword = \"p\"\n\
         [sites.shop]\nurl = \"https://shop.example.com\"\nusername = \"u\"\npassword = \"p\"\ntimeout_seconds = 5\n\
         upload_allowed_dirs = [\"/shop\"]\npost_types = [\"event\"]\n\
         [sites.old]\nurl = \"https://old.example.com\"\nenabled = false\n";

    /// 既定のサイトが最初、残りは名前の順。`enabled = false`のサイトは使わない。
    /// 共通の設定は、サイトに無ければ`[handlers.wordpress]`のもの
    #[test]
    fn test_site_list_order_default_and_shared_settings() {
        let list = sites_config(TWO_SITES).site_list().unwrap();
        let names: Vec<_> = list.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["shop", "blog"]);
        let (shop, blog) = (&list[0].1, &list[1].1);
        assert_eq!(shop.timeout_seconds, Some(5));
        assert_eq!(blog.timeout_seconds, Some(60));
        assert_eq!(
            shop.upload_allowed_dirs.as_deref(),
            Some(&["/shop".to_string()][..])
        );
        assert_eq!(
            blog.upload_allowed_dirs.as_deref(),
            Some(&["/shared".to_string()][..])
        );
        assert_eq!(shop.post_types.as_deref(), Some(&["event".to_string()][..]));
        assert_eq!(
            blog.post_types.as_deref(),
            Some(&["product".to_string()][..])
        );
        assert!(list
            .iter()
            .all(|(_, c)| c.enabled == Some(true) && c.sites.is_empty()));

        // 使うサイトが1つなら、default_siteは要らない
        let one =
            sites_config("enabled = true\n[sites.blog]\nurl = \"https://blog.example.com\"\n");
        assert_eq!(one.site_list().unwrap()[0].0, "blog");

        // 今までの形（sitesが無い）は、defaultという名前の1つ
        let single = sites_config("url = \"https://blog.example.com\"\nenabled = true\n");
        let list = single.site_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "default");
    }

    /// サイトに`[]`と書けば、`[handlers.wordpress]`のものを受け継がない。
    /// 省けば受け継ぐ（#684）
    #[test]
    fn test_site_list_empty_list_does_not_inherit() {
        let config = sites_config(
            "enabled = true
default_site = \"blog\"
             upload_allowed_dirs = [\"/shared\"]
post_types = [\"product\"]
             [sites.blog]
url = \"https://blog.example.com\"
             [sites.shop]
url = \"https://shop.example.com\"
             upload_allowed_dirs = []
post_types = []
",
        );
        let list = config.site_list().unwrap();
        let (blog, shop) = (&list[0].1, &list[1].1);
        assert_eq!(list[1].0, "shop");
        assert_eq!(shop.upload_allowed_dirs.as_deref(), Some(&[][..]));
        assert_eq!(shop.post_types.as_deref(), Some(&[][..]));
        assert_eq!(
            blog.upload_allowed_dirs.as_deref(),
            Some(&["/shared".to_string()][..])
        );
        assert_eq!(
            blog.post_types.as_deref(),
            Some(&["product".to_string()][..])
        );

        // 今までの形（sitesが無い）は、省いても`[]`でも、使えない
        for extra in [
            "",
            "upload_allowed_dirs = []
post_types = []
",
        ] {
            let single = sites_config(&format!(
                "url = \"https://blog.example.com\"
enabled = true
{extra}"
            ));
            let (_, site) = single.site_list().unwrap().remove(0);
            assert!(site.upload_allowed_dirs.unwrap_or_default().is_empty());
            assert!(site.post_types.unwrap_or_default().is_empty());
        }
    }

    /// 使わないサイトの`encrypted_credentials`は復号しない。マスター
    /// パスワードが無くても、ほかのサイトは読める（#684）
    #[test]
    fn test_resolve_credentials_skips_disabled_sites() {
        let _env_guard = WORDPRESS_MASTER_PASSWORD_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("WORDPRESS_MASTER_PASSWORD");
        }
        let encrypted = SecureCredentials::new("old-user".to_string(), "old-pass".to_string())
            .encrypt("master")
            .unwrap();
        let mut config = sites_config(
            "enabled = true
default_site = \"blog\"
             [sites.blog]
url = \"https://blog.example.com\"
username = \"u\"
password = \"p\"
             [sites.old]
url = \"https://old.example.com\"
enabled = false
",
        );
        config.sites.get_mut("old").unwrap().encrypted_credentials = Some(encrypted);
        let store = crate::security::keyring_store::MemoryStore::default();
        config.resolve_credentials_with(&store).unwrap();
        assert_eq!(config.sites["blog"].username, "u");
        assert_eq!(config.sites["old"].username, "");
        let list = config.site_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "blog");

        // 使うサイトなら、今までどおり復号できなければ誤り
        config.sites.get_mut("old").unwrap().enabled = None;
        let err = config
            .resolve_credentials_with(&store)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("handlers.wordpress.sites.old: "), "{err}");
    }

    #[test]
    fn test_site_list_refuses_unclear_layouts() {
        for (toml_str, message) in [
            (
                "enabled = true\n[sites.blog]\nurl = \"b\"\n[sites.shop]\nurl = \"s\"\n",
                "default_site is required with more than one site: write default_site = \"<site>\", one of: blog, shop",
            ),
            (
                "enabled = true\ndefault_site = \"other\"\n[sites.blog]\nurl = \"b\"\n",
                "'other' is not a site in use. Use one of: blog",
            ),
            (
                "enabled = true\ndefault_site = \"old\"\n[sites.blog]\nurl = \"b\"\n[sites.old]\nurl = \"o\"\nenabled = false\n",
                "'old' is not a site in use",
            ),
            (
                "url = \"https://a\"\nenabled = true\ndefault_site = \"blog\"\n",
                "there is no [handlers.wordpress.sites.blog]",
            ),
            (
                "url = \"https://a\"\nkeyring = \"a\"\nenabled = true\n[sites.blog]\nurl = \"b\"\n",
                "write url, keyring in each site's table, not in [handlers.wordpress]",
            ),
            (
                "enabled = true\n[sites.\"My Blog\"]\nurl = \"b\"\n",
                "handlers.wordpress.sites.My Blog: WordPress site name 'My Blog' is not valid",
            ),
            (
                "enabled = true\n[sites.blog]\nurl = \"b\"\ndefault_site = \"x\"\n",
                "default_site and sites can be written only in [handlers.wordpress]",
            ),
            (
                "enabled = true\n[sites.blog]\nurl = \"b\"\n[sites.blog.sites.more]\nurl = \"m\"\n",
                "default_site and sites can be written only in [handlers.wordpress]",
            ),
            (
                "enabled = true\n[sites.blog]\nusername = \"u\"\n",
                "handlers.wordpress.sites.blog.url is not set",
            ),
            (
                "enabled = true\n[sites.blog]\nurl = \"b\"\nenabled = false\n",
                "every site in [handlers.wordpress.sites.<site>] has enabled = false",
            ),
            ("enabled = true\n", "handlers.wordpress.url is not set"),
        ] {
            let err = sites_config(toml_str).site_list().unwrap_err();
            assert!(err.contains(message), "{toml_str}: {err}");
        }
    }

    /// サイトごとに、自分の`keyring`を読む。誤りには、どのサイトかを出す
    #[test]
    fn test_resolve_credentials_reads_each_site() {
        use crate::security::keyring_store::{
            store_wordpress_credentials, WordPressKeyringCredentials,
        };
        let store = store_with_example();
        store_wordpress_credentials(
            &store,
            "shop",
            &WordPressKeyringCredentials {
                site_url: "https://shop.example.com".to_string(),
                username: "shopkeeper".to_string(),
                application_password: zeroize::Zeroizing::new("shop pass".to_string()),
            },
        )
        .unwrap();
        let two = |shop: &str| {
            sites_config(&format!(
                "enabled = true\ndefault_site = \"blog\"\n\
                 [sites.blog]\nurl = \"https://example.com\"\nkeyring = \"example.com\"\n\
                 [sites.shop]\n{shop}"
            ))
        };
        let mut config = two("url = \"https://shop.example.com\"\nkeyring = \"shop\"\n");
        config.resolve_credentials_with(&store).unwrap();
        assert_eq!(config.sites["blog"].username, "editor");
        let shop = &config.sites["shop"];
        assert_eq!(shop.username, "shopkeeper");
        assert_eq!(shop.password, "shop pass");

        // 資格情報を保存したサイトと`url`が違えば、使わない（#541と同じ）
        let mut wrong = two("url = \"https://evil.example.com\"\nkeyring = \"shop\"\n");
        let err = wrong
            .resolve_credentials_with(&store)
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("handlers.wordpress.sites.shop.keyring: "),
            "{err}"
        );

        // 資格情報の無いサイト
        let mut missing = two("url = \"https://shop.example.com\"\n");
        let err = missing
            .resolve_credentials_with(&store)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("handlers.wordpress.sites.shop: "), "{err}");

        // 使わないサイト・全体を使わない設定は、読まない
        let mut disabled =
            two("url = \"https://shop.example.com\"\nenabled = false\nkeyring = \"none\"\n");
        disabled.resolve_credentials_with(&store).unwrap();
        let mut off = two("url = \"https://shop.example.com\"\nkeyring = \"none\"\n");
        off.enabled = Some(false);
        off.resolve_credentials_with(&store).unwrap();

        // [handlers.wordpress]にサイトの項目を書いた
        let mut mixed = two("url = \"https://shop.example.com\"\nkeyring = \"shop\"\n");
        mixed.url = "https://example.com".to_string();
        let err = mixed
            .resolve_credentials_with(&store)
            .unwrap_err()
            .to_string();
        assert!(err.contains("write url in each site's table"), "{err}");
    }

    /// サイトの`${VAR}`も展開し、誤りにはサイトの表の名前を出す
    #[test]
    fn test_expand_wordpress_config_expands_sites() {
        let mut config = sites_config(
            "enabled = true\ndefault_site = \"blog\"\n\
             [sites.blog]\nurl = \"https://blog.example.com\"\n\
             [sites.shop]\nurl = \"https://shop.example.com\"\npassword = \"${MCP_RS_TEST_UNSET_640}\"\n",
        );
        let err = McpConfig::expand_wordpress_config(&mut config).unwrap_err();
        assert_eq!(err.field, "handlers.wordpress.sites.shop.password");
    }

    /// `WORDPRESS_URL`等は、複数のサイトの設定を上書きしない
    #[test]
    fn test_env_overrides_leave_sites_alone() {
        let mut wordpress = Some(sites_config(TWO_SITES));
        apply_wordpress_env_overrides(&mut wordpress, |name| match name {
            "WORDPRESS_URL" => Some("https://evil.example.com".to_string()),
            _ => Some("x".to_string()),
        });
        let config = wordpress.unwrap();
        assert!(config.url.is_empty());
        assert_eq!(config.sites["blog"].url, "https://blog.example.com");
    }

    /// 今のフォルダの設定ファイルは読まず、見つけたら`--config`を案内する（#541）
    #[test]
    fn test_former_cwd_config_files_are_only_mentioned() {
        let dir = tempfile::tempdir().unwrap();
        assert!(former_cwd_config_files(dir.path()).is_empty());
        let message = no_config_file_message(Some("/home/u"), dir.path(), &[]);
        assert!(message.contains("--config <absolute path>"), "{message}");
        assert!(message.contains("`mcp-rs init`"), "{message}");
        assert!(message.contains("config.toml"), "{message}");
        assert!(!message.contains("current folder"), "{message}");

        std::fs::write(dir.path().join("config.toml"), "").unwrap();
        std::fs::create_dir(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config").join("mcp.toml"), "").unwrap();
        let former = former_cwd_config_files(dir.path());
        assert_eq!(former, vec!["config.toml", "config/mcp.toml"]);
        let message = no_config_file_message(Some("/home/u"), dir.path(), &former);
        assert!(
            message.contains("is not read without --config"),
            "{message}"
        );
        // 案内は絶対パス（相対パスは、起動したフォルダから読むため）。見つけた
        // フォルダのファイルを指す（#724。以前は作業フォルダから作ったので、
        // 確かめたいことを確かめていなかった）
        let absolute = std::path::absolute(dir.path().join("config.toml")).unwrap();
        assert!(
            message.contains(&format!("--config {}", absolute.display())),
            "{message}"
        );
        assert!(!message.contains("--config config.toml"), "{message}");
    }

    /// TOMLの誤りのエラーに、行の内容（パスワード）を出さない（#543）
    #[test]
    fn test_toml_errors_do_not_show_the_line() {
        let content =
            "[handlers.wordpress]\nurl = \"https://example.com\"\npassword = \"s3cret-pw\"x\n";
        let error = toml::from_str::<McpConfig>(content).unwrap_err();
        assert!(
            error.to_string().contains("s3cret-pw"),
            "toml quotes the line"
        );
        let message = describe_toml_error(&error, content);
        assert!(message.contains("line 3"), "{message}");
        assert!(!message.contains("s3cret-pw"), "{message}");

        // 型の誤りのエラーも、値を伏せる
        let content = "[transport.http]\nport = \"s3cret-pw\"\n";
        let error = toml::from_str::<McpConfig>(content).unwrap_err();
        let message = describe_toml_error(&error, content);
        assert!(!message.contains("s3cret-pw"), "{message}");
        assert!(message.contains("line 2"), "{message}");

        assert_eq!(
            redact_quoted(r#"invalid type: string "x", expected u16"#),
            r#"invalid type: string "…", expected u16"#
        );
        // 値の中の`"`は`\"`と表示される。その後ろも伏せる（PR #552のレビュー）
        assert_eq!(
            redact_quoted(r#"invalid type: string "ab\"cd-secret", expected u16"#),
            r#"invalid type: string "…", expected u16"#
        );
        // 英単語の中の`'`で、後ろを全部伏せない
        assert_eq!(
            redact_quoted("can't parse 'x' here"),
            "can't parse '…' here"
        );

        // 実際のエラーでも、`"`を含むパスワードを出さない
        let content = "[transport.http]\nport = 'ab\"cd-secret'\n";
        let error = toml::from_str::<McpConfig>(content).unwrap_err();
        let message = describe_toml_error(&error, content);
        assert!(!message.contains("cd-secret"), "{message}");
    }

    /// 設定なしで起動するのはSTDIOモードだけ（#667）。HTTPモードかの判定
    #[test]
    fn test_uses_stdio() {
        let mut config = McpConfig::default();
        assert!(config.uses_stdio());
        config.transport.transport_type = Some("unknown".to_string());
        assert!(config.uses_stdio());
        config.transport.transport_type = Some("http".to_string());
        assert!(!config.uses_stdio());
    }

    /// 足りない項目・知らない項目の名前は伏せない（#617）
    #[test]
    fn test_config_errors_show_field_names() {
        // `[handlers]`が無い
        let content = "[server]\n[transport]\n";
        let error = toml::from_str::<McpConfig>(content).unwrap_err();
        let message = describe_toml_error(&error, content);
        assert!(message.contains("missing field `handlers`"), "{message}");

        assert_eq!(
            redact_config_error("missing field `handlers`"),
            "missing field `handlers`"
        );
        // 知らない項目は、名前と候補を出す
        assert_eq!(
            redact_config_error("unknown field `pasword`, expected one of `url`, `password`"),
            "unknown field `pasword`, expected one of `url`, `password`"
        );
        assert_eq!(
            redact_config_error("unknown field `x`, there are no fields"),
            "unknown field `x`, there are no fields"
        );
        // ユーザーが書いた名前は、制御文字をエスケープし、長さを切る
        let long = "a".repeat(100);
        let message = redact_config_error(&format!("unknown field `{long}\u{1b}[31m`"));
        assert!(
            message.contains(&format!("`{}…`", "a".repeat(64))),
            "{message}"
        );
        let message = redact_config_error("unknown field `a\u{1b}[31m\nb`, expected `url`");
        assert_eq!(message, "unknown field `a\\u{1b}[31m\\nb`, expected `url`");

        // 値は今までどおり伏せる。値の中に同じ文字列があっても、出さない
        assert_eq!(
            redact_config_error(r#"invalid type: string "missing field `s3cret`", expected u16"#),
            r#"invalid type: string "…", expected u16"#
        );
        assert_eq!(
            redact_config_error(r#"missing field `handlers` for key "s3cret""#),
            r#"missing field `handlers` for key "…""#
        );
        let content = "[transport.http]\nport = \"s3cret-pw\"\n";
        let error = toml::from_str::<McpConfig>(content).unwrap_err();
        assert!(!describe_toml_error(&error, content).contains("s3cret-pw"));

        // `--config`無し（configクレート、`McpConfig::load`）のエラーも同じ
        let content = "[[plugins]]\ncommand = \"s3cret-cmd\"\n";
        let error = config::Config::builder()
            .add_source(config::Config::try_from(&McpConfig::default()).unwrap())
            .add_source(config::File::from_str(content, config::FileFormat::Toml))
            .build()
            .and_then(|built| built.try_deserialize::<McpConfig>())
            .unwrap_err();
        let message = redact_config_error(&error.to_string());
        assert!(
            message.contains("missing field `name` for key `plugins`"),
            "{message}"
        );
        assert!(!message.contains("s3cret-cmd"), "{message}");

        // 型の誤りでも、項目の場所は残し、値は伏せる（#724）
        assert_eq!(
            redact_config_error(
                r#"invalid type: string "s3cret", expected u16 for key `transport.http.port`"#
            ),
            r#"invalid type: string "…", expected u16 for key `transport.http.port`"#
        );
        // 項目の名前に使わない文字があれば、伏せる
        assert_eq!(
            redact_config_error(r#"invalid type: string "a", expected u16 for key `s3 cret`"#),
            r#"invalid type: string "…", expected u16 for key `…`"#
        );
        let content = "[transport.http]\nport = \"s3cret-pw\"\n";
        let error = config::Config::builder()
            .add_source(config::Config::try_from(&McpConfig::default()).unwrap())
            .add_source(config::File::from_str(content, config::FileFormat::Toml))
            .build()
            .and_then(|built| built.try_deserialize::<McpConfig>())
            .unwrap_err();
        let message = redact_config_error(&error.to_string());
        assert!(message.contains("transport.http.port"), "{message}");
        assert!(!message.contains("s3cret-pw"), "{message}");
    }

    /// ホームフォルダは、空の`HOME`を飛ばして`USERPROFILE`を見る（#710）
    #[test]
    fn test_home_dir_from() {
        let env = |home: Option<&str>, profile: Option<&str>| {
            let home = home.map(str::to_string);
            let profile = profile.map(str::to_string);
            move |name: &str| match name {
                "HOME" => home.clone(),
                "USERPROFILE" => profile.clone(),
                _ => None,
            }
        };
        assert_eq!(
            home_dir_from(env(Some("/home/me"), Some(r"C:\Users\me"))),
            Some("/home/me".to_string())
        );
        assert_eq!(
            home_dir_from(env(Some(""), Some(r"C:\Users\me"))),
            Some(r"C:\Users\me".to_string())
        );
        assert_eq!(
            home_dir_from(env(None, Some(r"C:\Users\me"))),
            Some(r"C:\Users\me".to_string())
        );
        assert_eq!(home_dir_from(env(Some(" "), Some(""))), None);
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
        // Windowsでは`~\`も展開し、`\`を区切り文字として扱う
        let expanded = expand_config_path_for("~\\mcp-rs\\config.toml", Some(&home_str), true);
        assert_eq!(
            std::path::PathBuf::from(&expanded),
            home.join("mcp-rs").join("config.toml")
        );
        // Unixでは、`\`は名前の文字なので変えない（#724）
        assert_eq!(
            expand_config_path_for("~\\mcp-rs\\config.toml", Some("/home/me"), false),
            "~\\mcp-rs\\config.toml"
        );
        assert_eq!(
            std::path::PathBuf::from(expand_config_path_for(
                "~/a\\b.toml",
                Some("/home/me"),
                false
            )),
            std::path::Path::new("/home/me").join("a\\b.toml")
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

    /// `[database]`の`keyring`から、パスワードを読む（#590）
    #[test]
    fn test_resolve_database_credentials_with_keyring() {
        use crate::security::keyring_store::{
            store_database_credentials, DatabaseKeyringCredentials, MemoryStore,
        };
        let store = MemoryStore::default();
        store_database_credentials(
            &store,
            "app@db",
            &DatabaseKeyringCredentials {
                host: "db.example.com".to_string(),
                port: 5432,
                username: "reader".to_string(),
                password: zeroize::Zeroizing::new("s3cret".to_string()),
            },
        )
        .unwrap();
        let settings = || DatabaseSettings {
            engine: "postgresql".to_string(),
            host: Some("db.example.com".to_string()),
            database: "app".to_string(),
            username: "reader".to_string(),
            keyring: Some("app@db".to_string()),
            ..Default::default()
        };

        let mut db = settings();
        McpConfig::resolve_database_credentials_with(&mut db, &store).unwrap();
        assert_eq!(db.password.as_deref(), Some("s3cret"));

        // passwordと一緒には書けない
        let mut both = DatabaseSettings {
            password: Some("other".to_string()),
            ..settings()
        };
        let err = McpConfig::resolve_database_credentials_with(&mut both, &store).unwrap_err();
        assert!(err.contains("cannot be combined"), "{err}");

        // 別のホストには使わない
        let mut elsewhere = DatabaseSettings {
            host: Some("attacker.example".to_string()),
            ..settings()
        };
        let err = McpConfig::resolve_database_credentials_with(&mut elsewhere, &store).unwrap_err();
        assert!(err.starts_with("database.keyring:"), "{err}");
        assert!(elsewhere.password.is_none());

        // 使わない設定では、資格情報ストアを読まない
        let mut disabled = DatabaseSettings {
            enabled: Some(false),
            keyring: Some("missing".to_string()),
            ..settings()
        };
        assert!(McpConfig::resolve_database_credentials_with(&mut disabled, &store).is_ok());
    }

    /// プラグインの相対パスは、設定ファイルのフォルダを基準にする（#598）
    #[test]
    fn test_plugin_relative_paths_are_resolved_from_the_config_folder() {
        let base = std::path::Path::new("/configs");
        let plugin = |command: &str, working_dir: Option<&str>| PluginConfig {
            name: "p".to_string(),
            command: command.to_string(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: working_dir.map(str::to_string),
            container: None,
            kubernetes: None,
            enabled: None,
            call_timeout_seconds: None,
            max_response_bytes: None,
            rate_limit: None,
            trust_annotations: None,
        };

        let mut relative = plugin("./server", Some("data"));
        relative.resolve_paths(Some(base));
        assert_eq!(
            std::path::Path::new(&relative.command),
            base.join("./server")
        );
        assert_eq!(
            std::path::Path::new(relative.working_dir.as_deref().unwrap()),
            base.join("data")
        );

        // 名前だけのコマンドは、今までどおりPATHから探す
        let mut on_path = plugin("node", None);
        on_path.resolve_paths(Some(base));
        assert_eq!(on_path.command, "node");

        // 絶対パスは変えない
        let absolute_command = if cfg!(windows) {
            "C:/bin/server"
        } else {
            "/bin/server"
        };
        let mut absolute = plugin(absolute_command, None);
        absolute.resolve_paths(Some(base));
        assert_eq!(absolute.command, absolute_command);

        // コンテナの中のパスは変えない
        let mut container = plugin("./server", Some("data"));
        container.container = Some(toml::from_str("image = \"x\"").unwrap());
        container.resolve_paths(Some(base));
        assert_eq!(container.command, "./server");
        assert_eq!(container.working_dir.as_deref(), Some("data"));
    }

    /// `read_file`で読んだ設定のプラグインは、設定ファイルのフォルダを基準にする
    #[test]
    fn test_read_file_resolves_plugin_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-config.toml");
        std::fs::write(
            &path,
            "[server]\n\n[transport]\ntype = \"stdio\"\n\n[handlers]\n\n[[plugins]]\nname = \"p\"\ncommand = \"bin/server\"\nworking_dir = \"work\"\n",
        )
        .unwrap();
        let config = McpConfig::read_file(path.to_str().unwrap()).unwrap();
        let folder = std::path::absolute(dir.path()).unwrap();
        assert_eq!(
            std::path::Path::new(&config.plugins[0].command),
            folder.join("bin/server")
        );
        assert_eq!(
            std::path::Path::new(config.plugins[0].working_dir.as_deref().unwrap()),
            folder.join("work")
        );
    }

    // ---- #632: Claude Desktopの拡張機能 ----

    #[test]
    fn test_config_path_from_env() {
        assert_eq!(
            config_path_from_env(Some(" C:/me/config.toml ")),
            Some("C:/me/config.toml".to_string())
        );
        // フォルダ名に`${`を含むパスは使う（#774）
        assert_eq!(
            config_path_from_env(Some("/home/me/${archive}/config.toml")),
            Some("/home/me/${archive}/config.toml".to_string())
        );
        // 値の全体が1つの`${...}`でなければ使う（#779）
        assert_eq!(
            config_path_from_env(Some("${archive}/settings-${team}")),
            Some("${archive}/settings-${team}".to_string())
        );
        for value in [
            None,
            Some(""),
            Some("  "),
            Some("${user_config.config_file}"),
        ] {
            assert_eq!(config_path_from_env(value), None, "{value:?}");
        }
    }

    fn extension_config(url: &str, username: &str, password: &str) -> McpConfig {
        let mut config = McpConfig::default();
        config.handlers.wordpress = Some(
            toml::from_str(&format!(
                "enabled = true\nurl = \"{url}\"\nusername = \"{username}\"\npassword = \"{password}\"\n"
            ))
            .unwrap(),
        );
        config
    }

    /// サイトが入力されていなければ、WordPressを使わずに起動し、何を入力するかを残す
    #[test]
    fn test_allow_unset_extension_site() {
        let mut empty = extension_config("", "", "");
        empty.allow_unset_extension_site();
        assert_eq!(
            empty.handlers.wordpress.as_ref().unwrap().enabled,
            Some(false)
        );
        assert_eq!(empty.startup_notes.len(), 1);
        assert!(empty.startup_notes[0]
            .1
            .contains("enter the site URL, the username, the Application Password"));

        // 拡張機能が埋めなかった${user_config.…}も、入力されていないとみなす
        let mut placeholder = extension_config("${user_config.wordpress_url}", "editor", "pass");
        placeholder.allow_unset_extension_site();
        assert_eq!(
            placeholder.handlers.wordpress.as_ref().unwrap().enabled,
            Some(false)
        );
        assert!(placeholder.startup_notes[0]
            .1
            .contains("enter the site URL in"));

        // 入力されていれば、何も変えない
        let mut filled = extension_config("https://example.com", "editor", "pass");
        filled.allow_unset_extension_site();
        assert_eq!(
            filled.handlers.wordpress.as_ref().unwrap().enabled,
            Some(true)
        );
        assert!(filled.startup_notes.is_empty());
    }

    /// 拡張機能から起動したら、選んだ設定ファイルがHTTPモードでもSTDIOにする
    /// （PR #666のレビュー）
    #[test]
    fn test_prepare_for_extension_uses_stdio() {
        let mut config = extension_config("https://example.com", "editor", "pass");
        config.transport.transport_type = Some("http".to_string());
        config.prepare_for_extension();
        assert_eq!(config.transport.transport_type.as_deref(), Some("stdio"));
        assert!(matches!(
            config.to_transport_config().transport_type,
            crate::transport::TransportType::Stdio
        ));
        assert!(config.startup_notes.is_empty());
    }

    /// 拡張機能に入れる設定ファイルが読め、サイトを入力すればWordPressを使う
    #[test]
    fn test_extension_config_file_reads() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/mcpb/config.toml");
        let config = McpConfig::read_file(path.to_str().unwrap()).unwrap();
        let wordpress = config.handlers.wordpress.unwrap();
        assert_eq!(wordpress.enabled, Some(true));
        assert_eq!(wordpress.url, "${WORDPRESS_URL}");
    }
}
