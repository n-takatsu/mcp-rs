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
        // この実行で書いた設定の、サイトのURLと資格情報の名前（確認が
        // 失敗したときに、資格情報を入れ直すため）
        let mut written = None;
        if write_config {
            // 資格情報ストアが使えるかを、質問の前に確かめる（PR #583のレビュー）
            check_store(self.store)?;
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
            written = Some((url, entry));
        } else {
            self.io.say(
                "Keeping the config file as it is. Checking it and registering mcp-rs with it.\n",
            );
        }

        // 3. 確かめる。認証が通らなければ、その場で資格情報を入れ直せる
        // （PR #583のレビュー。やり直しで、既定の答えのまま同じ失敗に
        // 戻らないように）
        loop {
            self.io
                .say("Checking the config file (like `mcp-rs check`)...");
            let report = crate::check::check(&crate::check::CheckArgs {
                config: Some(config_display.clone()),
                offline: false,
                help: false,
            })
            .await;
            self.io.say(&report.render());
            if report.count(crate::check::Status::Error) == 0 {
                break;
            }
            // 入れ直しで直るのは、サインインのエラーだけ（URLの誤り・サイトに
            // つながらない等は、入れ直しても直らない。PR #583のレビュー）
            let sign_in_failed = report
                .find("WordPress sign-in")
                .is_some_and(|(status, _)| status == crate::check::Status::Error);
            if let (Some((url, entry)), true) = (&written, sign_in_failed) {
                if confirm(
                    self.io,
                    "If the username or the Application Password was wrong, enter them again?",
                    true,
                )? {
                    let (url, entry) = (url.clone(), entry.clone());
                    self.enter_credentials(&url, &entry)?;
                    continue;
                }
            }
            self.io.say(
                "mcp-rs would not work with this config file yet, so it was not registered with any client. Fix the errors above, then run mcp-rs init again. To enter the credentials again, answer y to replacing the config file and n to using the stored credentials.",
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
            restart.push("Claude Desktop: quit it from the system tray (menu bar on macOS) and start it again. Closing the window leaves it running. If mcp-rs does not appear, check Settings > Developer");
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
                None => self.io.say(
                    "Enter a URL that starts with https:// (mcp-rs connects to WordPress over HTTPS only).",
                ),
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
        self.enter_credentials(&url, &entry)?;
        Ok((url, entry))
    }

    /// ユーザー名とアプリケーションパスワードを聞き、`entry`の名前で入れる
    fn enter_credentials(
        &mut self,
        url: &str,
        entry: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
            entry,
            &WordPressKeyringCredentials {
                site_url: url.to_string(),
                username,
                application_password,
            },
        )?;
        self.io.say(&format!(
            "Stored the credentials as '{entry}' in the OS credential store.\n"
        ));
        Ok(())
    }

    /// Claude Desktopの設定ファイルに足す。登録したら`true`
    fn register_claude_desktop(
        &mut self,
        launch: &Launch,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let paths = select_claude_desktop_configs(&claude_desktop_config_candidates());
        if paths.is_empty() {
            if cfg!(any(target_os = "windows", target_os = "macos")) {
                self.io.say("Claude Desktop: not found (skipped).");
            }
            return Ok(false);
        }
        let list = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if !confirm(
            self.io,
            &format!("Register mcp-rs with Claude Desktop ({list})?"),
            true,
        )? {
            self.io.say(&format!(
                "Skipped. To add it yourself, put this under \"mcpServers\" in {list}:\n{}\n",
                claude_desktop_entry_text(launch)
            ));
            return Ok(false);
        }
        let mut registered = false;
        for path in &paths {
            match self.add_claude_desktop_file(path, launch) {
                Ok(done) => registered |= done,
                Err(e) => self.io.say(&format!(
                    "Claude Desktop: could not update {}: {e}\nTo add it yourself, put this under \"mcpServers\":\n{}\n",
                    path.display(),
                    claude_desktop_entry_text(launch)
                )),
            }
        }
        Ok(registered)
    }

    /// Claude Desktopの設定ファイル1つに足す。足したら`true`
    fn add_claude_desktop_file(
        &mut self,
        path: &Path,
        launch: &Launch,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let config = read_claude_desktop_config(path)?;
        let mut replace = false;
        if claude_desktop_has_entry(&config) {
            replace = confirm(
                self.io,
                &format!(
                    "{} already has a server named \"{SERVER_NAME}\". Replace it?",
                    path.display()
                ),
                true,
            )?;
            if !replace {
                self.io.say("Kept the Claude Desktop settings as they are.");
                return Ok(false);
            }
        }
        let remove = self.choose_duplicates_to_remove(
            "Claude Desktop",
            &other_mcp_rs_servers(&config),
            launch,
        )?;
        match add_to_claude_desktop(path, launch, replace, &remove)? {
            Some(backup) => self.io.say(&format!(
                "Claude Desktop: registered in {} (the previous settings are kept as {}).",
                path.display(),
                backup.display()
            )),
            None => self.io.say(&format!(
                "Claude Desktop: registered in {}.",
                path.display()
            )),
        }
        Ok(true)
    }

    /// 別の名前で登録された mcp-rs（手で登録したもの等）を、1つずつ消すかを
    /// 聞く。消すものの名前を返す。同じ mcp-rs が2つ動くと、同じツールが
    /// 2組並び、AIがどちらを使うか分からなくなる（#575）
    ///
    /// 同じ設定ファイルを使うものは重複なので、既定は「はい」。別の設定
    /// ファイル（2つ目のサイト・データベース用等）のものは、意図して並べて
    /// いることがあるので、そのパスを見せて、既定は「いいえ」にする（PR #583
    /// のレビュー。Claude Code・Copilotの登録は、消すと控えが残らない）
    fn choose_duplicates_to_remove(
        &mut self,
        client: &str,
        others: &[OtherServer],
        launch: &Launch,
    ) -> io::Result<Vec<String>> {
        let mut remove = Vec::new();
        for other in others {
            let name = &other.name;
            let same_config = same_config_file(other.config.as_deref(), &launch.config);
            let question = if same_config {
                format!(
                    "{client} also starts mcp-rs as \"{name}\" with the same config file. Remove it, so that only one mcp-rs runs? (otherwise the same tools appear twice)"
                )
            } else {
                format!(
                    "{client} also starts mcp-rs as \"{name}\" with another config file ({}). If it is the setup you are replacing, remove it (otherwise the same tools may appear twice). Remove it?",
                    other.config.as_deref().unwrap_or("~/.config/mcp-rs/config.toml")
                )
            };
            if confirm(self.io, &question, same_config)? {
                remove.push(name.clone());
            } else if same_config {
                self.io.say(&format!(
                    "Kept \"{name}\". {client} will start two mcp-rs servers with the same tools; remove one of them later."
                ));
            } else {
                self.io.say(&format!("Kept \"{name}\"."));
            }
        }
        Ok(remove)
    }

    /// `claude`・`copilot`のコマンドで登録する。登録したら`true`。
    /// コマンドが動かなくても、ほかのクライアントの登録に進む
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
            // 消せなくても、登録を試す（別のスコープの登録は、消さずに
            // ユーザーのスコープへ足せることがある。PR #583のレビュー）
            if let Err(reason) = run_quietly(&program, &cli.remove_args(SERVER_NAME)) {
                self.io.say(&format!(
                    "{}: could not remove the existing \"{SERVER_NAME}\" ({reason}). Trying to register mcp-rs anyway.",
                    cli.label
                ));
            }
        }
        // 別の名前のmcp-rs（設定ファイルを読んで探す。内容は表示しない）
        let others = cli
            .config_file()
            .and_then(|path| read_json_file(&path).ok())
            .map(|config| other_mcp_rs_servers(&config))
            .unwrap_or_default();
        let remove = self.choose_duplicates_to_remove(cli.label, &others, launch)?;
        match run_quietly(&program, &cli.add_args(launch)) {
            Ok(()) => {
                self.io.say(&format!("{}: registered.", cli.label));
                for name in remove {
                    match run_quietly(&program, &cli.remove_args(&name)) {
                        Ok(()) => self.io.say(&format!("{}: removed \"{name}\".", cli.label)),
                        Err(reason) => self.io.say(&format!(
                            "{}: could not remove \"{name}\" ({reason}). Remove it with `{} {}`.",
                            cli.label,
                            cli.program,
                            cli.remove_args(&name).join(" ")
                        )),
                    }
                }
                Ok(true)
            }
            Err(reason) => {
                self.io.say(&format!(
                    "{}: registering failed ({reason}).\nRegister it yourself with:\n  {manual}\n",
                    cli.label
                ));
                Ok(false)
            }
        }
    }
}

/// 資格情報ストアが使えるかを確かめる（`os-keyring`無しのビルド、
/// Secret Serviceの無いLinux等では使えない）
fn check_store(store: &dyn SecretStore) -> Result<(), Box<dyn std::error::Error>> {
    match store.get("mcp-rs-init-check") {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "the OS credential store cannot be used ({e}). mcp-rs init keeps the WordPress credentials there; see the README for other ways (environment variables or encrypted credentials)"
        )
        .into()),
    }
}

/// コマンドを動かす。出力は表示しない（登録の内容に秘密があることがある）。
/// 失敗したら、エラーの出力の最後の行を返す
fn run_quietly(program: &Path, args: &[String]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run {}: {e}", program.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let last = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no error message")
        .trim()
        .to_string();
    Err(last)
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
    /// 登録を書く設定ファイル（別の名前のmcp-rsを探すために読む）
    config_file: fn() -> Option<PathBuf>,
}

impl Cli {
    /// Claude Code。どのフォルダで開いても使えるよう、ユーザーの設定に入れる
    /// （`--scope project`は、共有の`.mcp.json`に書くので使わない）
    const CLAUDE_CODE: Cli = Cli {
        label: "Claude Code",
        program: "claude",
        add_options: &["--scope", "user"],
        remove_options: &["--scope", "user"],
        config_file: claude_code_config_file,
    };
    /// Copilot CLI（VS CodeのCopilotのチャットも、これの登録を使う）。
    /// `~/.copilot/mcp-config.json`に書く
    const COPILOT: Cli = Cli {
        label: "GitHub Copilot (VS Code)",
        program: "copilot",
        add_options: &[],
        remove_options: &[],
        config_file: copilot_config_file,
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

    fn remove_args(&self, name: &str) -> Vec<String> {
        let mut args: Vec<String> = ["mcp", "remove", name]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.extend(self.remove_options.iter().map(|s| s.to_string()));
        args
    }

    fn config_file(&self) -> Option<PathBuf> {
        (self.config_file)()
    }

    /// 手で実行するときのコマンド
    fn add_command_text(&self, launch: &Launch) -> String {
        let args: Vec<String> = self.add_args(launch).iter().map(|a| quote_arg(a)).collect();
        format!("{} {}", self.program, args.join(" "))
    }
}

/// ホームフォルダ
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Claude Codeのユーザーの設定（`--scope user`の登録）。
/// `CLAUDE_CONFIG_DIR`があれば、その中
fn claude_code_config_file() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir).join(".claude.json")),
        None => Some(home_dir()?.join(".claude.json")),
    }
}

/// Copilot CLIのユーザーの設定
fn copilot_config_file() -> Option<PathBuf> {
    Some(home_dir()?.join(".copilot").join("mcp-config.json"))
}

/// `command`が mcp-rs の実行ファイルか（ファイル名で見分ける）
fn is_mcp_rs_command(command: &str) -> bool {
    let file = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    file == "mcp-rs" || file == "mcp-rs.exe"
}

/// 別の名前で mcp-rs を起動する登録
#[derive(Debug, PartialEq, Eq)]
struct OtherServer {
    name: String,
    /// `--config`の値（無ければ`None`。`~/.config/mcp-rs/config.toml`を読む）
    config: Option<String>,
}

/// `mcpServers`のうち、`mcp-rs`以外の名前で mcp-rs を起動するもの
fn other_mcp_rs_servers(config: &serde_json::Value) -> Vec<OtherServer> {
    let Some(servers) = config
        .get("mcpServers")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    servers
        .iter()
        .filter(|(name, server)| {
            name.as_str() != SERVER_NAME
                && server
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(is_mcp_rs_command)
        })
        .map(|(name, server)| {
            let args: Vec<&str> = server
                .get("args")
                .and_then(serde_json::Value::as_array)
                .map(|args| args.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            OtherServer {
                name: name.clone(),
                config: config_arg(&args),
            }
        })
        .collect()
}

/// mcp-rsの引数の`--config`の値
fn config_arg(args: &[&str]) -> Option<String> {
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if *arg == "--config" {
            return rest.next().map(|path| path.to_string());
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(path.to_string());
        }
    }
    None
}

/// 2つの設定ファイルが同じか（`None`は`--config`無し、つまり
/// `~/.config/mcp-rs/config.toml`）。`~/`を展開し、区切り文字をそろえ、
/// Windowsでは大文字・小文字を区別しない
fn same_config_file(other: Option<&str>, ours: &str) -> bool {
    let normalize = |path: &str| {
        let path = crate::config::expand_config_path(path.trim()).replace('\\', "/");
        if cfg!(windows) {
            path.to_lowercase()
        } else {
            path
        }
    };
    let other = match other {
        Some(path) => path.to_string(),
        None => match crate::config::user_config_path() {
            Some(path) => path,
            None => return false,
        },
    };
    normalize(&other) == normalize(ours)
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
/// 末尾の`/`を除く）。`https://`のURLでなければ`None`（WordPressの
/// ハンドラーは、`https://`でしか起動しない。PR #583のレビュー）
fn normalize_site_url(input: &str) -> Option<String> {
    let url = input.trim().trim_end_matches('/');
    site_origin(url)?;
    // ハンドラーと同じく、小文字の`https://`だけを受け付ける
    url.starts_with("https://").then(|| url.to_string())
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
# Settings: https://n-takatsu.github.io/mcp-rs/docs/guides/#config-file

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

/// Claude Desktopの設定ファイルの名前
const CLAUDE_DESKTOP_CONFIG: &str = "claude_desktop_config.json";

/// Claude Desktopの設定ファイルがあり得る場所（先にあるものを優先する）。
/// Claude Desktopが無いOS（Linux）は空。
///
/// Windowsは`%APPDATA%\Claude`。Microsoft Store版（MSIX）は、
/// `%LOCALAPPDATA%\Packages\Claude_<id>\LocalCache\Roaming\Claude`を読む
/// ことがあるという報告がある（PR #583のレビュー）ので、そちらも見る
fn claude_desktop_config_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(target_os = "windows") {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            candidates.push(
                PathBuf::from(appdata)
                    .join("Claude")
                    .join(CLAUDE_DESKTOP_CONFIG),
            );
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let packages = PathBuf::from(local).join("Packages");
            if let Ok(entries) = std::fs::read_dir(&packages) {
                let mut packaged: Vec<PathBuf> = entries
                    .flatten()
                    .filter(|entry| entry.file_name().to_string_lossy().starts_with("Claude_"))
                    .map(|entry| {
                        entry
                            .path()
                            .join("LocalCache")
                            .join("Roaming")
                            .join("Claude")
                            .join(CLAUDE_DESKTOP_CONFIG)
                    })
                    .collect();
                packaged.sort();
                candidates.extend(packaged);
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Claude")
                    .join(CLAUDE_DESKTOP_CONFIG),
            );
        }
    }
    candidates
}

/// 書く設定ファイルを選ぶ。あるファイルはすべて（どちらをClaude Desktopが
/// 読むか分からないので、両方に足す）。1つも無ければ、`Claude`のフォルダが
/// ある最初の場所（Claude Desktopを入れたが、設定をまだ保存していない）
fn select_claude_desktop_configs(candidates: &[PathBuf]) -> Vec<PathBuf> {
    let existing: Vec<PathBuf> = candidates.iter().filter(|p| p.is_file()).cloned().collect();
    if !existing.is_empty() {
        return existing;
    }
    candidates
        .iter()
        .find(|p| p.parent().is_some_and(Path::is_dir))
        .cloned()
        .into_iter()
        .collect()
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
    read_json_file(path).map_err(|e| {
        format!(
            "{e}. Fix it in Claude Desktop (Settings > Developer > Edit Config), then run mcp-rs init again"
        )
        .into()
    })
}

/// JSONのファイルを読む。無ければ空のオブジェクト。エラーの文に内容は入れない
fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let content = content.trim_start_matches('\u{feff}');
    if content.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    let value: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        format!(
            "cannot read {} as JSON (line {}, column {})",
            path.display(),
            e.line(),
            e.column()
        )
    })?;
    if !value.is_object() {
        return Err(format!("{} is not a JSON object", path.display()));
    }
    Ok(value)
}

/// Claude Desktopの設定に、もう`mcp-rs`の登録があるか
fn claude_desktop_has_entry(config: &serde_json::Value) -> bool {
    config
        .get("mcpServers")
        .and_then(|servers| servers.get(SERVER_NAME))
        .is_some()
}

/// Claude Desktopの設定ファイルの`mcpServers`に、mcp-rsを足す。ほかの項目と
/// 順番は変えない。`remove`の名前の登録（別の名前のmcp-rs）は消す。前の
/// ファイルは別の名前で残し、そのパスを返す。`replace`が無いのに、もう
/// 登録があればエラー
fn add_to_claude_desktop(
    path: &Path,
    launch: &Launch,
    replace: bool,
    remove: &[String],
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
    for name in remove {
        // `remove`は、preserve_orderでは順番を変える（最後の項目と入れ替える）
        servers.shift_remove(name);
    }
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
        // mcp-rsは、https://でしかWordPressにつながない（PR #583のレビュー）
        assert_eq!(normalize_site_url("http://example.com"), None);
        assert_eq!(normalize_site_url("HTTPS://example.com"), None);
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
        assert!(!claude_desktop_has_entry(
            &read_claude_desktop_config(&path).unwrap()
        ));

        let backup = add_to_claude_desktop(&path, &launch(), false, &[])
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
        assert!(claude_desktop_has_entry(
            &read_claude_desktop_config(&path).unwrap()
        ));

        // もう登録があるときは、replaceが要る
        assert!(add_to_claude_desktop(&path, &launch(), false, &[]).is_err());
        assert!(add_to_claude_desktop(&path, &launch(), true, &[]).is_ok());
    }

    /// 別の名前で mcp-rs を起動する登録を見つける（#575）
    #[test]
    fn test_other_mcp_rs_servers() {
        let config = serde_json::json!({
            "mcpServers": {
                "mcp-rs": {"command": "C:/x/mcp-rs.exe"},
                "mcp-rs-wordpress": {"command": "C:/Program Files/MCP-rs/mcp-rs.exe"},
                "blog": {"command": "C:\\tools\\MCP-RS.EXE", "args": []},
                "unix": {"command": "/home/me/.mcp-rs/bin/mcp-rs"},
                "other": {"command": "npx", "args": ["mcp-rs"]},
                "similar": {"command": "/usr/bin/mcp-rs-helper"},
                "remote": {"url": "https://example.com/mcp"}
            }
        });
        let names: Vec<String> = other_mcp_rs_servers(&config)
            .into_iter()
            .map(|other| other.name)
            .collect();
        assert_eq!(names, ["mcp-rs-wordpress", "blog", "unix"]);
        assert!(other_mcp_rs_servers(&serde_json::json!({})).is_empty());
    }

    /// `--config`の値を読み、同じ設定ファイルかを見分ける（PR #583のレビュー）
    #[test]
    fn test_config_arg_and_same_config_file() {
        assert_eq!(
            config_arg(&["--config", "a.toml"]),
            Some("a.toml".to_string())
        );
        assert_eq!(config_arg(&["--config=b.toml"]), Some("b.toml".to_string()));
        assert_eq!(config_arg(&[]), None);
        let config = serde_json::json!({"mcpServers": {
            "second-blog": {"command": "mcp-rs", "args": ["--config", "/home/me/second-blog.toml"]}
        }});
        assert_eq!(
            other_mcp_rs_servers(&config),
            [OtherServer {
                name: "second-blog".to_string(),
                config: Some("/home/me/second-blog.toml".to_string())
            }]
        );

        assert!(same_config_file(Some("/home/me/a.toml"), "/home/me/a.toml"));
        assert!(!same_config_file(
            Some("/home/me/second-blog.toml"),
            "/home/me/a.toml"
        ));
        if cfg!(windows) {
            assert!(same_config_file(
                Some("C:/Users/Me/Config.toml"),
                "c:\\users\\me\\config.toml"
            ));
        }
        // --config無しは、~/.config/mcp-rs/config.toml
        if let Some(user_config) = crate::config::user_config_path() {
            assert!(same_config_file(None, &user_config));
            assert!(!same_config_file(None, "/somewhere/else.toml"));
        }
    }

    /// 別の設定ファイルのmcp-rsは、既定では消さない。同じ設定ファイルの
    /// ものは、既定で消す
    #[test]
    fn test_choose_duplicates_to_remove() {
        let store = MemoryStore::default();
        let launch = Launch {
            exe: "mcp-rs".to_string(),
            config: "/home/me/a.toml".to_string(),
        };
        let others = vec![
            OtherServer {
                name: "old".to_string(),
                config: Some("/home/me/a.toml".to_string()),
            },
            OtherServer {
                name: "second-blog".to_string(),
                config: Some("/home/me/second-blog.toml".to_string()),
            },
        ];
        // Enterだけで進める
        let mut io = Script::new(&["", ""]);
        let remove = setup(&mut io, &store)
            .choose_duplicates_to_remove("Claude Code", &others, &launch)
            .unwrap();
        assert_eq!(remove, ["old"]);
        assert!(io
            .shown
            .iter()
            .any(|s| s.contains("/home/me/second-blog.toml") && s.contains("[y/N]")));
    }

    /// 別の名前の登録を消し、ほかの順番は変えない
    #[test]
    fn test_add_to_claude_desktop_removes_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        std::fs::write(
            &path,
            r#"{"mcpServers": {"a": {"command": "a"}, "mcp-rs-wordpress": {"command": "mcp-rs.exe"}, "b": {"command": "b"}, "c": {"command": "c"}}}"#,
        )
        .unwrap();
        add_to_claude_desktop(&path, &launch(), false, &["mcp-rs-wordpress".to_string()]).unwrap();
        let value = read_claude_desktop_config(&path).unwrap();
        let names: Vec<&String> = value["mcpServers"].as_object().unwrap().keys().collect();
        assert_eq!(names, ["a", "b", "c", "mcp-rs"]);
    }

    #[test]
    fn test_add_to_claude_desktop_without_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        assert_eq!(
            add_to_claude_desktop(&path, &launch(), false, &[]).unwrap(),
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
        let err = add_to_claude_desktop(&path, &launch(), false, &[])
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
            Cli::CLAUDE_CODE.remove_args("mcp-rs"),
            args(&["mcp", "remove", "mcp-rs", "--scope", "user"])
        );
        assert_eq!(
            Cli::COPILOT.add_command_text(&launch),
            "copilot mcp add mcp-rs -- \"C:/Program Files/mcp-rs/mcp-rs.exe\" --config C:/cfg/config.toml"
        );
        assert_eq!(
            Cli::COPILOT.remove_args("mcp-rs-wordpress"),
            args(&["mcp", "remove", "mcp-rs-wordpress"])
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

    /// 設定ファイルがあれば、すべてを選ぶ。無ければ、Claudeのフォルダがある
    /// 最初の場所（PR #583のレビュー。Microsoft Store版）
    #[test]
    fn test_select_claude_desktop_configs() {
        let dir = tempfile::tempdir().unwrap();
        let roaming = dir.path().join("Roaming").join("Claude");
        let packaged = dir.path().join("Packages").join("Claude");
        let roaming_file = roaming.join(CLAUDE_DESKTOP_CONFIG);
        let packaged_file = packaged.join(CLAUDE_DESKTOP_CONFIG);
        let candidates = vec![roaming_file.clone(), packaged_file.clone()];

        // Claude Desktopが無い
        assert!(select_claude_desktop_configs(&candidates).is_empty());
        // フォルダだけある（設定をまだ保存していない）
        std::fs::create_dir_all(&packaged).unwrap();
        assert_eq!(
            select_claude_desktop_configs(&candidates),
            vec![packaged_file.clone()]
        );
        std::fs::create_dir_all(&roaming).unwrap();
        assert_eq!(
            select_claude_desktop_configs(&candidates),
            vec![roaming_file.clone()]
        );
        // あるファイルはすべて
        std::fs::write(&packaged_file, "{}").unwrap();
        assert_eq!(
            select_claude_desktop_configs(&candidates),
            vec![packaged_file.clone()]
        );
        std::fs::write(&roaming_file, "{}").unwrap();
        assert_eq!(
            select_claude_desktop_configs(&candidates),
            vec![roaming_file, packaged_file]
        );
    }

    /// 資格情報ストアが使えなければ、質問の前に止める
    #[test]
    fn test_check_store() {
        struct Broken;
        impl SecretStore for Broken {
            fn get(&self, _: &str) -> Result<Option<Zeroizing<String>>, KeyringError> {
                Err(KeyringError::Unavailable("no Secret Service".to_string()))
            }
            fn set(&self, _: &str, _: &str) -> Result<(), KeyringError> {
                unreachable!()
            }
            fn delete(&self, _: &str) -> Result<bool, KeyringError> {
                unreachable!()
            }
        }
        assert!(check_store(&MemoryStore::default()).is_ok());
        let err = check_store(&Broken).unwrap_err().to_string();
        assert!(err.contains("no Secret Service"), "{err}");
    }

    /// 起動できないコマンドは、エラーの文にして続ける（PR #583のレビュー）
    #[test]
    fn test_run_quietly_reports_failures() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-program");
        let err = run_quietly(&missing, &args(&["mcp", "add"])).unwrap_err();
        assert!(err.contains("could not run"), "{err}");
    }

    /// 入れ直した資格情報で、前の登録を置き換える
    #[test]
    fn test_enter_credentials_replaces_stored_ones() {
        let store = MemoryStore::default();
        let mut io = Script::new(&["editor", "wrong"]);
        setup(&mut io, &store)
            .enter_credentials("https://example.com", "example.com")
            .unwrap();
        let mut io = Script::new(&["editor", "right"]);
        setup(&mut io, &store)
            .enter_credentials("https://example.com", "example.com")
            .unwrap();
        assert_eq!(
            *load_wordpress_credentials(&store, "example.com")
                .unwrap()
                .application_password,
            "right"
        );
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
            "http://example.com",   // httpsでない
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
