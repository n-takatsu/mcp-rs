//! `mcp-rs check`: 設定を、MCPのクライアントを使わずに確かめる（#565）
//!
//! 設定の誤りでmcp-rsが起動しないとき、MCPのクライアントは理由をほとんど
//! 見せない（Copilotのチャットは「ツールが見つからない」とだけ言う）。
//! 起動と同じ関数・同じ順で設定を読み、段階ごとに結果を表示する。
//! パスワード・トークン・ユーザー名は表示しない。
//!
//! `[database]`・`[[plugins]]`も確かめる（#589）。起動に失敗しても、
//! mcp-rsはそのツールが無いまま動くので、理由が分かりにくいため。
//!
//! まだ確かめないもの: HTTPモードの起動時の検査、黙って無視されるキー。

use crate::config::{McpConfig, PluginConfig, WordPressConfig};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `[database]`の項目の名前。データベースが使えなくても、mcp-rsはその
/// ツールが無いまま起動する
pub const DATABASE_ITEMS: [&str; 2] = ["database settings", "database"];

/// データベースへの接続・プラグインの起動を待つ時間（1つにつき、#589）
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// サブコマンドの名前
pub const SUBCOMMAND: &str = "check";

/// 1項目の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// 問題無い
    Ok,
    /// 動くが、知らせたいこと
    Warn,
    /// 起動しない、または使えないもの
    Error,
    /// 確かめなかったもの
    Skip,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Warn => "WARN",
            Status::Error => "ERROR",
            Status::Skip => "SKIP",
        }
    }
}

/// 確かめた結果の一覧
#[derive(Debug, Default)]
pub struct Report {
    items: Vec<(Status, String, String)>,
}

impl Report {
    fn push(&mut self, status: Status, item: &str, detail: impl Into<String>) {
        self.items.push((status, item.to_string(), detail.into()));
    }

    /// `items`以外の項目の、エラーの数（#590。`mcp-rs init`は、データベース
    /// だけのエラーでは登録を止めない）
    pub fn count_errors_except(&self, items: &[&str]) -> usize {
        self.items
            .iter()
            .filter(|(s, item, _)| *s == Status::Error && !items.contains(&item.as_str()))
            .count()
    }

    /// `status`の項目の数
    pub fn count(&self, status: Status) -> usize {
        self.items.iter().filter(|(s, _, _)| *s == status).count()
    }

    /// `item`の結果（テスト用）
    pub fn find(&self, item: &str) -> Option<(Status, &str)> {
        self.items
            .iter()
            .find(|(_, i, _)| i == item)
            .map(|(s, _, d)| (*s, d.as_str()))
    }

    /// 表示する文
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (status, item, detail) in &self.items {
            out.push_str(&format!("{:<5}  {}: {}\n", status.label(), item, detail));
        }
        let (errors, warnings) = (self.count(Status::Error), self.count(Status::Warn));
        out.push('\n');
        out.push_str(&if errors == 0 && warnings == 0 {
            "No problems found.\n".to_string()
        } else {
            format!(
                "{} error{}, {} warning{}.\n",
                errors,
                if errors == 1 { "" } else { "s" },
                warnings,
                if warnings == 1 { "" } else { "s" }
            )
        });
        out
    }
}

/// `mcp-rs check`の引数
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CheckArgs {
    /// `--config <path>`
    pub config: Option<String>,
    /// `--offline`: WordPressのサイト・データベースにつながず、プラグインを
    /// 起動しない
    pub offline: bool,
    /// `--help`
    pub help: bool,
}

/// 引数（サブコマンドの名前より後ろ）を解釈する
pub fn parse_args(args: &[String]) -> Result<CheckArgs, String> {
    let mut parsed = CheckArgs::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--help" | "-h" => parsed.help = true,
            "--offline" => parsed.offline = true,
            "--config" => match rest.next() {
                Some(path) => parsed.config = Some(path.clone()),
                None => return Err("--config needs a path".to_string()),
            },
            _ => match arg.strip_prefix("--config=") {
                Some("") => return Err("--config needs a path".to_string()),
                Some(path) => parsed.config = Some(path.to_string()),
                None => return Err(format!("unknown argument '{}'", arg)),
            },
        }
    }
    Ok(parsed)
}

/// `mcp-rs check --help`の表示
pub fn help_text() -> String {
    format!(
        "Usage: mcp-rs {SUBCOMMAND} [--config <path>] [--offline]

Checks the config file without an MCP client, the same way mcp-rs reads it
when it starts:
  - the config file loads (the line and the reason of an error)
  - the WordPress credentials can be read (keyring, ${{VAR}},
    encrypted_credentials)
  - mcp-rs can reach the WordPress site and sign in (read-only requests)
  - [database]: the settings (engine, ssl_mode, ssl_root_cert, ${{VAR}}),
    and that mcp-rs can connect and sign in. Weak TLS settings are warned
  - [[plugins]]: the command can be found, and each plugin starts and
    answers MCP initialize and tools/list. It is stopped afterwards
  - where the log and the audit log are written
Passwords, tokens and WordPress user names are not shown. For [database],
the user name is shown with the server (such as reader@127.0.0.1:5432/app).

Like starting mcp-rs, it creates the log and audit log folders and files
if they do not exist, and adds a \"checked\" line to the log file to make
sure it can be written. Nothing is sent to the site except read requests,
and nothing is written to the database. Starting a plugin runs its command
(or its container or Kubernetes Job) for a moment; use --offline to avoid
it. Connecting and starting each wait up to {timeout} seconds.

Not checked yet: the HTTP mode settings.

Options:
  --config <path>  Config file to check, as for starting mcp-rs. Without
                   it, ~/.config/mcp-rs/config.toml is checked
  --offline        Do not connect to the WordPress site or the database,
                   and do not start the plugins (only their settings and
                   commands are checked)
  -h, --help       Print this help and exit

Exit status: 0 when no errors were found (warnings are allowed), 1 when
there were errors, 2 for wrong arguments.
",
        timeout = CONNECT_TIMEOUT.as_secs()
    )
}

/// 引数を解釈して確かめ、結果を表示する。`Ok(true)`はエラー無し、
/// `Ok(false)`はエラーあり、`Err`は引数の誤り
pub async fn run(args: &[String]) -> Result<bool, String> {
    let args = parse_args(args)?;
    if args.help {
        print!("{}", help_text());
        return Ok(true);
    }
    println!("mcp-rs {} {}\n", env!("CARGO_PKG_VERSION"), SUBCOMMAND);
    let report = check(&args).await;
    print!("{}", report.render());
    Ok(report.count(Status::Error) == 0)
}

/// Unixで、設定ファイルをほかのユーザーが読めるなら、その知らせ（#599）。
/// パスワードを書き足すことがあるので、本人だけが読めるようにする
fn readable_by_others(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).ok()?.permissions().mode() & 0o777;
        (mode & 0o077 != 0).then(|| {
            format!(
                "other users can read it (mode {mode:o}). Run chmod 600 {}",
                path.display()
            )
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// 確かめる。表示はしない
pub async fn check(args: &CheckArgs) -> Report {
    // 起動と同じく、`MCP_RS_CONFIG`を`--config`より優先する（#632、PR #666のレビュー）
    let config_path = crate::config::config_path_from_env(
        std::env::var(crate::config::CONFIG_ENV_VAR).ok().as_deref(),
    )
    .or(args.config.clone());
    check_file(config_path.as_deref(), args.offline).await
}

/// `config_path`の設定ファイルを確かめる（`None`は既定の設定ファイル）。
/// `MCP_RS_CONFIG`は見ない。`mcp-rs init`は、書いた設定ファイルそのものを
/// 確かめる（PR #671のレビュー）
pub async fn check_file(config_path: Option<&str>, offline: bool) -> Report {
    let mut report = Report::default();
    let Some((config, credentials_ok)) = check_config(config_path, &mut report) else {
        return report;
    };
    check_version(&config, offline, &mut report).await;
    check_wordpress(&config, credentials_ok, offline, &mut report).await;
    check_database(&config, offline, &mut report).await;
    check_plugins(&config.plugins, offline, &mut report).await;
    check_logs(&config, &mut report);
    check_audit_log(&config, &mut report).await;
    note_unchecked(&config, &mut report);
    report
}

/// 設定ファイルを、起動と同じ関数・同じ順で読む。読めなければ`None`。
/// 2つ目の値は、WordPressの資格情報を解決できたか
fn check_config(path: Option<&str>, report: &mut Report) -> Option<(McpConfig, bool)> {
    let Some(path) = path else {
        // `--config`無し: 起動と同じ`McpConfig::load`（環境変数も読む）。
        // 資格情報の解決まで1回で行うので、段階に分けられない
        return match McpConfig::load() {
            Ok(config) => {
                let file = config
                    .config_file
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(environment variables only)".to_string());
                report.push(Status::Ok, "config file", file);
                // 置き場所（平文か等）は、段に分けて読む`--config`のときだけ見る
                report.push(
                    Status::Ok,
                    "WordPress credentials",
                    "resolved together with the config file (run with --config to see where they are kept)",
                );
                Some((config, true))
            }
            Err(e) => {
                report.push(Status::Error, "config file", e.to_string());
                None
            }
        };
    };

    let path = crate::config::expand_config_path(path);
    if let Some(warning) = crate::config::relative_config_path_warning(&path) {
        report.push(Status::Warn, "--config", warning);
    }
    let mut config = match McpConfig::read_file(&path) {
        Ok(config) => config,
        Err(e) => {
            report.push(Status::Error, "config file", e);
            return None;
        }
    };
    let shown = config
        .config_file
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or(path);
    report.push(Status::Ok, "config file", shown.clone());
    if let Some(warning) = readable_by_others(Path::new(&shown)) {
        report.push(Status::Warn, "config file permissions", warning);
    }

    // 置き場所は、解決する前の設定から決める（解決すると平文と同じになる）
    // ほかのサイトがあれば、サイトごとに（#640）
    let sources: Vec<(String, (Status, String))> = config
        .handlers
        .wordpress
        .as_ref()
        .filter(|wp| wp.enabled == Some(true))
        .and_then(|wp| {
            let sites = wp.site_list().ok()?;
            let multiple = sites.len() > 1;
            Some(
                sites
                    .iter()
                    .map(|(name, site)| {
                        (
                            site_item("WordPress credentials", multiple.then_some(name)),
                            credential_source(site),
                        )
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();
    let credentials_ok = match config.resolve_wordpress_credentials() {
        Ok(()) => {
            for (item, (status, detail)) in sources {
                report.push(status, &item, detail);
            }
            true
        }
        Err(e) => {
            report.push(Status::Error, "WordPress credentials", e);
            false
        }
    };
    if let Err(e) = config.expand_http_api_keys() {
        report.push(Status::Error, "HTTP API keys", e.to_string());
    }
    Some((config, credentials_ok))
}

/// 項目の名前。ほかのサイトがあれば、サイトの名前を付ける（#640）
fn site_item(item: &str, site: Option<&String>) -> String {
    match site {
        Some(site) => format!("{item} ({site})"),
        None => item.to_string(),
    }
}

/// 資格情報の置き場所の説明。平文で書いてあれば知らせる
fn credential_source(wp: &WordPressConfig) -> (Status, String) {
    if let Some(name) = &wp.keyring {
        (
            Status::Ok,
            format!("read from the OS credential store (keyring = \"{name}\")"),
        )
    } else if wp.encrypted_credentials.is_some() {
        (
            Status::Ok,
            "decrypted encrypted_credentials with WORDPRESS_MASTER_PASSWORD".to_string(),
        )
    } else if wp.password.contains("${") {
        (
            Status::Ok,
            "read from environment variables (${...})".to_string(),
        )
    } else {
        (
            Status::Warn,
            "the password is written in the config file. Consider the OS credential store (keyring = \"<name>\", mcp-rs wordpress-keyring set <name>)".to_string(),
        )
    }
}

/// 新しい版があるか（#635）。GitHubのリリースの最新の版と比べる
async fn check_version(config: &McpConfig, offline: bool, report: &mut Report) {
    const ITEM: &str = "mcp-rs version";
    if offline {
        report.push(Status::Skip, ITEM, "not checked: --offline");
        return;
    }
    if config.server.check_updates == Some(false) {
        report.push(
            Status::Skip,
            ITEM,
            "not checked: [server] check_updates = false",
        );
        return;
    }
    let status = crate::update_check::check().await;
    let detail = status.describe(crate::update_check::InstallMethod::current());
    let level = match status {
        crate::update_check::UpdateStatus::UpToDate { .. } => Status::Ok,
        crate::update_check::UpdateStatus::Available { .. } => Status::Warn,
        crate::update_check::UpdateStatus::Unknown(_) => Status::Skip,
    };
    report.push(level, ITEM, detail);
}

/// WordPress: 使う設定か、ハンドラーを作れるか、サイトにつながるか
async fn check_wordpress(
    config: &McpConfig,
    credentials_ok: bool,
    offline: bool,
    report: &mut Report,
) {
    const ITEM: &str = "WordPress";
    let Some(wp) = &config.handlers.wordpress else {
        report.push(Status::Skip, ITEM, "no [handlers.wordpress] section");
        return;
    };
    match wp.enabled {
        Some(true) => {}
        Some(false) => {
            report.push(Status::Skip, ITEM, "enabled = false");
            return;
        }
        None => {
            // README: enabledが無いと、エラー無しでツールが登録されない
            report.push(
                Status::Warn,
                ITEM,
                "[handlers.wordpress] has no enabled = true, so the WordPress tools are not registered",
            );
            return;
        }
    }
    if !credentials_ok {
        report.push(
            Status::Skip,
            "WordPress site",
            "not checked: the credentials could not be read",
        );
        return;
    }
    // ほかのサイトがあれば、サイトごとに確かめる（#640）
    let sites = match wp.site_list() {
        Ok(sites) => sites,
        Err(e) => {
            report.push(Status::Error, ITEM, e);
            return;
        }
    };
    let multiple = sites.len() > 1;
    for (name, site) in &sites {
        let label = multiple.then_some(name);
        let handler = match crate::handlers::WordPressHandler::try_new(site.clone()) {
            Ok(handler) => handler,
            Err(e) => {
                report.push(Status::Error, &site_item(ITEM, label), e);
                continue;
            }
        };
        if offline {
            report.push(
                Status::Skip,
                &site_item("WordPress site", label),
                format!("{} (not connected: --offline)", site.url),
            );
            continue;
        }
        let health = handler.health_check().await;
        report_health(&site.url, label, &health, report);
    }
}

/// `health_check`の結果を、確かめた順に1項目ずつ表示する。失敗した項目は、
/// その項目の名前でエラーにする（PR #583のレビューの前は、エラーをすべて
/// 「WordPress site」の名前で最後にまとめて出していた）。サインインが
/// 失敗したら、権限・メディアは確かめていない
fn report_health(
    url: &str,
    site: Option<&String>,
    health: &crate::handlers::wordpress::WordPressHealthCheck,
    report: &mut Report,
) {
    // `health_check`がエラーの文の先頭に付ける言葉と、項目
    const SITE: (&str, &str) = ("Site accessibility failed", "WordPress site");
    const STEPS: [(&str, &str); 4] = [
        ("REST API check failed", "WordPress REST API"),
        ("Authentication failed", "WordPress sign-in"),
        ("Permissions check failed", "WordPress permissions"),
        ("Media upload check failed", "WordPress media"),
    ];
    let mut shown = vec![false; health.error_details.len()];
    let mut detail = |prefix: &str| -> String {
        match health
            .error_details
            .iter()
            .position(|d| d.starts_with(prefix))
        {
            Some(i) => {
                shown[i] = true;
                health.error_details[i].clone()
            }
            None => "failed".to_string(),
        }
    };
    if !health.site_accessible {
        report.push(Status::Error, &site_item(SITE.1, site), detail(SITE.0));
    } else {
        let name = health
            .site_info
            .as_ref()
            .map(|info| format!(" \"{}\"", info.name))
            .unwrap_or_default();
        report.push(
            Status::Ok,
            &site_item(SITE.1, site),
            format!("{url}{name} is reachable"),
        );
        let oks = [
            health.rest_api_available,
            health.authentication_valid,
            health.permissions_adequate,
            health.media_upload_possible,
        ];
        for (index, ((prefix, item), ok)) in STEPS.iter().zip(oks).enumerate() {
            let item = site_item(item, site);
            if ok {
                report.push(Status::Ok, &item, "OK");
            } else if index >= 2 && !health.authentication_valid {
                report.push(Status::Skip, &item, "not checked: the sign-in failed");
            } else {
                report.push(Status::Error, &item, detail(prefix));
            }
        }
    }
    // どの項目にも当たらないエラー（念のため）
    for (detail, shown) in health.error_details.iter().zip(shown) {
        if !shown {
            report.push(Status::Error, &site_item(SITE.1, site), detail.clone());
        }
    }
}

/// ログを書く場所（起動と同じ候補・順）
fn check_logs(config: &McpConfig, report: &mut Report) {
    const ITEM: &str = "log file";
    let log_config = crate::logging::LogConfig::from_server_config(&config.server);
    if !log_config.file_enabled {
        report.push(Status::Skip, ITEM, "file logging is off");
        return;
    }
    let destination = crate::logging::check_log_destination(&log_config);
    match (
        destination.current_file_path(),
        &destination.fallback_reason,
    ) {
        (Some(path), None) => report.push(Status::Ok, ITEM, path.display().to_string()),
        (Some(path), Some(reason)) => report.push(
            Status::Warn,
            ITEM,
            format!("{} (fallback: {})", path.display(), reason),
        ),
        (None, _) => report.push(
            Status::Warn,
            ITEM,
            "no writable folder was found; mcp-rs would log to standard error only",
        ),
    }
}

/// 監査ログを書く場所（起動と同じ確かめ方）
async fn check_audit_log(config: &McpConfig, report: &mut Report) {
    const ITEM: &str = "audit log";
    let logger = crate::security::AuditLogger::new(config.audit_config());
    match logger.file_path() {
        Some(path) => match logger.check_file_output().await {
            Ok(()) => report.push(Status::Ok, ITEM, path),
            Err(e) => report.push(
                Status::Warn,
                ITEM,
                format!("{e}. Tool calls would be kept in memory only. Set [audit] file_path to a writable location"),
            ),
        },
        None if config.audit.as_ref().and_then(|a| a.enabled) == Some(false) => {
            report.push(Status::Skip, ITEM, "not written to a file ([audit] enabled = false)")
        }
        None => report.push(
            Status::Warn,
            ITEM,
            "kept in memory only: no folder for it could be found. Set [audit] file_path to an absolute path",
        ),
    }
}

/// `[database]`: 設定（`${VAR}`・`engine`・`ssl_mode`・`ssl_root_cert`）と、
/// つながってログインできるか（#589）。起動と同じ順・同じ関数で確かめる。
/// 書き込みはしない
async fn check_database(config: &McpConfig, offline: bool, report: &mut Report) {
    const SETTINGS: &str = DATABASE_ITEMS[0];
    const CONNECTION: &str = DATABASE_ITEMS[1];
    let Some(settings) = &config.database else {
        return;
    };
    if !settings.is_enabled() {
        report.push(Status::Skip, CONNECTION, "enabled = false");
        return;
    }
    let mut settings = settings.clone();
    settings.resolve_paths(config.config_file.as_deref().and_then(Path::parent));
    if let Err(e) = McpConfig::resolve_database_credentials(&mut settings) {
        report.push(Status::Error, SETTINGS, e);
        return;
    }
    match database_settings_warning(&settings) {
        Ok(None) => report.push(Status::Ok, SETTINGS, settings.describe()),
        Ok(Some(warning)) => report.push(
            Status::Warn,
            SETTINGS,
            format!("{}: {}", settings.describe(), warning),
        ),
        Err(e) => {
            report.push(Status::Error, SETTINGS, e);
            return;
        }
    }
    if offline {
        report.push(Status::Skip, CONNECTION, "not connected: --offline");
        return;
    }
    match tokio::time::timeout(CONNECT_TIMEOUT, connect_database(&settings)).await {
        Ok(Ok(())) => report.push(
            Status::Ok,
            CONNECTION,
            format!("connected and signed in to {}", settings.describe()),
        ),
        Ok(Err(e)) => report.push(Status::Error, CONNECTION, e),
        Err(_) => report.push(
            Status::Error,
            CONNECTION,
            format!(
                "{} did not answer within {} seconds",
                settings.describe(),
                CONNECT_TIMEOUT.as_secs()
            ),
        ),
    }
}

/// `[database]`の設定を確かめる。通信を守れない設定なら、その警告
#[cfg(feature = "database")]
fn database_settings_warning(
    settings: &crate::config::DatabaseSettings,
) -> Result<Option<String>, String> {
    crate::handlers::database::handler::DatabaseHandler::check_settings(settings)
}

#[cfg(not(feature = "database"))]
fn database_settings_warning(
    settings: &crate::config::DatabaseSettings,
) -> Result<Option<String>, String> {
    settings.validate()?;
    Err(
        "this build of mcp-rs does not include database support (it was built with --no-default-features; build without it, or add --features database)"
            .to_string(),
    )
}

/// 起動と同じ関数で、データベースにつなぎ、ログインできるかを確かめる
#[cfg(feature = "database")]
async fn connect_database(settings: &crate::config::DatabaseSettings) -> Result<(), String> {
    crate::handlers::database::handler::DatabaseHandler::from_settings(settings)
        .await
        .map(drop)
}

#[cfg(not(feature = "database"))]
async fn connect_database(_settings: &crate::config::DatabaseSettings) -> Result<(), String> {
    Ok(())
}

/// `[[plugins]]`: 1つずつ、起動に使うコマンドが見つかるかと、起動して
/// MCPの`initialize`・`tools/list`に答えるか（#589）。確かめたら止める
async fn check_plugins(plugins: &[PluginConfig], offline: bool, report: &mut Report) {
    let mut seen = std::collections::HashSet::new();
    for plugin in plugins {
        let item = format!("plugin '{}'", plugin.name);
        if !plugin.enabled.unwrap_or(true) {
            report.push(Status::Skip, &item, "enabled = false");
            continue;
        }
        if !seen.insert(plugin.name.as_str()) {
            report.push(
                Status::Error,
                &item,
                "another plugin has the same name; only the first one is started",
            );
            continue;
        }
        let program = match crate::plugin_proxy::launcher_program(plugin) {
            Ok(program) => program,
            Err(e) => {
                report.push(Status::Error, &item, e);
                continue;
            }
        };
        if let Err(e) = find_command(&program) {
            report.push(Status::Error, &item, e);
            continue;
        }
        if offline {
            report.push(
                Status::Skip,
                &item,
                format!("'{}' was found (not started: --offline)", program),
            );
            continue;
        }
        report_plugin_start(plugin, &item, report).await;
    }
    if !offline {
        wait_for_plugin_cleanups(report).await;
    }
}

/// 止めたプラグインの後片付け（`docker rm -f`・`kubectl delete`）の終了を
/// 待つ。待たずに`check`が終わると、プロセスの終了でコマンドが動かず、
/// コンテナ・Job・Secretが残ることがある（PR #698のレビュー）
async fn wait_for_plugin_cleanups(report: &mut Report) {
    let cleanups =
        tokio::task::spawn_blocking(|| crate::plugin_proxy::wait_for_cleanups(CLEANUP_TIMEOUT))
            .await
            .unwrap_or_else(|_| crate::plugin_proxy::CleanupReport {
                timed_out: true,
                failures: Vec::new(),
            });
    report_plugin_cleanups(&cleanups, report);
}

/// 後片付けを待つ時間（サーバーとして終わるときと同じ）
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);

/// 後片付けで失敗した・終わらなかったものを、手で確かめるコマンドと合わせて
/// 知らせる
fn report_plugin_cleanups(cleanups: &crate::plugin_proxy::CleanupReport, report: &mut Report) {
    const ITEM: &str = "plugin cleanup";
    const HOW: &str = "check for any left over with docker ps -a --filter label=mcp-rs.plugin / kubectl get jobs,secrets -l mcp-rs.plugin";
    for failure in &cleanups.failures {
        report.push(Status::Warn, ITEM, format!("{failure}; {HOW}"));
    }
    if cleanups.timed_out {
        report.push(
            Status::Warn,
            ITEM,
            format!(
                "removing the plugin containers or Kubernetes jobs did not finish within {} seconds; {HOW}",
                CLEANUP_TIMEOUT.as_secs()
            ),
        );
    }
}

/// プラグインを起動して、ツールの一覧まで受け取り、止める。監査ログは
/// メモリにだけ残す（ツールは呼ばない）
async fn report_plugin_start(plugin: &PluginConfig, item: &str, report: &mut Report) {
    use crate::mcp::McpHandler as _;
    let started = tokio::time::timeout(
        CONNECT_TIMEOUT,
        crate::plugin_proxy::PluginHandler::start(plugin),
    )
    .await;
    match started {
        Ok(Ok(handler)) => {
            let tools = handler.list_tools().await.map(|t| t.len()).unwrap_or(0);
            // 落とすと、プロセス（コンテナ・Job）を止める
            drop(handler);
            report.push(
                Status::Ok,
                item,
                format!("started and answered with {} tool(s); stopped", tools),
            );
        }
        Ok(Err(e)) => report.push(Status::Error, item, e),
        Err(_) => {
            let hint = if plugin.container.is_some() || plugin.kubernetes.is_some() {
                ". A container or a Kubernetes Job can take longer the first time (pulling the image); mcp-rs itself waits longer when it starts"
            } else {
                ""
            };
            report.push(
                Status::Error,
                item,
                format!(
                    "did not answer initialize and tools/list within {} seconds{}",
                    CONNECT_TIMEOUT.as_secs(),
                    hint
                ),
            )
        }
    }
}

/// 起動に使うコマンドを探す。パスで書いてあればそのファイルを、名前だけ
/// なら`PATH`（絶対パスの項目だけ）を探す。Windowsでは、拡張子の無い名前に
/// `.exe`だけを補う（起動に使う`Command::new`と同じ。`npx`等の`.cmd`は、
/// `npx.cmd`と書く必要がある）
fn find_command(program: &str) -> Result<PathBuf, String> {
    let path = Path::new(program);
    if path.is_absolute() || path.components().count() > 1 {
        return if path.is_file() {
            Ok(path.to_path_buf())
        } else {
            Err(format!("'{}' was not found", program))
        };
    }
    let search = |extensions: &[&str]| {
        let dirs = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .collect::<Vec<_>>();
        crate::init::find_program_in(program, dirs.into_iter(), extensions)
    };
    let bare_on_windows = cfg!(windows) && path.extension().is_none();
    if let Some(found) = search(if bare_on_windows { &["exe"] } else { &[""] }) {
        return Ok(found);
    }
    if bare_on_windows {
        for extension in ["cmd", "bat"] {
            if search(&[extension]).is_some() {
                return Err(format!(
                    "'{program}.exe' was not found in PATH, but '{program}.{extension}' was. Write command = '{program}.{extension}'"
                ));
            }
        }
    }
    Err(format!("'{}' was not found in PATH", program))
}

/// まだ確かめないもの
fn note_unchecked(config: &McpConfig, report: &mut Report) {
    if config.transport.transport_type.as_deref() == Some("http") {
        report.push(
            Status::Skip,
            "HTTP mode",
            "the HTTP settings are checked when mcp-rs starts",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn health(oks: [bool; 5], errors: &[&str]) -> crate::handlers::wordpress::WordPressHealthCheck {
        crate::handlers::wordpress::WordPressHealthCheck {
            site_accessible: oks[0],
            rest_api_available: oks[1],
            authentication_valid: oks[2],
            permissions_adequate: oks[3],
            media_upload_possible: oks[4],
            error_details: errors.iter().map(|s| s.to_string()).collect(),
            site_info: None,
            log_destination: String::new(),
        }
    }

    /// サインインが失敗したら、その項目のエラーにし、権限・メディアは
    /// 確かめていないと出す（PR #583。誤ったパスワードで「権限OK」と出た）
    #[test]
    fn test_report_health_when_the_sign_in_fails() {
        let mut report = Report::default();
        report_health(
            "https://example.com",
            None,
            &health(
                [true, true, false, false, false],
                &["Authentication failed: Invalid credentials"],
            ),
            &mut report,
        );
        assert_eq!(report.find("WordPress site").unwrap().0, Status::Ok);
        assert_eq!(report.find("WordPress REST API").unwrap().0, Status::Ok);
        assert_eq!(
            report.find("WordPress sign-in"),
            Some((Status::Error, "Authentication failed: Invalid credentials"))
        );
        assert_eq!(
            report.find("WordPress permissions").unwrap().0,
            Status::Skip
        );
        assert_eq!(report.find("WordPress media").unwrap().0, Status::Skip);
        assert_eq!(report.count(Status::Error), 1);
    }

    #[test]
    fn test_report_health_ok_and_unreachable() {
        let mut report = Report::default();
        report_health(
            "https://example.com",
            None,
            &health([true; 5], &[]),
            &mut report,
        );
        assert_eq!(report.count(Status::Ok), 5);
        assert_eq!(report.count(Status::Error), 0);

        let mut report = Report::default();
        report_health(
            "https://example.com",
            None,
            &health([false; 5], &["Site accessibility failed: timeout"]),
            &mut report,
        );
        assert_eq!(
            report.find("WordPress site"),
            Some((Status::Error, "Site accessibility failed: timeout"))
        );
        assert_eq!(report.count(Status::Error), 1);
        assert!(report.find("WordPress sign-in").is_none());

        // 権限だけが失敗した
        let mut report = Report::default();
        report_health(
            "https://example.com",
            None,
            &health(
                [true, true, true, false, true],
                &["Permissions check failed: 403"],
            ),
            &mut report,
        );
        assert_eq!(
            report.find("WordPress permissions"),
            Some((Status::Error, "Permissions check failed: 403"))
        );
        assert_eq!(report.count(Status::Error), 1);
    }

    /// 設定ファイルを書いて、`check_config`を呼ぶ
    fn check_text(text: &str) -> (Option<(McpConfig, bool)>, Report) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-config.toml");
        std::fs::write(&path, text).unwrap();
        let mut report = Report::default();
        let result = check_config(Some(path.to_str().unwrap()), &mut report);
        (result, report)
    }

    #[test]
    fn test_parse_args() {
        assert_eq!(parse_args(&[]).unwrap(), CheckArgs::default());
        assert_eq!(
            parse_args(&args(&["--config", "C:/x.toml", "--offline"])).unwrap(),
            CheckArgs {
                config: Some("C:/x.toml".to_string()),
                offline: true,
                help: false,
            }
        );
        assert_eq!(
            parse_args(&args(&["--config=a.toml"])).unwrap().config,
            Some("a.toml".to_string())
        );
        assert!(parse_args(&args(&["-h"])).unwrap().help);
        assert_eq!(
            parse_args(&args(&["--config"])).unwrap_err(),
            "--config needs a path"
        );
        assert_eq!(
            parse_args(&args(&["--confg"])).unwrap_err(),
            "unknown argument '--confg'"
        );
    }

    #[test]
    fn test_toml_error_shows_line_without_values() {
        // #565の1つ目: `"`の位置がずれた設定（2026-11-15にVS Codeで起きた）
        let (result, report) = check_text(
            "[handlers.wordpress]\nurl = \"https://example.com\n\"keyring = \"secret-name\"\nenabled = true\n",
        );
        assert!(result.is_none());
        let (status, detail) = report.find("config file").unwrap();
        assert_eq!(status, Status::Error);
        assert!(detail.contains("line 3"), "{detail}");
        assert!(!detail.contains("secret-name"), "{detail}");
    }

    #[test]
    fn test_missing_credentials_names_keyring() {
        // #565の2つ目: keyringの行が無い
        let (result, report) = check_text(
            "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://example.com\"\nenabled = true\n",
        );
        let (_, credentials_ok) = result.unwrap();
        assert!(!credentials_ok);
        let (status, detail) = report.find("WordPress credentials").unwrap();
        assert_eq!(status, Status::Error);
        assert!(detail.contains("keyring"), "{detail}");
    }

    #[test]
    fn test_missing_environment_variable() {
        let (_, report) = check_text(
            "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://example.com\"\nusername = \"u\"\npassword = \"${MCP_RS_CHECK_TEST_UNSET_VARIABLE}\"\nenabled = true\n",
        );
        let (status, detail) = report.find("WordPress credentials").unwrap();
        assert_eq!(status, Status::Error);
        assert!(
            detail.contains("MCP_RS_CHECK_TEST_UNSET_VARIABLE"),
            "{detail}"
        );
    }

    #[test]
    fn test_plain_password_is_a_warning_and_not_shown() {
        let (result, report) = check_text(
            "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://example.com\"\nusername = \"someone\"\npassword = \"plain-secret\"\nenabled = true\n",
        );
        assert!(result.unwrap().1);
        let (status, _) = report.find("WordPress credentials").unwrap();
        assert_eq!(status, Status::Warn);
        let text = report.render();
        assert!(!text.contains("plain-secret"), "{text}");
        assert!(!text.contains("someone"), "{text}");
    }

    #[tokio::test]
    async fn test_wordpress_without_enabled_is_a_warning() {
        let (result, _) = check_text(
            "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://example.com\"\nusername = \"u\"\npassword = \"p\"\n",
        );
        let (config, ok) = result.unwrap();
        let mut report = Report::default();
        check_wordpress(&config, ok, true, &mut report).await;
        let (status, detail) = report.find("WordPress").unwrap();
        assert_eq!(status, Status::Warn);
        assert!(detail.contains("enabled = true"), "{detail}");
    }

    #[tokio::test]
    async fn test_wordpress_offline_and_insecure_url() {
        let (result, _) = check_text(
            "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://example.com\"\nusername = \"u\"\npassword = \"p\"\nenabled = true\n",
        );
        let (config, ok) = result.unwrap();
        let mut report = Report::default();
        check_wordpress(&config, ok, true, &mut report).await;
        let (status, detail) = report.find("WordPress site").unwrap();
        assert_eq!(status, Status::Skip);
        assert!(detail.contains("--offline"), "{detail}");

        // http://は、起動と同じくハンドラーを作れない
        let (result, _) = check_text(
            "[server]\n[transport]\n[handlers.wordpress]\nurl = \"http://example.com\"\nusername = \"u\"\npassword = \"p\"\nenabled = true\n",
        );
        let (config, ok) = result.unwrap();
        let mut report = Report::default();
        check_wordpress(&config, ok, true, &mut report).await;
        assert_eq!(report.find("WordPress").unwrap().0, Status::Error);
    }

    /// ほかのサイトがあれば、サイトごとに確かめ、項目にサイトの名前を付ける（#640）
    #[tokio::test]
    async fn test_wordpress_checks_each_site() {
        let (result, report) = check_text(
            "[server]\n[transport]\n[handlers.wordpress]\nenabled = true\ndefault_site = \"blog\"\n\
             [handlers.wordpress.sites.blog]\nurl = \"https://blog.example.com\"\nusername = \"u\"\npassword = \"p\"\n\
             [handlers.wordpress.sites.shop]\nurl = \"https://shop.example.com\"\nusername = \"u\"\npassword = \"p\"\n",
        );
        assert_eq!(
            report.find("WordPress credentials (blog)").unwrap().0,
            Status::Warn
        );
        assert_eq!(
            report.find("WordPress credentials (shop)").unwrap().0,
            Status::Warn
        );
        let (config, ok) = result.unwrap();
        let mut report = Report::default();
        check_wordpress(&config, ok, true, &mut report).await;
        let (_, blog) = report.find("WordPress site (blog)").unwrap();
        assert!(blog.starts_with("https://blog.example.com"), "{blog}");
        let (_, shop) = report.find("WordPress site (shop)").unwrap();
        assert!(shop.starts_with("https://shop.example.com"), "{shop}");
    }

    #[test]
    fn test_render_summary() {
        let mut report = Report::default();
        report.push(Status::Ok, "a", "fine");
        assert!(report.render().ends_with("No problems found.\n"));
        report.push(Status::Error, "b", "broken");
        report.push(Status::Warn, "c", "hmm");
        let text = report.render();
        assert!(text.contains("ERROR  b: broken"), "{text}");
        assert!(text.ends_with("1 error, 1 warning.\n"), "{text}");
    }

    /// `[database]`・`[[plugins]]`を足した設定を読む（#589）
    fn config_with(extra: &str) -> McpConfig {
        let text = format!(
            "[server]\n\n[transport]\ntype = \"stdio\"\n\n[handlers]\n\n{}",
            extra
        );
        let (config, report) = check_text(&text);
        config
            .unwrap_or_else(|| panic!("config did not load: {}", report.render()))
            .0
    }

    async fn database_report(extra: &str, offline: bool) -> Report {
        let mut report = Report::default();
        check_database(&config_with(extra), offline, &mut report).await;
        report
    }

    #[tokio::test]
    async fn test_database_settings() {
        // [database]が無ければ、何も出さない
        let report = database_report("", false).await;
        assert_eq!(report.items.len(), 0);

        let report = database_report(
            "[database]\nenabled = false\nengine = \"postgresql\"\ndatabase = \"app\"\nusername = \"u\"\n",
            false,
        )
        .await;
        assert_eq!(
            report.find("database"),
            Some((Status::Skip, "enabled = false"))
        );

        // 環境変数が無い
        let report = database_report(
            "[database]\nengine = \"postgresql\"\ndatabase = \"app\"\nusername = \"u\"\npassword = \"${MCP_RS_TEST_SURELY_UNSET_DB_PASSWORD}\"\n",
            true,
        )
        .await;
        let (status, detail) = report.find("database settings").unwrap();
        assert_eq!(status, Status::Error);
        assert!(
            detail.contains("MCP_RS_TEST_SURELY_UNSET_DB_PASSWORD"),
            "{detail}"
        );
        assert!(report.find("database").is_none());

        // 書いた値の誤り
        let report = database_report(
            "[database]\nengine = \"postgresql\"\ndatabase = \"app\"\nusername = \"u\"\nssl_mode = \"strict\"\n",
            true,
        )
        .await;
        let (status, detail) = report.find("database settings").unwrap();
        assert_eq!(status, Status::Error);
        assert!(detail.contains("ssl_mode"), "{detail}");
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn test_database_offline_and_weak_tls() {
        // ループバックなら、TLSの警告は出さない。--offlineでは、つながない
        let report = database_report(
            "[database]\nengine = \"postgresql\"\nhost = \"127.0.0.1\"\ndatabase = \"app\"\nusername = \"reader\"\npassword = \"s3cret-value\"\n",
            true,
        )
        .await;
        let (status, detail) = report.find("database settings").unwrap();
        assert_eq!(status, Status::Ok, "{detail}");
        assert!(detail.contains("reader@127.0.0.1:5432/app"), "{detail}");
        assert_eq!(
            report.find("database"),
            Some((Status::Skip, "not connected: --offline"))
        );
        assert!(!report.render().contains("s3cret-value"));

        // ネットワークの向こうで、暗号化を確かめない設定
        let report = database_report(
            "[database]\nengine = \"postgresql\"\nhost = \"db.example.com\"\ndatabase = \"app\"\nusername = \"reader\"\nssl_mode = \"require\"\n",
            true,
        )
        .await;
        let (status, detail) = report.find("database settings").unwrap();
        assert_eq!(status, Status::Warn);
        assert!(detail.contains("verify-full"), "{detail}");
    }

    /// つながらなければエラー。パスワードは出さない
    #[cfg(feature = "database")]
    #[tokio::test]
    async fn test_database_connection_failure() {
        let report = database_report(
            "[database]\nengine = \"postgresql\"\nhost = \"127.0.0.1\"\nport = 1\ndatabase = \"app\"\nusername = \"reader\"\npassword = \"s3cret-value\"\nssl_mode = \"disable\"\n",
            false,
        )
        .await;
        let (status, detail) = report.find("database").unwrap();
        assert_eq!(status, Status::Error, "{detail}");
        assert!(detail.contains("127.0.0.1:1"), "{detail}");
        assert!(!report.render().contains("s3cret-value"));
    }

    #[tokio::test]
    async fn test_plugins_offline() {
        let exe = std::env::current_exe().unwrap();
        let exe = exe.to_string_lossy().replace('\\', "/");
        let config = config_with(&format!(
            "[[plugins]]\nname = \"found\"\ncommand = \"{exe}\"\n\n\
             [[plugins]]\nname = \"found\"\ncommand = \"{exe}\"\n\n\
             [[plugins]]\nname = \"missing\"\ncommand = \"mcp-rs-surely-missing-command\"\n\n\
             [[plugins]]\nname = \"off\"\ncommand = \"mcp-rs-surely-missing-command\"\nenabled = false\n\n\
             [[plugins]]\nname = \"bad name\"\ncommand = \"{exe}\"\n"
        ));
        let mut report = Report::default();
        check_plugins(&config.plugins, true, &mut report).await;

        let (status, detail) = report.find("plugin 'found'").unwrap();
        assert_eq!(status, Status::Skip);
        assert!(detail.contains("--offline"), "{detail}");
        // 同じ名前の2つ目
        let duplicate = report
            .items
            .iter()
            .filter(|(_, item, _)| item == "plugin 'found'")
            .nth(1)
            .unwrap();
        assert_eq!(duplicate.0, Status::Error);
        assert!(duplicate.2.contains("same name"));

        let (status, detail) = report.find("plugin 'missing'").unwrap();
        assert_eq!(status, Status::Error);
        assert!(detail.contains("not found"), "{detail}");
        assert_eq!(
            report.find("plugin 'off'"),
            Some((Status::Skip, "enabled = false"))
        );
        assert_eq!(report.find("plugin 'bad name'").unwrap().0, Status::Error);
    }

    /// 起動して、ツールの一覧を受け取り、止める
    #[tokio::test]
    async fn test_plugin_starts_and_stops() {
        // 後片付けの結果を読み出すので、同じことをするテストと同時に動かさない
        // （PR #698のレビュー）
        let _registry = crate::plugin_proxy::CLEANUP_REGISTRY_TEST_LOCK.lock().await;
        let plugin = crate::plugin_proxy::tests::test_plugin_config("echo");
        let mut report = Report::default();
        check_plugins(&[plugin], false, &mut report).await;
        let (status, detail) = report.find("plugin 'echo'").unwrap();
        assert_eq!(status, Status::Ok, "{detail}");
        assert!(detail.starts_with("started and answered with"), "{detail}");
        assert!(detail.ends_with("stopped"), "{detail}");
    }

    #[test]
    fn test_find_command() {
        let exe = std::env::current_exe().unwrap();
        assert_eq!(find_command(exe.to_str().unwrap()).unwrap(), exe);
        assert!(find_command("mcp-rs-surely-missing-command")
            .unwrap_err()
            .contains("not found in PATH"));
        assert!(find_command("./mcp-rs-surely-missing-command").is_err());
    }

    /// プラグインの後片付けで失敗した・終わらなかったものは、手で確かめる
    /// コマンドと合わせて警告する（PR #698のレビュー）
    #[test]
    fn test_report_plugin_cleanups() {
        let mut report = Report::default();
        report_plugin_cleanups(&crate::plugin_proxy::CleanupReport::default(), &mut report);
        assert!(report.items.is_empty());

        let cleanups = crate::plugin_proxy::CleanupReport {
            timed_out: true,
            failures: vec!["`kubectl delete job/x` failed (exit code: 1)".to_string()],
        };
        report_plugin_cleanups(&cleanups, &mut report);
        let shown = report.render();
        assert!(shown.contains("plugin cleanup"), "{shown}");
        assert!(shown.contains("`kubectl delete job/x` failed"), "{shown}");
        assert!(
            shown.contains("did not finish within 10 seconds"),
            "{shown}"
        );
        assert!(
            shown.contains("kubectl get jobs,secrets -l mcp-rs.plugin"),
            "{shown}"
        );
    }
}
