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
//! WordPressに続けて、データベース（`[database]`、PostgreSQL・MySQL・
//! MariaDB）も聞く（#590）。データベースのパスワードも、資格情報ストアに
//! 入れる。パスワードは引数で受け取らない。画面には出さない。

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use zeroize::Zeroizing;

use crate::security::credential_tool::read_hidden;
use crate::security::keyring_store::{
    load_wordpress_credentials, site_origin, store_database_credentials,
    store_wordpress_credentials, DatabaseKeyringCredentials, KeyringError, OsKeyring, SecretStore,
    WordPressKeyringCredentials,
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
  3. optionally, a database for the AI to read (PostgreSQL, MySQL or
     MariaDB): adds [database] to the config file, with the password in the
     OS credential store. Writing data stays off unless you allow it
  4. checks the config file like `mcp-rs check`: it connects to the site
     and signs in with read-only requests, and connects to the database
  5. registers mcp-rs with Claude Desktop, Claude Code and GitHub Copilot
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
        previous: discover_registrations(),
        superseded: Vec::new(),
        suggested_url: None,
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
pub(crate) trait Prompt {
    /// 1行の答え（前後の空白を除く）
    fn ask(&mut self, question: &str) -> io::Result<String>;
    /// 画面に出さずに読む答え
    fn ask_hidden(&mut self, question: &str) -> io::Result<Zeroizing<String>>;
    fn say(&mut self, text: &str);
}

/// 端末
pub(crate) struct Terminal;

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
pub(crate) fn confirm(io: &mut dyn Prompt, question: &str, default: bool) -> io::Result<bool> {
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
    /// クライアントに今ある、mcp-rsの登録（#602）
    previous: Vec<Registration>,
    /// 新しい設定ファイルに置き換える、前の設定ファイル（#602）。これを使う
    /// 登録は、重複として既定で消す
    superseded: Vec<String>,
    /// 前の設定ファイルにあった、WordPressのサイトのURL（#602）
    suggested_url: Option<String>,
}

impl Setup<'_> {
    async fn run(mut self) -> Result<bool, Box<dyn std::error::Error>> {
        self.io.say(&format!(
            "mcp-rs {} init: set up mcp-rs for Claude Desktop, Claude Code and GitHub Copilot.\nPress Ctrl+C to stop at any time.\n",
            env!("CARGO_PKG_VERSION")
        ));
        // 0. 前の設定ファイル（手で登録したmcp-rs等）にしか無い設定を、
        // 知らせて選ばせる（#602）
        let mut carry: Option<toml::Table> = None;
        let mut use_previous_as_is = false;
        // 新しい設定ファイルを書いたときだけ、置き換えたものとして扱う前の
        // 設定ファイル（PR #606のレビュー。書かなかったのに前の登録を消すと、
        // 設定が抜け落ちる）と、それに平文のパスワードがあるか
        let mut replacing: Option<(String, bool)> = None;
        if let Some((path, used_by)) = self.previous_config() {
            match self.choose_previous(&path, &used_by)? {
                PreviousChoice::CarryOver(table) => {
                    replacing = Some((path, has_plaintext_secret(&table)));
                    carry = Some(*table);
                }
                PreviousChoice::UseAsIs => {
                    self.config_path = PathBuf::from(&path);
                    use_previous_as_is = true;
                }
                PreviousChoice::LeaveOut(plaintext) => replacing = Some((path, plaintext)),
                PreviousChoice::NotReplacing => {}
            }
        }
        let config_display = self.config_path.display().to_string();

        // 1・2. 資格情報と設定ファイル
        let write_config = if use_previous_as_is {
            false
        } else if self.config_path.exists() {
            self.io
                .say(&format!("A config file already exists: {config_display}"));
            match &replacing {
                // 写すと選んだので、既定は置き換える（PR #606のレビュー）
                Some((previous, _)) if carry.is_some() => confirm(
                    self.io,
                    &format!(
                        "Replace it with one that has the settings copied from {previous}? (a backup copy is kept)"
                    ),
                    true,
                )?,
                _ => confirm(
                    self.io,
                    "Replace it with a new one for WordPress? (a backup copy is kept)",
                    false,
                )?,
            }
        } else {
            true
        };
        let mut plaintext_files: Vec<String> = Vec::new();
        if write_config {
            if let Some((previous, plaintext)) = replacing {
                if plaintext {
                    plaintext_files.push(previous.clone());
                }
                self.superseded.push(previous);
            }
        }
        // この実行で書いた設定の、サイトのURLと資格情報の名前（確認が
        // 失敗したときに、資格情報を入れ直すため）
        let mut written = None;
        if write_config {
            // 資格情報ストアが使えるかを、質問の前に確かめる（PR #583のレビュー）
            check_store(self.store)?;
            let (url, entry) = self.wordpress_credentials()?;
            let content = match &carry {
                Some(old) => carry_over_toml(old, &url, &entry)?,
                None => config_toml(&url, &entry),
            };
            let backup = write_config_file(&self.config_path, &content)?;
            if let Some(backup) = backup {
                self.io.say(&format!(
                    "Kept the previous config file as {}",
                    backup.display()
                ));
                self.offer_to_delete_secret_backup(&backup)?;
            }
            self.io
                .say(&format!("Wrote the config file: {config_display}\n"));
            if carry.is_some() {
                self.note_copied_secrets_and_env(&content);
            }
            written = Some((url, entry));
        } else {
            self.io.say(
                "Keeping the config file as it is. Checking it and registering mcp-rs with it.\n",
            );
        }

        // 2b. データベース（#590）。設定ファイルに`[database]`が無ければ
        // 聞き、その節だけを書き足す（ファイルは置き換えない）
        self.database_step()?;

        // 3. 確かめる。認証が通らなければ、その場で資格情報を入れ直せる
        // （PR #583のレビュー。やり直しで、既定の答えのまま同じ失敗に
        // 戻らないように）
        let log_file;
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
            // データベースだけのエラーでは、登録を止めない。mcp-rsは、
            // データベースのツールが無いまま起動する（#590）
            if report.count_errors_except(&crate::check::DATABASE_ITEMS) == 0 {
                if report.count(crate::check::Status::Error) > 0 {
                    self.io.say(
                        "The database cannot be used yet: until the errors above are fixed, mcp-rs starts without the database tools. Fix [database] in the config file, then check again with mcp-rs check.\n",
                    );
                }
                log_file = report.find("log file").map(|(_, file)| file.to_string());
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
        let exe = stable_exe_path(&std::env::current_exe()?, |path| path.exists());
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
        // ログの場所は、実行ファイルの場所で決まる（#602）
        let moved_from: Vec<&str> = self
            .previous
            .iter()
            .filter(|registration| !same_file(&registration.exe, &launch.exe))
            .map(|registration| registration.exe.as_str())
            .collect();
        if let (Some(log_file), Some(old)) = (&log_file, moved_from.first()) {
            self.io.say(&format!(
                "\nLogs: mcp-rs now writes its log to {log_file}. The mcp-rs you used before ({old}) wrote it in another folder (next to it, or in the .mcp-rs/logs folder of your home folder)."
            ));
        }
        for file in plaintext_files {
            self.io.say(&format!(
                "\nNote: {file} still has a password in plain text. mcp-rs no longer uses that file: delete it, or remove the password from it."
            ));
        }
        Ok(true)
    }

    /// 今の登録が使う設定ファイルのうち、これから書く設定ファイルと違い、
    /// 実際にあるもの（#602）。そのファイルと、それを使う登録の説明を返す
    fn previous_config(&self) -> Option<(String, Vec<String>)> {
        let ours = self.config_path.display().to_string();
        // これから書く設定ファイルを使う登録があれば、移行は済んでいる。
        // ほかの設定ファイルの登録は、意図して並べたもの（2つ目のサイト等）
        // として扱い、今までどおり既定で残す（PR #606のレビュー）
        if self
            .previous
            .iter()
            .any(|registration| same_config_file(registration.config.as_deref(), &ours))
        {
            return None;
        }
        let mut found: Option<(String, Vec<String>)> = None;
        for registration in &self.previous {
            let Some(path) = registration
                .config
                .clone()
                .or_else(crate::config::user_config_path)
            else {
                continue;
            };
            if same_config_file(Some(&path), &ours) || !Path::new(&path).is_file() {
                continue;
            }
            let label = format!("{} \"{}\"", registration.client, registration.name);
            match &mut found {
                Some((first, used_by)) if same_config_file(Some(&path), first) => {
                    used_by.push(label)
                }
                Some(_) => {}
                None => found = Some((path, vec![label])),
            }
        }
        found
    }

    /// 前の設定ファイルをどうするかを聞く（#602）
    fn choose_previous(
        &mut self,
        path: &str,
        used_by: &[String],
    ) -> Result<PreviousChoice, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path)?;
        let table: toml::Table = match text.trim_start_matches('\u{feff}').parse() {
            Ok(table) => table,
            Err(_) => {
                self.io.say(&format!(
                    "mcp-rs is registered with {path} ({}), but it cannot be read as TOML, so nothing is copied from it.\n",
                    used_by.join(", ")
                ));
                return Ok(PreviousChoice::LeaveOut(false));
            }
        };
        self.suggested_url = table
            .get("handlers")
            .and_then(|h| h.get("wordpress"))
            .and_then(|w| w.get("url"))
            .and_then(toml::Value::as_str)
            .filter(|url| !url.contains("${"))
            .map(str::to_string);
        let plaintext = has_plaintext_secret(&table);
        let extras = extra_settings(&table);
        let site = table
            .get("handlers")
            .and_then(|h| h.get("wordpress"))
            .and_then(|w| w.get("url"))
            .and_then(toml::Value::as_str)
            .map(|url| format!("for {url}"))
            .unwrap_or_else(|| "without a WordPress site".to_string());
        self.io.say(&format!(
            "mcp-rs is already registered with another config file: {path} ({site}; {})",
            used_by.join(", ")
        ));
        // 移行する前の設定か、意図して並べた別の設定（2つ目のサイト等）かは、
        // 見分けられないので聞く（PR #606のレビュー）
        if !confirm(
            self.io,
            "Is it the setup that the new config file replaces? (answer n for a setup you keep, such as one for another site)",
            true,
        )? {
            self.suggested_url = None;
            self.io.say("Keeping it as it is.\n");
            return Ok(PreviousChoice::NotReplacing);
        }
        if extras.is_empty() {
            self.io.say(
                "It has nothing that a new config file for WordPress would not have, so the new one replaces it.\n",
            );
            return Ok(PreviousChoice::LeaveOut(plaintext));
        }
        self.io
            .say("It has settings that a new config file for WordPress would not have:");
        for extra in &extras {
            self.io.say(&format!("  - {extra}"));
        }
        self.io.say("  1. Copy these settings into the new config file (the WordPress credentials go to the OS credential store, not the file)");
        self.io.say("  2. Keep using that config file as it is");
        self.io.say("  3. Leave these settings out");
        loop {
            let answer = self.io.ask("Choose 1, 2 or 3 [1]: ")?;
            match answer.as_str() {
                "" | "1" => return Ok(PreviousChoice::CarryOver(Box::new(table))),
                "2" => return Ok(PreviousChoice::UseAsIs),
                "3" => return Ok(PreviousChoice::LeaveOut(plaintext)),
                _ => self.io.say("Answer 1, 2 or 3."),
            }
        }
    }

    /// 写した設定に、平文の秘密や`${VAR}`が残るとき、知らせる（PR #606の
    /// レビュー）。前の登録の`env`は、新しい登録に写らない
    fn note_copied_secrets_and_env(&mut self, content: &str) {
        let Ok(table) = content.parse::<toml::Table>() else {
            return;
        };
        if has_plaintext_secret(&table) {
            self.io.say(
                "Note: the new config file has a password or an API key in plain text, copied from the previous one ([database] or [transport.http]). Move it to an environment variable (${...}) when you can.",
            );
        }
        if !content.contains("${") {
            return;
        }
        let names: Vec<String> = self
            .previous
            .iter()
            .flat_map(|registration| registration.env.iter().cloned())
            .collect();
        if names.is_empty() {
            self.io.say(
                "Note: the new config file reads environment variables (${...}). MCP clients pass them only when they are registered with them, so the check below fails if they are not set for mcp-rs.",
            );
        } else {
            self.io.say(&format!(
                "Note: the new config file reads environment variables (${{...}}). The previous registration passed {}, but the new registration does not pass any. If the check below fails, set them for mcp-rs (for example in the client's settings), or move the values to the OS credential store.",
                names.join(", ")
            ));
        }
    }

    /// 平文のパスワードを含む控えなら、消すかを聞く（#599）
    fn offer_to_delete_secret_backup(&mut self, backup: &Path) -> io::Result<()> {
        let plaintext = std::fs::read_to_string(backup)
            .ok()
            .and_then(|text| text.parse::<toml::Table>().ok())
            .is_some_and(|table| has_plaintext_secret(&table));
        if plaintext
            && confirm(
                self.io,
                "The backup copy has a password in plain text, which the new config file does not need. Delete the backup copy?",
                true,
            )?
        {
            std::fs::remove_file(backup)?;
            self.io.say("Deleted the backup copy.");
        }
        Ok(())
    }

    /// サイトのURLを聞き、資格情報を資格情報ストアに入れる（入っていれば
    /// 使うかを聞く）。設定ファイルに書くURLと、資格情報の名前を返す
    fn wordpress_credentials(&mut self) -> Result<(String, String), Box<dyn std::error::Error>> {
        self.io.say("WordPress site");
        self.io.say("  mcp-rs signs in with an Application Password (WordPress: Users > Profile > Application Passwords).");
        let url = loop {
            let answer = match &self.suggested_url {
                Some(suggested) => {
                    let answer = self.io.ask(&format!("Site URL [{suggested}]: "))?;
                    if answer.is_empty() {
                        suggested.clone()
                    } else {
                        answer
                    }
                }
                None => self
                    .io
                    .ask("Site URL (for example https://example.com): ")?,
            };
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

    /// データベースを使うかを聞き、使うなら`[database]`を設定ファイルに
    /// 書き足す（#590）。既に`[database]`があれば、そのままにする
    fn database_step(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let content = match std::fs::read_to_string(&self.config_path) {
            Ok(content) => content,
            Err(_) => return Ok(()),
        };
        match content.parse::<toml::Table>() {
            Ok(table) if table.contains_key("database") => {
                self.io
                    .say("The config file already has [database]. Keeping it as it is.\n");
                return Ok(());
            }
            Ok(_) => {}
            // 読めない設定ファイルには書き足さない（確認でエラーになる）
            Err(_) => return Ok(()),
        }
        if !confirm(
            self.io,
            "Also let the AI read a database (PostgreSQL, MySQL or MariaDB)?",
            false,
        )? {
            self.io.say("");
            return Ok(());
        }
        check_store(self.store)?;
        let answers = ask_database(self.io, self.store, None)?;
        let backup = append_to_config_file(&self.config_path, &database_toml(&answers)?)?;
        self.io.say(&format!(
            "Kept the previous config file as {}",
            backup.display()
        ));
        self.offer_to_delete_secret_backup(&backup)?;
        self.io.say(&format!(
            "Added [database] to the config file: {}\n",
            self.config_path.display()
        ));
        Ok(())
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
            let replaced = self
                .superseded
                .iter()
                .find(|path| same_config_file(other.config.as_deref(), path));
            let same_config =
                same_config_file(other.config.as_deref(), &launch.config) || replaced.is_some();
            let question = if let Some(replaced) = replaced {
                format!(
                    "{client} also starts mcp-rs as \"{name}\" with the previous config file ({replaced}), which the new one replaces. Remove it, so that only one mcp-rs runs?"
                )
            } else if same_config {
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
pub(crate) fn check_store(store: &dyn SecretStore) -> Result<(), Box<dyn std::error::Error>> {
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

/// `[database]`の値を聞く。パスワードは資格情報ストアに入れる（#590）。
/// `entry`があれば、その名前で入れる（`database-keyring set <名前>`、#628。
/// そのときは、パスワードを空にできない）
pub(crate) fn ask_database(
    io: &mut dyn Prompt,
    store: &dyn SecretStore,
    entry: Option<&str>,
) -> Result<DatabaseAnswers, Box<dyn std::error::Error>> {
    io.say("Database");
    let engine = loop {
        let answer = io
            .ask("Engine: postgresql, mysql or mariadb [postgresql]: ")?
            .to_ascii_lowercase();
        match answer.as_str() {
            "" => break "postgresql".to_string(),
            "postgresql" | "mysql" | "mariadb" => break answer,
            _ => io.say("Enter postgresql, mysql or mariadb."),
        }
    };
    let host = ask_with_default(io, "Host", "localhost")?;
    let default_port = if engine == "postgresql" { 5432 } else { 3306 };
    let port = loop {
        let answer = io.ask(&format!("Port [{default_port}]: "))?;
        if answer.is_empty() {
            break default_port;
        }
        match answer.parse::<u16>() {
            Ok(port) if port > 0 => break port,
            _ => io.say("Enter a port number (1-65535)."),
        }
    };
    let database = ask_required(io, "Database name: ", "The database name is empty.")?;
    io.say("  Use a database user that can only read what the AI needs. mcp-rs refuses SQL that writes, but the user's permissions are the real limit.");
    let username = ask_required(io, "User name: ", "The user name is empty.")?;
    let password = match entry {
        Some(_) => ask_database_password(io)?,
        None => {
            io.ask_hidden("Password (not shown while typing; leave empty if there is none): ")?
        }
    };
    let keyring = if password.is_empty() {
        None
    } else {
        let entry = match entry {
            Some(entry) => entry.to_string(),
            None => {
                let suggested = format!("{database}@{host}");
                let answer = io.ask(&format!(
                    "Name for this password in the OS credential store [{suggested}]: "
                ))?;
                if answer.is_empty() {
                    suggested
                } else {
                    answer
                }
            }
        };
        store_database_credentials(
            store,
            &entry,
            &DatabaseKeyringCredentials {
                host: host.clone(),
                port,
                username: username.clone(),
                password,
            },
        )?;
        io.say(&format!(
            "Stored the password as '{entry}' in the OS credential store."
        ));
        Some(entry)
    };

    let loopback = is_loopback_host(&host);
    let ssl_default = if loopback { "prefer" } else { "verify-full" };
    if !loopback {
        io.say("  The database is on another machine: verify-full encrypts the connection and checks the server's certificate and host name.");
    }
    let ssl_mode = loop {
        let answer = ask_with_default(io, "TLS (ssl_mode)", ssl_default)?;
        let check = crate::config::DatabaseSettings {
            engine: engine.clone(),
            database: database.clone(),
            username: username.clone(),
            ssl_mode: Some(answer.clone()),
            // `verify-ca`の確認（ssl_root_certが要る）は、次の質問の後で行う
            ssl_root_cert: Some("-".to_string()),
            ..Default::default()
        };
        match check.validate() {
            Ok(()) => break answer,
            Err(e) => io.say(&e),
        }
    };
    let ssl_root_cert = if ssl_mode.starts_with("verify") {
        let answer = io.ask(
            "CA certificate file (PEM) for a private CA, such as Amazon RDS (leave empty for a public CA): ",
        )?;
        (!answer.is_empty()).then_some(answer)
    } else {
        None
    };

    let allow_write = confirm(
        io,
        "Also let the AI change data (INSERT, UPDATE, DELETE)?",
        false,
    )? && confirm(
        io,
        &format!("The AI will be able to change and delete data in '{database}'. Allow it?"),
        false,
    )?;

    let answers = DatabaseAnswers {
        engine,
        host,
        port,
        database,
        username,
        keyring,
        ssl_mode,
        ssl_root_cert,
        allow_write,
    };
    answers.settings().validate()?;
    Ok(answers)
}

/// `[既定]`付きで聞く。空の答えは`default`
pub(crate) fn ask_with_default(
    io: &mut dyn Prompt,
    question: &str,
    default: &str,
) -> io::Result<String> {
    let answer = io.ask(&format!("{question} [{default}]: "))?;
    Ok(if answer.is_empty() {
        default.to_string()
    } else {
        answer
    })
}

/// 空でない答えを聞く
pub(crate) fn ask_required(io: &mut dyn Prompt, question: &str, empty: &str) -> io::Result<String> {
    loop {
        let answer = io.ask(question)?;
        if !answer.is_empty() {
            return Ok(answer);
        }
        io.say(empty);
    }
}

/// 空でないパスワードを、画面に出さずに聞く（#628）
pub(crate) fn ask_database_password(io: &mut dyn Prompt) -> io::Result<Zeroizing<String>> {
    loop {
        let password = io.ask_hidden("Password (not shown while typing): ")?;
        if !password.is_empty() {
            return Ok(password);
        }
        io.say("The password is empty.");
    }
}

/// クライアントに登録する、mcp-rsの実行ファイルのパス（#578）。Homebrewで
/// 入れたmcp-rsは、版ごとのフォルダ（`<prefix>/Cellar/mcp-rs/<版>/bin/mcp-rs`）
/// にあり、更新すると古い版のフォルダは消える。そのパスを登録すると、
/// 更新の後にクライアントが起動できなくなるので、版によらない
/// `<prefix>/opt/mcp-rs/bin/mcp-rs`があれば、そちらにする。`exists`は、
/// パスがあるかを確かめる関数（テストで差し替える）
fn stable_exe_path(exe: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let components: Vec<_> = exe.components().collect();
    if let Some(cellar) = components
        .iter()
        .position(|component| component.as_os_str() == "Cellar")
    {
        // Cellar/<formula>/<版>/<残り>
        if components.len() > cellar + 3 {
            let prefix: PathBuf = components[..cellar].iter().collect();
            let rest: PathBuf = components[cellar + 3..].iter().collect();
            let opt = prefix
                .join("opt")
                .join(components[cellar + 1].as_os_str())
                .join(rest);
            if exists(&opt) {
                return opt;
            }
        }
    }
    exe.to_path_buf()
}

/// `mcp-rs init`で聞いた、`[database]`の値（#590）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatabaseAnswers {
    engine: String,
    host: String,
    port: u16,
    database: String,
    username: String,
    /// パスワードを入れた、資格情報ストアの項目の名前。パスワードが無ければ`None`
    keyring: Option<String>,
    ssl_mode: String,
    ssl_root_cert: Option<String>,
    allow_write: bool,
}

impl DatabaseAnswers {
    /// 確かめるための`DatabaseSettings`
    fn settings(&self) -> crate::config::DatabaseSettings {
        crate::config::DatabaseSettings {
            engine: self.engine.clone(),
            host: Some(self.host.clone()),
            port: Some(self.port),
            database: self.database.clone(),
            username: self.username.clone(),
            keyring: self.keyring.clone(),
            ssl_mode: Some(self.ssl_mode.clone()),
            ssl_root_cert: self.ssl_root_cert.clone(),
            allow_write: self.allow_write.then_some(true),
            ..Default::default()
        }
    }
}

/// 設定ファイルに書き足す`[database]`の節。値はTOMLとして書く（引用符等を
/// エスケープする）。パスワードは書かず、`keyring`で資格情報ストアを指す
pub(crate) fn database_toml(answers: &DatabaseAnswers) -> Result<String, toml::ser::Error> {
    let mut root = toml::Table::new();
    root.insert(
        "database".to_string(),
        toml::Value::try_from(answers.settings())?,
    );
    Ok(format!(
        "# Database (added by mcp-rs init). The password is in the OS credential store (keyring)\n{}",
        toml::to_string(&root)?
    ))
}

/// ループバック（同じマシン）のホストか。TLSの既定を決める
fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    host.eq_ignore_ascii_case("localhost")
        || host.starts_with('/')
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// 設定ファイルの最後に`section`を書き足す（#590）。前のファイルを控えに
/// 残し、そのパスを返す。置き換えは`write_config_file`と同じく、一時
/// ファイルを経て行い、Unixでは本人だけが読めるようにする
pub(crate) fn append_to_config_file(path: &Path, section: &str) -> io::Result<PathBuf> {
    let mut content = std::fs::read_to_string(path)?;
    let backup = backup_path(path);
    std::fs::copy(path, &backup)?;
    make_private(&backup)?;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str(section);
    write_atomically(path, &content, true)?;
    Ok(backup)
}

/// `path`に`content`を書く。フォルダが無ければ作る。前のファイルがあれば、
/// 別の名前で残し、そのパスを返す。Unixでは、フォルダを本人だけ（700）、
/// ファイルと控えを本人だけが読める（600）ようにする（#599）。後で
/// `[database]`のパスワード等を書き足すことがあるため
fn write_config_file(path: &Path, content: &str) -> io::Result<Option<PathBuf>> {
    if let Some(parent) = path.parent() {
        create_private_dir_all(parent)?;
    }
    let backup = if path.exists() {
        let backup = backup_path(path);
        std::fs::copy(path, &backup)?;
        make_private(&backup)?;
        Some(backup)
    } else {
        None
    };
    write_atomically(path, content, true)?;
    Ok(backup)
}

/// フォルダを作る。Unixでは、新しく作るフォルダを本人だけ（700）にする
fn create_private_dir_all(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Unixでは、ファイルを本人だけが読み書きできる（600）ようにする
pub(crate) fn make_private(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// 同じフォルダの別の名前に書いてから、名前を変えて置き換える（#599）。
/// 途中で止まっても、元のファイルが壊れない。`private`なら、Unixでは
/// 本人だけが読めるようにする
pub(crate) fn write_atomically(path: &Path, content: &str, private: bool) -> io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.mcp-rs-init-{}", std::process::id()));
    let result = (|| {
        std::fs::write(&temporary, content)?;
        if private {
            make_private(&temporary)?;
        }
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// 2つのファイルのパスが同じか（区切り文字をそろえ、Windowsでは大文字・
/// 小文字を区別しない）
fn same_file(a: &str, b: &str) -> bool {
    let normalize = |path: &str| {
        let path = path.trim().replace('\\', "/");
        if cfg!(windows) {
            path.to_lowercase()
        } else {
            path
        }
    };
    normalize(a) == normalize(b)
}

/// クライアントにある、mcp-rsの登録（#602）
#[derive(Debug, Clone, PartialEq, Eq)]
struct Registration {
    client: &'static str,
    name: String,
    exe: String,
    /// `--config`の値（無ければ`None`）
    config: Option<String>,
    /// 登録で渡している環境変数の名前（値は持たない）
    env: Vec<String>,
}

/// 前の設定ファイルをどうするか（#602）
enum PreviousChoice {
    /// 新しい設定ファイルに写す
    CarryOver(Box<toml::Table>),
    /// そのまま使う
    UseAsIs,
    /// 写さない（平文のパスワードがあったか）
    LeaveOut(bool),
    /// 置き換える前の設定ではない（2つ目のサイト等）。何もしない
    NotReplacing,
}

/// `mcpServers`のうち、mcp-rsを起動する登録
fn mcp_rs_registrations(client: &'static str, config: &serde_json::Value) -> Vec<Registration> {
    let Some(servers) = config
        .get("mcpServers")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    servers
        .iter()
        .filter_map(|(name, server)| {
            let exe = server.get("command").and_then(serde_json::Value::as_str)?;
            if !is_mcp_rs_command(exe) {
                return None;
            }
            let args: Vec<&str> = server
                .get("args")
                .and_then(serde_json::Value::as_array)
                .map(|args| args.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            let env = server
                .get("env")
                .and_then(serde_json::Value::as_object)
                .map(|env| env.keys().cloned().collect())
                .unwrap_or_default();
            Some(Registration {
                client,
                name: name.clone(),
                exe: exe.to_string(),
                config: config_arg(&args),
                env,
            })
        })
        .collect()
}

/// Claude Desktop・Claude Code・Copilotにある、mcp-rsの登録（#602）。
/// 読めない設定ファイルは飛ばす。内容は表示しない
fn discover_registrations() -> Vec<Registration> {
    let mut found = Vec::new();
    for path in select_claude_desktop_configs(&claude_desktop_config_candidates()) {
        if let Ok(config) = read_json_file(&path) {
            found.extend(mcp_rs_registrations("Claude Desktop", &config));
        }
    }
    for (client, path) in [
        ("Claude Code", claude_code_config_file()),
        ("GitHub Copilot", copilot_config_file()),
    ] {
        if let Some(config) = path.and_then(|path| read_json_file(&path).ok()) {
            found.extend(mcp_rs_registrations(client, &config));
        }
    }
    found
}

/// `[handlers.wordpress]`の資格情報の項目（新しい設定には写さない）
const WORDPRESS_CREDENTIAL_KEYS: [&str; 4] =
    ["username", "password", "encrypted_credentials", "keyring"];

/// 前の設定にあって、`init`が書く設定（`config_toml`）に無いもの（#602）
fn extra_settings(old: &toml::Table) -> Vec<String> {
    let mut extras = Vec::new();
    for (key, value) in old {
        match key.as_str() {
            "transport" => {
                if let Some(table) = value.as_table() {
                    let kind = table
                        .get("type")
                        .or_else(|| table.get("transport_type"))
                        .and_then(toml::Value::as_str);
                    if kind.is_some_and(|kind| kind != "stdio") {
                        extras.push(format!(
                            "[transport] type = \"{}\" (the new config file uses stdio, which MCP clients need)",
                            kind.unwrap_or_default()
                        ));
                    }
                    for (sub, _) in table
                        .iter()
                        .filter(|(sub, v)| v.is_table() && *sub != "stdio")
                    {
                        extras.push(format!("[transport.{sub}]"));
                    }
                }
            }
            "server" => {
                if let Some(table) = value.as_table() {
                    for (sub, value) in table {
                        if sub == "log_level" && value.as_str() == Some("warn") {
                            continue;
                        }
                        extras.push(format!("[server] {sub}"));
                    }
                }
            }
            "handlers" => {
                if let Some(table) = value.as_table() {
                    for (handler, value) in table {
                        if handler != "wordpress" {
                            extras.push(format!("[handlers.{handler}]"));
                            continue;
                        }
                        let Some(wordpress) = value.as_table() else {
                            continue;
                        };
                        for (sub, _) in wordpress {
                            if !["enabled", "url"].contains(&sub.as_str())
                                && !WORDPRESS_CREDENTIAL_KEYS.contains(&sub.as_str())
                            {
                                extras.push(format!("[handlers.wordpress] {sub}"));
                            }
                        }
                    }
                }
            }
            "plugins" => {
                let names: Vec<&str> = value
                    .as_array()
                    .map(|plugins| {
                        plugins
                            .iter()
                            .filter_map(|p| p.get("name").and_then(toml::Value::as_str))
                            .collect()
                    })
                    .unwrap_or_default();
                extras.push(format!("[[plugins]] {}", names.join(", ")));
            }
            other => extras.push(format!("[{other}]")),
        }
    }
    extras
}

/// 前の設定を写した、新しい設定ファイル（#602）。`[handlers.wordpress]`は、
/// 資格情報を外して`url`・`keyring`にし、`[transport]`はSTDIOにする
/// （MCPのクライアントはSTDIOで起動する）。前のファイルのコメントは写らない
fn carry_over_toml(old: &toml::Table, url: &str, entry: &str) -> Result<String, toml::ser::Error> {
    let mut table = old.clone();
    let transport = table
        .entry("transport")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let Some(transport) = transport.as_table_mut() {
        transport.remove("transport_type");
        transport.insert("type".to_string(), toml::Value::String("stdio".to_string()));
    }
    let handlers = table
        .entry("handlers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let Some(handlers) = handlers.as_table_mut() {
        let wordpress = handlers
            .entry("wordpress")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if let Some(wordpress) = wordpress.as_table_mut() {
            for key in WORDPRESS_CREDENTIAL_KEYS {
                wordpress.remove(key);
            }
            wordpress.insert("enabled".to_string(), toml::Value::Boolean(true));
            wordpress.insert("url".to_string(), toml::Value::String(url.to_string()));
            wordpress.insert(
                "keyring".to_string(),
                toml::Value::String(entry.to_string()),
            );
        }
    }
    let body = toml::to_string(&table)?;
    Ok(format!(
        "# mcp-rs config file, written by `mcp-rs init` (mcp-rs {}) from a previous\n# config file. The WordPress username and Application Password are kept in\n# the OS credential store (mcp-rs wordpress-keyring show {entry}).\n# Check it with: mcp-rs check --config <this file>\n\n{body}",
        env!("CARGO_PKG_VERSION")
    ))
}

/// 平文のパスワード・APIキーを含む設定か（`${VAR}`は平文ではない、#599）
fn has_plaintext_secret(table: &toml::Table) -> bool {
    let plain = |value: Option<&toml::Value>| {
        value
            .and_then(toml::Value::as_str)
            .is_some_and(|s| !s.trim().is_empty() && !s.contains("${"))
    };
    let get = |path: &[&str]| -> Option<&toml::Value> {
        let mut value = table.get(path[0])?;
        for key in &path[1..] {
            value = value.get(key)?;
        }
        Some(value)
    };
    let api_key = get(&["transport", "http", "api_keys"])
        .and_then(toml::Value::as_array)
        .is_some_and(|keys| keys.iter().any(|k| plain(k.get("key"))));
    plain(get(&["handlers", "wordpress", "password"]))
        || plain(get(&["database", "password"]))
        || api_key
}

/// `path`の控えのパス（`<name>.bak-<日時>`）
pub(crate) fn backup_path(path: &Path) -> PathBuf {
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
    // Claude Desktopが同時に書いても、途中で止まっても、壊れたファイルを
    // 残さない（#599）
    write_atomically(path, &content, false)?;
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

pub(crate) fn find_program_in(
    name: &str,
    dirs: impl Iterator<Item = PathBuf>,
    extensions: &[&str],
) -> Option<PathBuf> {
    // `PATH`の相対パス（空の項目は今のフォルダ）は見ない（#599）。開いた
    // フォルダの`claude`等を動かさないため
    for dir in dirs.filter(|dir| dir.is_absolute()) {
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
            previous: Vec::new(),
            superseded: Vec::new(),
            suggested_url: None,
        }
    }

    /// 手で登録していた設定（#602の実機の例）
    const PREVIOUS_CONFIG: &str = r#"
[server]
bind_addr = "127.0.0.1:8080"
log_level = "info"

[transport]
type = "stdio"

[transport.http]
port = 8080

[handlers.wordpress]
url = "https://example.com"
username = "editor"
password = "plain secret"
enabled = true
timeout_seconds = 30
upload_allowed_dirs = ["C:/Users/me/Pictures/mcp-upload"]

[handlers.wordpress.rate_limit]
requests_per_second = 10
burst_size = 20
enabled = true

[[plugins]]
name = "fs"
command = "node"
args = ["server.js", "C:/data"]

[audit]
file_path = "C:/Users/me/.mcp-rs/logs/audit.log"
"#;

    #[test]
    fn test_extra_settings_lists_what_a_new_file_would_lose() {
        let old: toml::Table = PREVIOUS_CONFIG.parse().unwrap();
        let extras = extra_settings(&old);
        for expected in [
            "[server] bind_addr",
            "[server] log_level",
            "[transport.http]",
            "[handlers.wordpress] timeout_seconds",
            "[handlers.wordpress] upload_allowed_dirs",
            "[handlers.wordpress] rate_limit",
            "[[plugins]] fs",
            "[audit]",
        ] {
            assert!(
                extras.iter().any(|e| e == expected),
                "{expected}: {extras:?}"
            );
        }
        // 資格情報・url・enabledは、新しい設定にもある（資格情報は資格情報ストアへ）
        assert!(!extras
            .iter()
            .any(|e| e.contains("password") || e.contains("username")));
        assert!(!extras
            .iter()
            .any(|e| e.ends_with(" url") || e.ends_with(" enabled")));
        // 新しい設定と同じものだけなら、何も無い
        let fresh: toml::Table = config_toml("https://example.com", "example.com")
            .parse()
            .unwrap();
        assert!(
            extra_settings(&fresh).is_empty(),
            "{:?}",
            extra_settings(&fresh)
        );
    }

    /// 写した設定は、起動と同じ形で読め、資格情報を含まない（#602）
    #[test]
    fn test_carry_over_toml_keeps_settings_without_credentials() {
        let old: toml::Table = PREVIOUS_CONFIG.parse().unwrap();
        let text = carry_over_toml(&old, "https://example.com", "example.com").unwrap();
        assert!(!text.contains("plain secret"), "{text}");
        assert!(!text.contains("editor"), "{text}");
        let config: crate::config::McpConfig = toml::from_str(&text).unwrap();
        let wordpress = config.handlers.wordpress.unwrap();
        assert_eq!(wordpress.url, "https://example.com");
        assert_eq!(wordpress.keyring.as_deref(), Some("example.com"));
        assert_eq!(wordpress.enabled, Some(true));
        assert_eq!(config.plugins.len(), 1);
        assert_eq!(config.plugins[0].name, "fs");
        assert_eq!(config.transport.transport_type.as_deref(), Some("stdio"));
        let table: toml::Table = text.parse().unwrap();
        assert!(extra_settings(&table)
            .iter()
            .any(|e| e == "[handlers.wordpress] upload_allowed_dirs"));
        assert!(!has_plaintext_secret(&table));
    }

    #[test]
    fn test_has_plaintext_secret() {
        let secret = |text: &str| has_plaintext_secret(&text.parse::<toml::Table>().unwrap());
        assert!(secret(PREVIOUS_CONFIG));
        assert!(!secret(
            "[handlers.wordpress]\npassword = \"${WORDPRESS_PASSWORD}\""
        ));
        assert!(!secret("[handlers.wordpress]\nkeyring = \"example.com\""));
        assert!(secret("[database]\npassword = \"db secret\""));
        assert!(secret(
            "[transport.http]\n[[transport.http.api_keys]]\nkey = \"k\""
        ));
        assert!(!secret(
            "[transport.http]\n[[transport.http.api_keys]]\nkey = \"${KEY}\""
        ));
    }

    #[test]
    fn test_mcp_rs_registrations() {
        let config = serde_json::json!({ "mcpServers": {
            "mcp-rs-wordpress": {
                "command": "C:/Program Files/MCP-rs/mcp-rs.exe",
                "args": ["--config", "C:/old/mcp-config.toml"],
                "env": { "WORDPRESS_PASSWORD": "secret" }
            },
            "mcp-rs": { "command": "C:/Users/me/.mcp-rs/bin/mcp-rs.exe" },
            "other": { "command": "npx" }
        }});
        assert_eq!(
            mcp_rs_registrations("Claude Desktop", &config),
            vec![
                Registration {
                    client: "Claude Desktop",
                    name: "mcp-rs-wordpress".to_string(),
                    exe: "C:/Program Files/MCP-rs/mcp-rs.exe".to_string(),
                    config: Some("C:/old/mcp-config.toml".to_string()),
                    // 名前だけ（値は持たない）
                    env: vec!["WORDPRESS_PASSWORD".to_string()],
                },
                Registration {
                    client: "Claude Desktop",
                    name: "mcp-rs".to_string(),
                    exe: "C:/Users/me/.mcp-rs/bin/mcp-rs.exe".to_string(),
                    config: None,
                    env: Vec::new(),
                },
            ]
        );
    }

    /// 前の設定ファイルを見つけ、写すかを聞く。写すと、新しい設定に入り、
    /// その設定ファイルを使う登録は、重複として既定で消す（#602）
    #[test]
    fn test_previous_config_is_carried_over() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("mcp-config.toml");
        std::fs::write(&old_path, PREVIOUS_CONFIG).unwrap();
        let old = old_path.display().to_string();
        let store = MemoryStore::default();
        let mut io = Script::new(&["", "", ""]);
        let mut setup = setup(&mut io, &store);
        setup.config_path = dir.path().join("new").join("config.toml");
        setup.previous = vec![Registration {
            client: "Claude Desktop",
            name: "mcp-rs-wordpress".to_string(),
            exe: "C:/Program Files/MCP-rs/mcp-rs.exe".to_string(),
            config: Some(old.clone()),
            env: Vec::new(),
        }];
        let (path, used_by) = setup.previous_config().unwrap();
        assert_eq!(path, old);
        assert_eq!(used_by, ["Claude Desktop \"mcp-rs-wordpress\""]);
        // Enterだけで、写す（1）
        let choice = setup.choose_previous(&path, &used_by).unwrap();
        assert!(matches!(choice, PreviousChoice::CarryOver(_)));
        assert_eq!(setup.suggested_url.as_deref(), Some("https://example.com"));
        setup.superseded.push(path);
        // 前の設定ファイルを使う登録は、既定で消す
        let launch = Launch {
            exe: "mcp-rs".to_string(),
            config: setup.config_path.display().to_string(),
        };
        let others = vec![OtherServer {
            name: "mcp-rs-wordpress".to_string(),
            config: Some(old),
        }];
        let remove = setup
            .choose_duplicates_to_remove("Claude Desktop", &others, &launch)
            .unwrap();
        assert_eq!(remove, ["mcp-rs-wordpress"]);
        assert!(io.shown.iter().any(|s| s.contains("[[plugins]] fs")));
    }

    /// 移行が済んでいる（これから書く設定ファイルを使う登録がある）ときは、
    /// 別の設定ファイルの登録を、移行する前の設定として扱わない（PR #606の
    /// レビュー。2つ目のサイトの登録が消えた）
    #[test]
    fn test_previous_config_is_ignored_after_migrating() {
        let dir = tempfile::tempdir().unwrap();
        let second = dir.path().join("second-blog.toml");
        std::fs::write(&second, PREVIOUS_CONFIG).unwrap();
        let store = MemoryStore::default();
        let mut io = Script::new(&[]);
        let mut setup = setup(&mut io, &store);
        setup.config_path = dir.path().join("config.toml");
        let registration = |name: &str, config: &Path| Registration {
            client: "Claude Desktop",
            name: name.to_string(),
            exe: "mcp-rs".to_string(),
            config: Some(config.display().to_string()),
            env: Vec::new(),
        };
        setup.previous = vec![
            registration("mcp-rs", &setup.config_path.clone()),
            registration("second-blog", &second),
        ];
        assert_eq!(setup.previous_config(), None);
        // 移行が済んでいなければ、見つける
        setup.previous.remove(0);
        assert!(setup.previous_config().is_some());
    }

    /// 置き換える前の設定かを先に聞き、nなら何もしない（PR #606のレビュー）
    #[test]
    fn test_previous_config_can_be_kept_as_another_setup() {
        let dir = tempfile::tempdir().unwrap();
        let second = dir.path().join("second-blog.toml");
        std::fs::write(&second, PREVIOUS_CONFIG).unwrap();
        let store = MemoryStore::default();
        let mut io = Script::new(&["n"]);
        let mut setup = setup(&mut io, &store);
        let choice = setup
            .choose_previous(&second.display().to_string(), &[])
            .unwrap();
        assert!(matches!(choice, PreviousChoice::NotReplacing));
        // 2つ目のサイトのURLを、新しい設定の既定にしない
        assert_eq!(setup.suggested_url, None);
        assert!(setup.superseded.is_empty());
    }

    /// 2を選ぶと、前の設定ファイルをそのまま使う
    #[test]
    fn test_previous_config_can_be_used_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("mcp-config.toml");
        std::fs::write(&old_path, PREVIOUS_CONFIG).unwrap();
        let store = MemoryStore::default();
        let mut io = Script::new(&["", "4", "2"]);
        let mut setup = setup(&mut io, &store);
        let choice = setup
            .choose_previous(&old_path.display().to_string(), &[])
            .unwrap();
        assert!(matches!(choice, PreviousChoice::UseAsIs));
        assert!(io.shown.iter().any(|s| s == "Answer 1, 2 or 3."));
    }

    /// 平文のパスワードを含む控えは、消すかを聞く（#599）
    #[test]
    fn test_secret_backup_can_be_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, PREVIOUS_CONFIG).unwrap();
        let backup = write_config_file(&path, "[server]\n").unwrap().unwrap();
        let store = MemoryStore::default();
        let mut io = Script::new(&[""]);
        setup(&mut io, &store)
            .offer_to_delete_secret_backup(&backup)
            .unwrap();
        assert!(!backup.exists());
        // 平文のパスワードが無い控えは、聞かずに残す
        let backup = write_config_file(&path, "[server]\nlog_level = \"warn\"\n")
            .unwrap()
            .unwrap();
        let mut io = Script::new(&[]);
        setup(&mut io, &store)
            .offer_to_delete_secret_backup(&backup)
            .unwrap();
        assert!(backup.exists());
    }

    /// Unixでは、書いた設定ファイル・控え・フォルダを本人だけにする（#599）
    #[cfg(unix)]
    #[test]
    fn test_config_file_is_private_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new").join("config.toml");
        write_config_file(&path, "a").unwrap();
        let backup = write_config_file(&path, "b").unwrap().unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&backup), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
    }

    /// 書き換えは、一時ファイルを残さない（#599）
    #[test]
    fn test_write_atomically_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        std::fs::write(&path, "old").unwrap();
        write_atomically(&path, "new", false).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["claude_desktop_config.json"]);
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
        // 相対パスの項目（今のフォルダ等）は見ない（#599）
        let relative = || vec![PathBuf::from("."), PathBuf::from("")].into_iter();
        assert_eq!(find_program_in("tool", relative(), &["exe", "cmd"]), None);
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

    // ---- #590: [database] ----

    const WORDPRESS_ONLY_CONFIG: &str = "[server]\n\n[transport]\ntype = \"stdio\"\n\n[handlers.wordpress]\nurl = \"https://example.com\"\nenabled = true\nkeyring = \"example.com\"\n";

    fn config_in_temp(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    fn database_setup<'a>(io: &'a mut Script, store: &'a MemoryStore, path: &Path) -> Setup<'a> {
        Setup {
            config_path: path.to_path_buf(),
            ..setup(io, store)
        }
    }

    #[test]
    fn test_database_step_adds_the_section_and_stores_the_password() {
        let (_dir, path) = config_in_temp(WORDPRESS_ONLY_CONFIG);
        let store = MemoryStore::default();
        let mut io = Script::new(&[
            "y",              // データベースも使う
            "",               // postgresql
            "db.example.com", // ホスト
            "",               // 5432
            "app",            // データベース名
            "reader",         // ユーザー名
            "s3cret",         // パスワード
            "",               // 項目の名前: app@db.example.com
            "",               // ssl_mode: verify-full（ループバックではない）
            "",               // CAのファイル無し
            "",               // 書き込みは許さない
        ]);
        database_setup(&mut io, &store, &path)
            .database_step()
            .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // 前の設定は、そのまま残る
        assert!(content.starts_with(WORDPRESS_ONLY_CONFIG), "{content}");
        assert!(!content.contains("s3cret"), "{content}");
        let config: crate::config::McpConfig = toml::from_str(&content).unwrap();
        let db = config.database.unwrap();
        assert_eq!(db.engine, "postgresql");
        assert_eq!(db.host.as_deref(), Some("db.example.com"));
        assert_eq!(db.port, Some(5432));
        assert_eq!(db.keyring.as_deref(), Some("app@db.example.com"));
        assert_eq!(db.ssl_mode.as_deref(), Some("verify-full"));
        assert!(db.password.is_none());
        assert_eq!(db.allow_write, None);

        // 書いた設定から、資格情報ストアのパスワードが読める
        let mut resolved = db.clone();
        crate::config::McpConfig::resolve_database_credentials_with(&mut resolved, &store).unwrap();
        assert_eq!(resolved.password.as_deref(), Some("s3cret"));

        // 控えが残る
        let backups = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("config.toml.bak-")
            })
            .count();
        assert_eq!(backups, 1);
    }

    #[test]
    fn test_database_step_on_loopback_mysql_with_writing() {
        let (_dir, path) = config_in_temp(WORDPRESS_ONLY_CONFIG);
        let store = MemoryStore::default();
        let mut io = Script::new(&[
            "y", "mysql", "", "", "shop", "app", "", // パスワード無し
            "", // ssl_mode: prefer（ループバック）
            "y", "y", // 書き込みを許す（2回確かめる）
        ]);
        database_setup(&mut io, &store, &path)
            .database_step()
            .unwrap();
        let config: crate::config::McpConfig =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let db = config.database.unwrap();
        assert_eq!(db.engine, "mysql");
        assert_eq!(db.host.as_deref(), Some("localhost"));
        assert_eq!(db.port, Some(3306));
        assert_eq!(db.keyring, None);
        assert_eq!(db.ssl_mode.as_deref(), Some("prefer"));
        assert_eq!(db.allow_write, Some(true));
        assert!(io
            .shown
            .iter()
            .any(|s| s.contains("change and delete data in 'shop'")));
    }

    #[test]
    fn test_database_step_keeps_an_existing_section_and_can_be_declined() {
        let with_db = format!(
            "{WORDPRESS_ONLY_CONFIG}\n[database]\nengine = \"postgresql\"\ndatabase = \"x\"\nusername = \"y\"\n"
        );
        let (_dir, path) = config_in_temp(&with_db);
        let store = MemoryStore::default();
        let mut io = Script::new(&[]);
        database_setup(&mut io, &store, &path)
            .database_step()
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), with_db);
        assert!(io
            .shown
            .iter()
            .any(|s| s.contains("already has [database]")));

        let (_dir, path) = config_in_temp(WORDPRESS_ONLY_CONFIG);
        let mut io = Script::new(&["n"]);
        database_setup(&mut io, &store, &path)
            .database_step()
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            WORDPRESS_ONLY_CONFIG
        );
    }

    #[test]
    fn test_database_step_asks_again_for_invalid_answers() {
        let (_dir, path) = config_in_temp(WORDPRESS_ONLY_CONFIG);
        let store = MemoryStore::default();
        let mut io = Script::new(&[
            "y",
            "oracle",
            "postgresql",
            "10.0.0.5",
            "99999",
            "5433",
            "",
            "app",
            "",
            "reader",
            "",
            "verify-ca", // PostgreSQLでは使えない
            "",          // verify-full
            "C:/certs/ca.pem",
            "",
        ]);
        database_setup(&mut io, &store, &path)
            .database_step()
            .unwrap();
        let config: crate::config::McpConfig =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let db = config.database.unwrap();
        assert_eq!(db.port, Some(5433));
        assert_eq!(db.database, "app");
        assert_eq!(db.ssl_mode.as_deref(), Some("verify-full"));
        assert_eq!(db.ssl_root_cert.as_deref(), Some("C:/certs/ca.pem"));
        assert!(io
            .shown
            .iter()
            .any(|s| s == "Enter postgresql, mysql or mariadb."));
        assert!(io
            .shown
            .iter()
            .any(|s| s.contains("verify-ca would accept")));
    }

    #[test]
    fn test_database_toml_escapes_values() {
        let answers = DatabaseAnswers {
            engine: "postgresql".to_string(),
            host: "db.example.com".to_string(),
            port: 5432,
            database: "a\"b".to_string(),
            username: "reader".to_string(),
            keyring: Some("a\"b@db".to_string()),
            ssl_mode: "verify-full".to_string(),
            ssl_root_cert: None,
            allow_write: false,
        };
        let text = database_toml(&answers).unwrap();
        assert!(text.starts_with("# Database (added by mcp-rs init)"));
        let parsed: toml::Table = text.parse().unwrap();
        assert_eq!(parsed["database"]["database"].as_str(), Some("a\"b"));
        assert!(parsed["database"].get("allow_write").is_none());
        assert!(parsed["database"].get("password").is_none());
    }

    #[test]
    fn test_is_loopback_host() {
        for host in [
            "localhost",
            "LOCALHOST",
            "127.0.0.1",
            "::1",
            "[::1]",
            "/var/run/postgresql",
        ] {
            assert!(is_loopback_host(host), "{host}");
        }
        for host in ["db.example.com", "10.0.0.5", "localhost.example.com"] {
            assert!(!is_loopback_host(host), "{host}");
        }
    }

    /// Homebrewの版ごとのフォルダではなく、版によらないパスを登録する（#578）
    #[test]
    fn test_stable_exe_path_for_homebrew() {
        let cellar = Path::new("/opt/homebrew/Cellar/mcp-rs/0.29.0/bin/mcp-rs");
        assert_eq!(
            stable_exe_path(cellar, |_| true),
            PathBuf::from("/opt/homebrew/opt/mcp-rs/bin/mcp-rs")
        );
        let linux = Path::new("/home/linuxbrew/.linuxbrew/Cellar/mcp-rs/0.29.0/bin/mcp-rs");
        assert_eq!(
            stable_exe_path(linux, |_| true),
            PathBuf::from("/home/linuxbrew/.linuxbrew/opt/mcp-rs/bin/mcp-rs")
        );
        // 版によらないパスが無ければ、そのまま
        assert_eq!(stable_exe_path(cellar, |_| false), cellar);
        // Homebrewでなければ、そのまま
        for other in [
            "C:/Users/me/.mcp-rs/bin/mcp-rs.exe",
            "/usr/local/bin/mcp-rs",
            "/opt/homebrew/Cellar/mcp-rs",
        ] {
            assert_eq!(
                stable_exe_path(Path::new(other), |_| true),
                Path::new(other)
            );
        }
    }
}
