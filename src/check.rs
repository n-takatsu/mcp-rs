//! `mcp-rs check`: 設定を、MCPのクライアントを使わずに確かめる（#565）
//!
//! 設定の誤りでmcp-rsが起動しないとき、MCPのクライアントは理由をほとんど
//! 見せない（Copilotのチャットは「ツールが見つからない」とだけ言う）。
//! 起動と同じ関数・同じ順で設定を読み、段階ごとに結果を表示する。
//! パスワード・トークン・ユーザー名は表示しない。
//!
//! まだ確かめないもの: `[database]`・`[[plugins]]`の起動、HTTPモードの
//! 起動時の検査、黙って無視されるキー。

use crate::config::{McpConfig, WordPressConfig};

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
    /// `--offline`: WordPressのサイトにつながない
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
  - where the log and the audit log are written
Passwords, tokens and user names are not shown.

Like starting mcp-rs, it creates the log and audit log folders and files
if they do not exist, and adds a \"checked\" line to the log file to make
sure it can be written. Nothing is sent to the site except read requests.

Not checked yet: [database], [[plugins]], and the HTTP mode settings.

Options:
  --config <path>  Config file to check, as for starting mcp-rs. Without
                   it, ~/.config/mcp-rs/config.toml is checked
  --offline        Do not connect to the WordPress site
  -h, --help       Print this help and exit

Exit status: 0 when no errors were found (warnings are allowed), 1 when
there were errors, 2 for wrong arguments.
"
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

/// 確かめる。表示はしない
pub async fn check(args: &CheckArgs) -> Report {
    let mut report = Report::default();
    let Some((config, credentials_ok)) = check_config(args.config.as_deref(), &mut report) else {
        return report;
    };
    check_wordpress(&config, credentials_ok, args.offline, &mut report).await;
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
    report.push(Status::Ok, "config file", shown);

    // 置き場所は、解決する前の設定から決める（解決すると平文と同じになる）
    let source = config
        .handlers
        .wordpress
        .as_ref()
        .filter(|wp| wp.enabled == Some(true))
        .map(credential_source);
    let credentials_ok = match config.resolve_wordpress_credentials() {
        Ok(()) => {
            if let Some((status, detail)) = source {
                report.push(status, "WordPress credentials", detail);
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
    let handler = match crate::handlers::WordPressHandler::try_new(wp.clone()) {
        Ok(handler) => handler,
        Err(e) => {
            report.push(Status::Error, ITEM, e);
            return;
        }
    };
    if offline {
        report.push(
            Status::Skip,
            "WordPress site",
            format!("{} (not connected: --offline)", wp.url),
        );
        return;
    }
    let health = handler.health_check().await;
    report_health(&wp.url, &health, report);
}

/// `health_check`の結果を、確かめた順に1項目ずつ表示する。失敗した項目は、
/// その項目の名前でエラーにする（PR #583のレビューの前は、エラーをすべて
/// 「WordPress site」の名前で最後にまとめて出していた）。サインインが
/// 失敗したら、権限・メディアは確かめていない
fn report_health(
    url: &str,
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
        report.push(Status::Error, SITE.1, detail(SITE.0));
    } else {
        let name = health
            .site_info
            .as_ref()
            .map(|info| format!(" \"{}\"", info.name))
            .unwrap_or_default();
        report.push(Status::Ok, SITE.1, format!("{url}{name} is reachable"));
        let oks = [
            health.rest_api_available,
            health.authentication_valid,
            health.permissions_adequate,
            health.media_upload_possible,
        ];
        for (index, ((prefix, item), ok)) in STEPS.iter().zip(oks).enumerate() {
            if ok {
                report.push(Status::Ok, item, "OK");
            } else if index >= 2 && !health.authentication_valid {
                report.push(Status::Skip, item, "not checked: the sign-in failed");
            } else {
                report.push(Status::Error, item, detail(prefix));
            }
        }
    }
    // どの項目にも当たらないエラー（念のため）
    for (detail, shown) in health.error_details.iter().zip(shown) {
        if !shown {
            report.push(Status::Error, SITE.1, detail.clone());
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

/// まだ確かめないもの
fn note_unchecked(config: &McpConfig, report: &mut Report) {
    if config.database.as_ref().is_some_and(|db| db.is_enabled()) {
        report.push(
            Status::Skip,
            "database",
            "not checked yet; after starting mcp-rs, see mcp_rs_status",
        );
    }
    if !config.plugins.is_empty() {
        report.push(
            Status::Skip,
            "plugins",
            format!(
                "{} plugin(s) not checked yet; after starting mcp-rs, see mcp_rs_status",
                config.plugins.len()
            ),
        );
    }
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
        report_health("https://example.com", &health([true; 5], &[]), &mut report);
        assert_eq!(report.count(Status::Ok), 5);
        assert_eq!(report.count(Status::Error), 0);

        let mut report = Report::default();
        report_health(
            "https://example.com",
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
}
