//! `mcp-rs init`: 質問に答えるだけで、使い始める準備をする（#575）
//!
//! 今まで手で行っていた次の作業を、1つのコマンドにまとめる。
//!
//! 1. WordPressの資格情報を、OSの資格情報ストアに入れる（`wordpress-keyring set`と同じ）
//! 2. 設定ファイルを書く（既定は`~/.config/mcp-rs/config.toml`。`--config`無しでも読む場所）
//! 3. `mcp-rs check`と同じ確認をする（サイトにつながり、認証が通るか）
//! 4. Claude Desktop・Claude Code・Copilot（VS Code）に登録する。1つずつ
//!    確かめてから行い、断ったものは、登録のコマンドや設定を表示する
//!
//! 最初の版は、WordPressだけを聞く（`[database]`は後で足す）。パスワードは
//! 引数で受け取らない。画面には出さない。

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use zeroize::Zeroizing;

use crate::security::credential_tool::read_hidden;
use crate::security::keyring_store::{
    load_wordpress_credentials, site_origin, store_wordpress_credentials, KeyringError, OsKeyring,
    SecretStore, WordPressKeyringCredentials,
};

/// サブコマンドの名前
pub const SUBCOMMAND: &str = "init";

/// クライアントに登録するときの、MCPサーバーの名前
const SERVER_NAME: &str = "mcp-rs";

/// `mcp-rs init`の引数
#[derive(Debug, Default, PartialEq, Eq)]
pub struct InitArgs {
    /// `--config <path>`: 書く設定ファイル
    pub config: Option<String>,
    /// `--help`
    pub help: bool,
}

/// 引数（サブコマンドの名前より後ろ）を解釈する
pub fn parse_args(args: &[String]) -> Result<InitArgs, String> {
    let mut parsed = InitArgs::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--help" | "-h" => parsed.help = true,
            "--config" => match rest.next() {
                Some(path) if !path.is_empty() => parsed.config = Some(path.clone()),
                _ => return Err("--config needs a path".to_string()),
            },
            _ => match arg.strip_prefix("--config=") {
                Some("") => return Err("--config needs a path".to_string()),
                Some(path) => parsed.config = Some(path.to_string()),
                // 引数は表示しない（パスワードを渡してしまったときに、もう一度出さない）
                None => {
                    return Err(
                        "unknown argument. mcp-rs init asks for everything it needs, and never takes passwords as arguments"
                            .to_string(),
                    )
                }
            },
        }
    }
    Ok(parsed)
}

/// `mcp-rs init --help`の表示
pub fn help_text() -> String {
    format!(
        "Usage: mcp-rs {SUBCOMMAND} [--config <path>]

Sets up mcp-rs by asking a few questions:
  1. the WordPress site URL, username and Application Password, kept in the
     OS credential store (Windows Credential Manager, macOS Keychain,
     Secret Service on Linux)
  2. writes the config file (asks before replacing one, and keeps a backup)
  3. checks the config file like `mcp-rs check`: it connects to the site
     and signs in with read-only requests
  4. registers mcp-rs with Claude Desktop, Claude Code and GitHub Copilot
     (VS Code), asking for each one. For the ones you skip, it prints the
     command or the settings to add yourself

Run it in a terminal. Passwords are never taken as arguments and are not
shown while typing.

Options:
  --config <path>  Config file to write. Without it,
                   ~/.config/mcp-rs/config.toml, which mcp-rs also reads
                   without --config
  -h, --help       Print this help and exit
"
    )
}

/// 引数を解釈して実行する。`Ok(true)`は最後まで済んだ、`Ok(false)`は途中で
/// 止まった（理由は表示済み）、`Err`は引数の誤り等
pub async fn run(args: &[String]) -> Result<bool, String> {
    let args = parse_args(args)?;
    if args.help {
        print!("{}", help_text());
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        return Err(
            "mcp-rs init asks questions, so run it in a terminal. To set up without one, see `mcp-rs wordpress-keyring --help` and the README"
                .to_string(),
        );
    }
    let config_path = match args.config {
        Some(path) => crate::config::expand_config_path(&path),
        None => crate::config::user_config_path().ok_or_else(|| {
            "cannot find your home folder: pass the config file to write with --config <path>"
                .to_string()
        })?,
    };
    let config_path =
        std::path::absolute(&config_path).map_err(|e| format!("cannot use {config_path}: {e}"))?;
    let mut terminal = Terminal;
    let result = Setup {
        io: &mut terminal,
        store: &OsKeyring,
        config_path,
    }
    .run()
    .await;
    // 途中の失敗（資格情報ストアが使えない等）は、使い方の誤りではないので、
    // 使い方は表示しない
    result.or_else(|e| {
        eprintln!("\nerror: {e}");
        Ok(false)
    })
}

/// 質問と表示。テストでは、決めた答えを返すものに置き換える
trait Prompt {
    /// 1行の答え（前後の空白を除く）
    fn ask(&mut self, question: &str) -> io::Result<String>;
    /// 画面に出さずに読む答え
    fn ask_hidden(&mut self, question: &str) -> io::Result<Zeroizing<String>>;
    fn say(&mut self, text: &str);
}

/// 端末
struct Terminal;

impl Prompt for Terminal {
    fn ask(&mut self, question: &str) -> io::Result<String> {
        print!("{question}");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the input ended",
            ));
        }
        Ok(line.trim().to_string())
    }

    fn ask_hidden(&mut self, question: &str) -> io::Result<Zeroizing<String>> {
        let value = read_hidden(question)?;
        Ok(Zeroizing::new(value.trim().to_string()))
    }

    fn say(&mut self, text: &str) {
        println!("{text}");
    }
}

/// はい・いいえの質問。空の答えは`default`
fn confirm(io: &mut dyn Prompt, question: &str, default: bool) -> io::Result<bool> {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    loop {
        let answer = io.ask(&format!("{question} {hint} "))?;
        match answer.to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => io.say("Answer y or n."),
        }
    }
}

/// 準備の手順
struct Setup<'a> {
    io: &'a mut dyn Prompt,
    store: &'a dyn SecretStore,
    config_path: PathBuf,
}

impl Setup<'_> {
    async fn run(mut self) -> Result<bool, Box<dyn std::error::Error>> {
        self.io.say(&format!(
            "mcp-rs {} init: set up mcp-rs for Claude Desktop, Claude Code and GitHub Copilot.\nPress Ctrl+C to stop at any time.\n",
            env!("CARGO_PKG_VERSION")
        ));
        let config_display = self.config_path.display().to_string();

        // 1・2. 資格情報と設定ファイル
        let write_config = if self.config_path.exists() {
            self.io
                .say(&format!("A config file already exists: {config_display}"));
            confirm(
                self.io,
                "Replace it with a new one for WordPress? (a backup copy is kept)",
                false,
            )?
        } else {
            true
        };
        if write_config {
            let (url, entry) = self.wordpress_credentials()?;
            let backup = write_config_file(&self.config_path, &config_toml(&url, &entry))?;
            if let Some(backup) = backup {
                self.io.say(&format!(
                    "Kept the previous config file as {}",
                    backup.display()
                ));
            }
            self.io
                .say(&format!("Wrote the config file: {config_display}\n"));
        } else {
            self.io.say(
                "Keeping the config file as it is. Checking it and registering mcp-rs with it.\n",
            );
        }

        // 3. 確かめる
        self.io
            .say("Checking the config file (like `mcp-rs check`)...");
        let report = crate::check::check(&crate::check::CheckArgs {
            config: Some(config_display.clone()),
            offline: false,
            help: false,
        })
        .await;
        self.io.say(&report.render());
        if report.count(crate::check::Status::Error) > 0 {
            self.io.say(
                "mcp-rs would not work with this config file yet. Fix the errors above (for example, run mcp-rs init again and check the URL, the username and the Application Password), then run mcp-rs init again to register it.",
            );
            return Ok(false);
        }

        // 4. クライアントに登録する
        let exe = std::env::current_exe()?;
        let launch = Launch {
            exe: exe.display().to_string(),
            config: config_display,
        };
        self.io.say(&format!(
            "MCP clients will start mcp-rs as:\n  {} --config {}\nIf you move mcp-rs or the config file, run mcp-rs init again.\n",
            launch.exe, launch.config
        ));
        let mut restart = Vec::new();
        if self.register_claude_desktop(&launch)? {
            restart.push("Claude Desktop: quit it from the system tray (menu bar on macOS) and start it again. Closing the window leaves it running");
        }
        if self.register_cli(&launch, &Cli::CLAUDE_CODE)? {
            restart.push("Claude Code: start a new session, then check with /mcp");
        }
        if self.register_cli(&launch, &Cli::COPILOT)? {
            restart.push("VS Code (Copilot): restart VS Code and start a new chat, then ask it to call mcp_rs_status");
        }
        if restart.is_empty() {
            self.io.say("\nmcp-rs was not registered with any client.");
        } else {
            self.io.say("\nDone. To start using mcp-rs:");
            for line in restart {
                self.io.say(&format!("  - {line}"));
            }
        }
        Ok(true)
    }

    /// サイトのURLを聞き、資格情報を資格情報ストアに入れる（入っていれば
    /// 使うかを聞く）。設定ファイルに書くURLと、資格情報の名前を返す
    fn wordpress_credentials(&mut self) -> Result<(String, String), Box<dyn std::error::Error>> {
        self.io.say("WordPress site");
        self.io.say("  mcp-rs signs in with an Application Password (WordPress: Users > Profile > Application Passwords).");
        let url = loop {
            let answer = self
                .io
                .ask("Site URL (for example https://example.com): ")?;
            match normalize_site_url(&answer) {
                Some(url) => break url,
                None => self
                    .io
                    .say("Enter a URL that starts with https:// or http://."),
            }
        };
        let suggested = default_entry_name(&url);
        let answer = self.io.ask(&format!(
            "Name for these credentials in the OS credential store [{suggested}]: "
        ))?;
        let entry = if answer.is_empty() { suggested } else { answer };

        match load_wordpress_credentials(self.store, &entry) {
            Ok(stored) if site_origin(&url).as_deref() == Some(stored.site_url.as_str()) => {
                let reuse = confirm(
                    self.io,
                    &format!(
                        "Credentials named '{entry}' for {} (user {}) are already stored. Use them?",
                        stored.site_url, stored.username
                    ),
                    true,
                )?;
                if reuse {
                    return Ok((url, entry));
                }
            }
            Ok(_) => {
                if !confirm(
                    self.io,
                    &format!(
                        "Credentials named '{entry}' are stored for another site. Replace them?"
                    ),
                    false,
                )? {
                    return Err(format!(
                        "kept the credentials named '{entry}'. Run mcp-rs init again and choose another name"
                    )
                    .into());
                }
            }
            Err(KeyringError::NotFound(_)) => {}
            // 前の形の登録・mcp-rsのものではない登録は、上書きを聞く
            Err(KeyringError::Invalid(_)) | Err(KeyringError::MissingSiteUrl(_)) => {
                if !confirm(
                    self.io,
                    &format!("An entry named '{entry}' exists but cannot be used. Replace it?"),
                    false,
                )? {
                    return Err(format!(
                        "kept the entry named '{entry}'. Run mcp-rs init again and choose another name"
                    )
                    .into());
                }
            }
            Err(e) => return Err(e.into()),
        }

        let username = loop {
            let answer = self.io.ask("WordPress username: ")?;
            if !answer.is_empty() {
                break answer;
            }
            self.io.say("The username is empty.");
        };
        let application_password = loop {
            let answer = self
                .io
                .ask_hidden("Application Password (not shown while typing): ")?;
            if !answer.is_empty() {
                break answer;
            }
            self.io.say("The Application Password is empty.");
        };
        store_wordpress_credentials(
            self.store,
            &entry,
            &WordPressKeyringCredentials {
                site_url: url.clone(),
                username,
                application_password,
            },
        )?;
        self.io.say(&format!(
            "Stored the credentials as '{entry}' in the OS credential store.\n"
        ));
        Ok((url, entry))
    }

    /// Claude Desktopの設定ファイルに足す。登録したら`true`
    fn register_claude_desktop(
        &mut self,
        launch: &Launch,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(path) = claude_desktop_config_path() else {
            return Ok(false);
        };
        if !path.parent().is_some_and(Path::is_dir) {
            self.io.say("Claude Desktop: not found (skipped).");
            return Ok(false);
        }
        if !confirm(
            self.io,
            &format!("Register mcp-rs with Claude Desktop ({})?", path.display()),
            true,
        )? {
            self.io.say(&format!(
                "Skipped. To add it yourself, put this under \"mcpServers\" in {}:\n{}\n",
                path.display(),
                claude_desktop_entry_text(launch)
            ));
            return Ok(false);
        }
        let mut replace = false;
        if claude_desktop_has_entry(&path)? {
            replace = confirm(
                self.io,
                &format!(
                    "Claude Desktop already has a server named \"{SERVER_NAME}\". Replace it?"
                ),
                true,
            )?;
            if !replace {
                self.io.say("Kept the Claude Desktop settings as they are.");
                return Ok(false);
            }
        }
        let backup = add_to_claude_desktop(&path, launch, replace)?;
        if let Some(backup) = backup {
            self.io.say(&format!(
                "Claude Desktop: registered (the previous settings are kept as {}).",
                backup.display()
            ));
        } else {
            self.io.say("Claude Desktop: registered.");
        }
        Ok(true)
    }

    /// `claude`・`copilot`のコマンドで登録する。登録したら`true`
    fn register_cli(
        &mut self,
        launch: &Launch,
        cli: &Cli,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let manual = cli.add_command_text(launch);
        let Some(program) = find_program(cli.program) else {
            self.io.say(&format!(
                "{}: the `{}` command was not found (skipped). If you use it, register mcp-rs with:\n  {manual}\n",
                cli.label, cli.program
            ));
            return Ok(false);
        };
        if !confirm(
            self.io,
            &format!("Register mcp-rs with {}?", cli.label),
            true,
        )? {
            self.io
                .say(&format!("Skipped. To register it yourself:\n  {manual}\n"));
            return Ok(false);
        }
        // 登録の内容（環境変数等）は表示しない
        let exists = Command::new(&program)
            .args(["mcp", "get", SERVER_NAME])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if exists {
            if !confirm(
                self.io,
                &format!(
                    "{} already has a server named \"{SERVER_NAME}\". Replace it?",
                    cli.label
                ),
                true,
            )? {
                self.io
                    .say(&format!("Kept the {} settings as they are.", cli.label));
                return Ok(false);
            }
            let removed = Command::new(&program)
                .args(cli.remove_args())
                .stdin(Stdio::null())
                .output()?;
            if !removed.status.success() {
                self.io.say(&format!(
                    "{}: could not remove the existing \"{SERVER_NAME}\": {}\nRemove it with `{} {}`, then register mcp-rs with:\n  {manual}\n",
                    cli.label,
                    String::from_utf8_lossy(&removed.stderr).trim(),
                    cli.program,
                    cli.remove_args().join(" ")
                ));
                return Ok(false);
            }
        }
        let added = Command::new(&program)
            .args(cli.add_args(launch))
            .stdin(Stdio::null())
            .output()?;
        if added.status.success() {
            self.io.say(&format!("{}: registered.", cli.label));
            Ok(true)
        } else {
            self.io.say(&format!(
                "{}: registering failed: {}\nRegister it yourself with:\n  {manual}\n",
                cli.label,
                String::from_utf8_lossy(&added.stderr).trim()
            ));
            Ok(false)
        }
    }
}

/// クライアントが起動するmcp-rs
struct Launch {
    exe: String,
    config: String,
}

impl Launch {
    /// mcp-rsに渡す引数
    fn args(&self) -> Vec<String> {
        vec!["--config".to_string(), self.config.clone()]
    }
}

/// 登録のコマンドを持つクライアント
struct Cli {
    label: &'static str,
    program: &'static str,
    /// `mcp add`の、名前より前に付ける引数
    add_options: &'static [&'static str],
    /// `mcp remove`の、名前より後に付ける引数
    remove_options: &'static [&'static str],
}

impl Cli {
    /// Claude Code。どのフォルダで開いても使えるよう、ユーザーの設定に入れる
    /// （`--scope project`は、共有の`.mcp.json`に書くので使わない）
    const CLAUDE_CODE: Cli = Cli {
        label: "Claude Code",
        program: "claude",
        add_options: &["--scope", "user"],
        remove_options: &["--scope", "user"],
    };
    /// Copilot CLI（VS CodeのCopilotのチャットも、これの登録を使う）。
    /// `~/.copilot/mcp-config.json`に書く
    const COPILOT: Cli = Cli {
        label: "GitHub Copilot (VS Code)",
        program: "copilot",
        add_options: &[],
        remove_options: &[],
    };

    fn add_args(&self, launch: &Launch) -> Vec<String> {
        let mut args: Vec<String> = ["mcp", "add"].iter().map(|s| s.to_string()).collect();
        args.extend(self.add_options.iter().map(|s| s.to_string()));
        args.push(SERVER_NAME.to_string());
        args.push("--".to_string());
        args.push(launch.exe.clone());
        args.extend(launch.args());
        args
    }

    fn remove_args(&self) -> Vec<String> {
        let mut args: Vec<String> = ["mcp", "remove", SERVER_NAME]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.extend(self.remove_options.iter().map(|s| s.to_string()));
        args
    }

    /// 手で実行するときのコマンド
    fn add_command_text(&self, launch: &Launch) -> String {
        let args: Vec<String> = self.add_args(launch).iter().map(|a| quote_arg(a)).collect();
        format!("{} {}", self.program, args.join(" "))
    }
}

/// 表示するコマンドの引数を、空白等があれば`"`で囲む
fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
        arg.to_string()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

/// 入力されたサイトのURLを、設定ファイルに書く形にする（前後の空白と、
/// 末尾の`/`を除く）。http(s)のURLでなければ`None`
fn normalize_site_url(input: &str) -> Option<String> {
    let url = input.trim().trim_end_matches('/');
    site_origin(url)?;
    Some(url.to_string())
}

/// 資格情報の名前の既定（サイトのホスト名。既定でないポートがあれば付ける）
fn default_entry_name(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            let host = parsed.host_str()?.to_string();
            Some(match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host,
            })
        })
        .unwrap_or_else(|| "wordpress".to_string())
}

/// 書く設定ファイル。WordPressだけを使い、資格情報はOSの資格情報ストアから読む
fn config_toml(url: &str, entry: &str) -> String {
    let quote = |s: &str| toml::Value::String(s.to_string()).to_string();
    format!(
        "# mcp-rs config file, written by `mcp-rs init` (mcp-rs {version}).
# Check it with: mcp-rs check --config <this file>
# Settings: https://github.com/n-takatsu/mcp-rs#basic-configuration

[transport]
type = \"stdio\"

[server]
# Logs go to a file, never to standard output in STDIO mode
log_level = \"warn\"

[handlers.wordpress]
enabled = true
url = {url}
# The username and the Application Password are kept in the OS credential
# store under this name (mcp-rs wordpress-keyring show {entry_plain})
keyring = {entry}
",
        version = env!("CARGO_PKG_VERSION"),
        url = quote(url),
        entry = quote(entry),
        entry_plain = entry,
    )
}

/// `path`に`content`を書く。フォルダが無ければ作る。前のファイルがあれば、
/// 別の名前で残し、そのパスを返す
fn write_config_file(path: &Path, content: &str) -> io::Result<Option<PathBuf>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let backup = if path.exists() {
        let backup = backup_path(path);
        std::fs::copy(path, &backup)?;
        Some(backup)
    } else {
        None
    };
    std::fs::write(path, content)?;
    Ok(backup)
}

/// `path`の控えのパス（`<name>.bak-<日時>`）
fn backup_path(path: &Path) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.bak-{stamp}"))
}

/// Claude Desktopの設定ファイルの場所。Claude Desktopが無いOS（Linux）は`None`
fn claude_desktop_config_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("Claude")
                .join("claude_desktop_config.json"),
        )
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
        )
    } else {
        None
    }
}

/// Claude Desktopに足す1件
fn claude_desktop_entry(launch: &Launch) -> serde_json::Value {
    serde_json::json!({
        "command": launch.exe,
        "args": launch.args(),
    })
}

/// 手で足すときに表示する形
fn claude_desktop_entry_text(launch: &Launch) -> String {
    let entry = serde_json::json!({ SERVER_NAME: claude_desktop_entry(launch) });
    serde_json::to_string_pretty(&entry).unwrap_or_default()
}

/// Claude Desktopの設定ファイルを読む。無ければ空。内容は表示しない
/// （ほかのMCPサーバーの環境変数に、秘密が入っていることがある）
fn read_claude_desktop_config(
    path: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = std::fs::read_to_string(path)?;
    let content = content.trim_start_matches('\u{feff}');
    if content.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    let value: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        format!(
            "cannot read {} as JSON (line {}, column {}). Fix it in Claude Desktop (Settings > Developer > Edit Config), then run mcp-rs init again",
            path.display(),
            e.line(),
            e.column()
        )
    })?;
    if !value.is_object() {
        return Err(format!("{} is not a JSON object", path.display()).into());
    }
    Ok(value)
}

/// Claude Desktopに、もう`mcp-rs`の登録があるか
fn claude_desktop_has_entry(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(read_claude_desktop_config(path)?
        .get("mcpServers")
        .and_then(|servers| servers.get(SERVER_NAME))
        .is_some())
}

/// Claude Desktopの設定ファイルの`mcpServers`に、mcp-rsを足す。ほかの項目と
/// 順番は変えない。前のファイルは別の名前で残し、そのパスを返す。`replace`が
/// 無いのに、もう登録があればエラー
fn add_to_claude_desktop(
    path: &Path,
    launch: &Launch,
    replace: bool,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let mut config = read_claude_desktop_config(path)?;
    let root = config
        .as_object_mut()
        .ok_or("the Claude Desktop config file is not a JSON object")?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or("\"mcpServers\" in the Claude Desktop config file is not a JSON object")?;
    if servers.contains_key(SERVER_NAME) && !replace {
        return Err(format!("Claude Desktop already has a server named \"{SERVER_NAME}\"").into());
    }
    servers.insert(SERVER_NAME.to_string(), claude_desktop_entry(launch));
    let content = serde_json::to_string_pretty(&config)? + "\n";
    let backup = if path.exists() {
        let backup = backup_path(path);
        std::fs::copy(path, &backup)?;
        Some(backup)
    } else {
        None
    };
    std::fs::write(path, content)?;
    Ok(backup)
}

/// `PATH`から、コマンドのファイルを探す。Windowsでは`.exe`・`.cmd`・`.bat`も
/// 探す（npmで入れた`claude.cmd`等。`Command::new`は`.exe`しか補わない）
fn find_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: &[&str] = if cfg!(windows) {
        &["exe", "cmd", "bat"]
    } else {
        &[""]
    };
    find_program_in(name, std::env::split_paths(&path), extensions)
}

fn find_program_in(
    name: &str,
    dirs: impl Iterator<Item = PathBuf>,
    extensions: &[&str],
) -> Option<PathBuf> {
    for dir in dirs {
        for extension in extensions {
            let candidate = if extension.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{name}.{extension}"))
            };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::keyring_store::MemoryStore;
    use std::collections::VecDeque;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn launch() -> Launch {
        Launch {
            exe: "C:/Users/me/.mcp-rs/bin/mcp-rs.exe".to_string(),
            config: "C:/Users/me/.config/mcp-rs/config.toml".to_string(),
        }
    }

    /// 決めた答えを順に返し、表示を覚える
    #[derive(Default)]
    struct Script {
        answers: VecDeque<String>,
        shown: Vec<String>,
    }

    impl Script {
        fn new(answers: &[&str]) -> Self {
            Self {
                answers: answers.iter().map(|s| s.to_string()).collect(),
                shown: Vec::new(),
            }
        }
    }

    impl Prompt for Script {
        fn ask(&mut self, question: &str) -> io::Result<String> {
            self.shown.push(question.to_string());
            self.answers
                .pop_front()
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no more answers"))
        }
        fn ask_hidden(&mut self, question: &str) -> io::Result<Zeroizing<String>> {
            self.ask(question).map(Zeroizing::new)
        }
        fn say(&mut self, text: &str) {
            self.shown.push(text.to_string());
        }
    }

    fn setup<'a>(io: &'a mut Script, store: &'a MemoryStore) -> Setup<'a> {
        Setup {
            io,
            store,
            config_path: PathBuf::from("unused.toml"),
        }
    }

    #[test]
    fn test_parse_args() {
        assert_eq!(parse_args(&[]), Ok(InitArgs::default()));
        assert!(parse_args(&args(&["--help"])).unwrap().help);
        assert_eq!(
            parse_args(&args(&["--config", "a.toml"])).unwrap().config,
            Some("a.toml".to_string())
        );
        assert_eq!(
            parse_args(&args(&["--config=b.toml"])).unwrap().config,
            Some("b.toml".to_string())
        );
        assert!(parse_args(&args(&["--config"])).is_err());
        assert!(parse_args(&args(&["--config="])).is_err());
        // 知らない引数は、表示しない（パスワードを渡してしまったとき）
        let err = parse_args(&args(&["secret-password"])).unwrap_err();
        assert!(!err.contains("secret-password"), "{err}");
    }

    #[test]
    fn test_site_url_and_entry_name() {
        assert_eq!(
            normalize_site_url(" https://example.com/ "),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            normalize_site_url("https://example.com/blog"),
            Some("https://example.com/blog".to_string())
        );
        assert_eq!(normalize_site_url("example.com"), None);
        assert_eq!(normalize_site_url("ftp://example.com"), None);
        assert_eq!(default_entry_name("https://example.com"), "example.com");
        assert_eq!(
            default_entry_name("http://localhost:8080/wp"),
            "localhost:8080"
        );
    }

    /// 書いた設定ファイルを、起動と同じ形で読める
    #[test]
    fn test_config_toml_loads() {
        let text = config_toml("https://example.com", "example \"site\"");
        let config: crate::config::McpConfig = toml::from_str(&text).unwrap();
        let wordpress = config.handlers.wordpress.unwrap();
        assert_eq!(wordpress.enabled, Some(true));
        assert_eq!(wordpress.url, "https://example.com");
        assert_eq!(wordpress.keyring.as_deref(), Some("example \"site\""));
        assert_eq!(config.transport.transport_type.as_deref(), Some("stdio"));
    }

    #[test]
    fn test_write_config_file_keeps_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new").join("config.toml");
        assert_eq!(write_config_file(&path, "first").unwrap(), None);
        let backup = write_config_file(&path, "second").unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "first");
    }

    #[test]
    fn test_add_to_claude_desktop_keeps_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        let before = r#"{
  "preferences": {"z": 1, "a": 2},
  "mcpServers": {
    "other": {"command": "other.exe", "env": {"TOKEN": "secret"}}
  },
  "after": true
}"#;
        std::fs::write(&path, before).unwrap();
        assert!(!claude_desktop_has_entry(&path).unwrap());

        let backup = add_to_claude_desktop(&path, &launch(), false)
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read_to_string(backup).unwrap(), before);
        let after = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&after).unwrap();
        // ほかの項目と順番はそのまま
        let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["preferences", "mcpServers", "after"]);
        let pref_keys: Vec<&String> = value["preferences"].as_object().unwrap().keys().collect();
        assert_eq!(pref_keys, ["z", "a"]);
        assert_eq!(value["mcpServers"]["other"]["env"]["TOKEN"], "secret");
        assert_eq!(
            value["mcpServers"]["mcp-rs"],
            serde_json::json!({
                "command": "C:/Users/me/.mcp-rs/bin/mcp-rs.exe",
                "args": ["--config", "C:/Users/me/.config/mcp-rs/config.toml"]
            })
        );
        assert!(claude_desktop_has_entry(&path).unwrap());

        // もう登録があるときは、replaceが要る
        assert!(add_to_claude_desktop(&path, &launch(), false).is_err());
        assert!(add_to_claude_desktop(&path, &launch(), true).is_ok());
    }

    #[test]
    fn test_add_to_claude_desktop_without_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        assert_eq!(
            add_to_claude_desktop(&path, &launch(), false).unwrap(),
            None
        );
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(value["mcpServers"]["mcp-rs"].is_object());
    }

    #[test]
    fn test_add_to_claude_desktop_refuses_broken_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        std::fs::write(&path, "{ \"mcpServers\": { \"x\": SECRET } ").unwrap();
        let err = add_to_claude_desktop(&path, &launch(), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("line"), "{err}");
        // 内容は出さず、ファイルも変えない
        assert!(!err.contains("SECRET"), "{err}");
        assert!(std::fs::read_to_string(&path).unwrap().contains("SECRET"));
    }

    #[test]
    fn test_cli_commands() {
        let launch = Launch {
            exe: "C:/Program Files/mcp-rs/mcp-rs.exe".to_string(),
            config: "C:/cfg/config.toml".to_string(),
        };
        assert_eq!(
            Cli::CLAUDE_CODE.add_args(&launch),
            args(&[
                "mcp",
                "add",
                "--scope",
                "user",
                "mcp-rs",
                "--",
                "C:/Program Files/mcp-rs/mcp-rs.exe",
                "--config",
                "C:/cfg/config.toml"
            ])
        );
        assert_eq!(
            Cli::CLAUDE_CODE.remove_args(),
            args(&["mcp", "remove", "mcp-rs", "--scope", "user"])
        );
        assert_eq!(
            Cli::COPILOT.add_command_text(&launch),
            "copilot mcp add mcp-rs -- \"C:/Program Files/mcp-rs/mcp-rs.exe\" --config C:/cfg/config.toml"
        );
        assert_eq!(
            Cli::COPILOT.remove_args(),
            args(&["mcp", "remove", "mcp-rs"])
        );
    }

    #[test]
    fn test_find_program_in() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("tool.cmd"), "").unwrap();
        let dirs = || vec![PathBuf::from("/nonexistent"), dir.path().to_path_buf()].into_iter();
        assert_eq!(
            find_program_in("tool", dirs(), &["exe", "cmd"]),
            Some(dir.path().join("tool.cmd"))
        );
        assert_eq!(find_program_in("tool", dirs(), &["exe"]), None);
        assert_eq!(find_program_in("missing", dirs(), &["exe", "cmd"]), None);
    }

    #[test]
    fn test_confirm() {
        let mut io = Script::new(&["", "maybe", "N"]);
        assert!(confirm(&mut io, "Go?", true).unwrap());
        assert!(!confirm(&mut io, "Go?", true).unwrap());
        assert!(io.shown.iter().any(|s| s == "Answer y or n."));
    }

    /// 新しい資格情報を聞いて、資格情報ストアに入れる
    #[test]
    fn test_wordpress_credentials_stores_new_ones() {
        let store = MemoryStore::default();
        let mut io = Script::new(&[
            "example.com",          // URLでない
            "https://example.com/", // 末尾の/は除く
            "",                     // 名前は既定（example.com）
            "editor",
            "abcd efgh ijkl",
        ]);
        let (url, entry) = setup(&mut io, &store).wordpress_credentials().unwrap();
        assert_eq!(url, "https://example.com");
        assert_eq!(entry, "example.com");
        let stored = load_wordpress_credentials(&store, "example.com").unwrap();
        assert_eq!(stored.site_url, "https://example.com");
        assert_eq!(stored.username, "editor");
        assert_eq!(*stored.application_password, "abcd efgh ijkl");
        // パスワードは表示しない
        assert!(!io.shown.iter().any(|s| s.contains("abcd efgh ijkl")));
    }

    /// 同じサイトの資格情報が入っていれば、使うかを聞く
    #[test]
    fn test_wordpress_credentials_reuses_stored_ones() {
        let store = MemoryStore::default();
        store_wordpress_credentials(
            &store,
            "example.com",
            &WordPressKeyringCredentials {
                site_url: "https://example.com".to_string(),
                username: "editor".to_string(),
                application_password: Zeroizing::new("old password".to_string()),
            },
        )
        .unwrap();
        let mut io = Script::new(&["https://example.com", "", "y"]);
        let (_, entry) = setup(&mut io, &store).wordpress_credentials().unwrap();
        assert_eq!(entry, "example.com");
        assert_eq!(
            *load_wordpress_credentials(&store, "example.com")
                .unwrap()
                .application_password,
            "old password"
        );
        assert!(!io.shown.iter().any(|s| s.contains("old password")));
    }

    /// 別のサイトの資格情報は、断れば変えない
    #[test]
    fn test_wordpress_credentials_keeps_another_sites_entry() {
        let store = MemoryStore::default();
        store_wordpress_credentials(
            &store,
            "blog",
            &WordPressKeyringCredentials {
                site_url: "https://other.example".to_string(),
                username: "someone".to_string(),
                application_password: Zeroizing::new("pw".to_string()),
            },
        )
        .unwrap();
        let mut io = Script::new(&["https://example.com", "blog", "n"]);
        assert!(setup(&mut io, &store).wordpress_credentials().is_err());
        assert_eq!(
            load_wordpress_credentials(&store, "blog").unwrap().site_url,
            "https://other.example"
        );
    }
}
