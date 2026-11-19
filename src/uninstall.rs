//! `mcp-rs uninstall`: mcp-rsの登録を外す（#636）
//!
//! `mcp-rs init`（#575）は、Claude Desktop・Claude Code・Copilot（VS Code）に
//! mcp-rsを登録し、資格情報ストアに資格情報を入れる。やめるときは、それぞれの
//! 設定を手で探して消す必要があり、パッケージマネージャー（#578）で外しても、
//! クライアントの登録と資格情報ストアの項目は残る。
//!
//! 1つずつ確かめてから、次を外す。
//!
//! 1. Claude Desktopの`claude_desktop_config.json`の、mcp-rsの登録（控えを残す）
//! 2. Claude Code・Copilotの登録（`claude mcp remove`・`copilot mcp remove`）
//! 3. 登録が使う設定ファイル（と既定の設定ファイル）の`keyring`が指す、
//!    資格情報ストアのWordPress・データベースの項目
//! 4. 設定ファイル（既定は残す）
//!
//! 実行ファイルは消さない（Windowsでは、動いている自分自身を消せない）。
//! 入れ方（#635）に合わせて、消し方を表示する。

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::init::{
    claude_desktop_config_candidates, confirm, find_program, mcp_rs_registrations, read_json_file,
    remove_from_claude_desktop, run_quietly, select_claude_desktop_configs, Cli, Prompt, Terminal,
};
use crate::security::keyring_store::{OsKeyring, SecretStore};
use crate::update_check::InstallMethod;

/// サブコマンドの名前
pub const SUBCOMMAND: &str = "uninstall";

const HELP: &str = "Usage: mcp-rs uninstall

Removes what mcp-rs init set up, asking before each step:
  1. the mcp-rs registrations in Claude Desktop (a backup copy of its config
     file is kept), Claude Code and GitHub Copilot (VS Code)
  2. the WordPress and database credentials in the OS credential store that
     the config files refer to (keyring = \"<name>\")
  3. the config files (kept unless you answer y)

It does not delete mcp-rs itself, and shows how to remove it for the way it
was installed (Scoop, Homebrew, the install script, or by hand). Restart the
MCP clients afterwards.

Options:
  -h, --help  Print this help and exit";

/// 引数を解釈して実行する。`Ok(true)`は最後まで済んだ、`Err`は引数の誤り等
pub fn run(args: &[String]) -> Result<bool, String> {
    match args {
        [] => {}
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{HELP}");
            return Ok(true);
        }
        _ => {
            return Err(format!(
                "{SUBCOMMAND} takes no arguments. See `mcp-rs {SUBCOMMAND} --help`"
            ))
        }
    }
    if !std::io::stdin().is_terminal() {
        return Err(format!(
            "mcp-rs {SUBCOMMAND} asks before each step, so run it in a terminal"
        ));
    }
    let mut terminal = Terminal;
    let mut uninstall = Uninstall {
        io: &mut terminal,
        store: &OsKeyring,
        desktop_configs: select_claude_desktop_configs(&claude_desktop_config_candidates()),
        clis: vec![&Cli::CLAUDE_CODE, &Cli::COPILOT],
        default_config: crate::config::user_config_path().map(PathBuf::from),
        exe: std::env::current_exe().ok(),
    };
    uninstall.run().map_err(|e| e.to_string())?;
    Ok(true)
}

/// 外す手順。テストでは、入出力・資格情報ストア・設定ファイルの場所を
/// 差し替える
struct Uninstall<'a> {
    io: &'a mut dyn Prompt,
    store: &'a dyn SecretStore,
    /// Claude Desktopの設定ファイル
    desktop_configs: Vec<PathBuf>,
    /// Claude Code・Copilot
    clis: Vec<&'a Cli>,
    /// mcp-rsが既定で読む設定ファイル（`~/.config/mcp-rs/config.toml`）
    default_config: Option<PathBuf>,
    /// 今のmcp-rsの実行ファイル（消し方の案内に使う）
    exe: Option<PathBuf>,
}

impl Uninstall<'_> {
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.io.say(&format!(
            "mcp-rs {} uninstall: remove the mcp-rs registrations and credentials, asking before each step.\nPress Ctrl+C to stop at any time.\n",
            env!("CARGO_PKG_VERSION")
        ));
        // 登録が使っている設定ファイル（資格情報を探すため）
        let mut configs: Vec<PathBuf> = Vec::new();
        let mut removed = 0;
        removed += self.remove_from_claude_desktop(&mut configs)?;
        removed += self.remove_from_clis(&mut configs)?;
        if removed == 0 {
            self.io.say("No mcp-rs registrations were removed.\n");
        }

        if let Some(default) = &self.default_config {
            if default.exists() {
                configs.push(default.clone());
            }
        }
        let configs = dedup_paths(configs);
        self.remove_credentials(&configs)?;
        self.remove_config_files(&configs)?;
        self.explain_the_rest(removed > 0);
        Ok(())
    }

    /// Claude Desktopの登録を外す。外した数
    fn remove_from_claude_desktop(
        &mut self,
        configs: &mut Vec<PathBuf>,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut removed = 0;
        for path in self.desktop_configs.clone() {
            let Ok(config) = read_json_file(&path) else {
                continue;
            };
            let mut names = Vec::new();
            for registration in mcp_rs_registrations("Claude Desktop", &config) {
                if confirm(
                    self.io,
                    &format!(
                        "Remove \"{}\" ({}) from Claude Desktop ({})?",
                        registration.name,
                        describe(&registration.exe, registration.config.as_deref()),
                        path.display()
                    ),
                    true,
                )? {
                    if let Some(config) = &registration.config {
                        configs.push(PathBuf::from(config));
                    }
                    names.push(registration.name);
                }
            }
            if names.is_empty() {
                continue;
            }
            let backup = remove_from_claude_desktop(&path, &names)?;
            self.io.say(&format!(
                "Removed {} from Claude Desktop (kept the previous file as {}).\n",
                names.join(", "),
                backup.display()
            ));
            removed += names.len();
        }
        Ok(removed)
    }

    /// Claude Code・Copilotの登録を外す。外した数
    fn remove_from_clis(
        &mut self,
        configs: &mut Vec<PathBuf>,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut removed = 0;
        for cli in self.clis.clone() {
            let Some(config) = cli
                .config_file()
                .and_then(|path| read_json_file(&path).ok())
            else {
                continue;
            };
            for registration in mcp_rs_registrations(cli.label, &config) {
                if !confirm(
                    self.io,
                    &format!(
                        "Remove \"{}\" ({}) from {}?",
                        registration.name,
                        describe(&registration.exe, registration.config.as_deref()),
                        cli.label
                    ),
                    true,
                )? {
                    continue;
                }
                if let Some(config) = &registration.config {
                    configs.push(PathBuf::from(config));
                }
                let args = cli.remove_args(&registration.name);
                let command = format!("{} {}", cli.program, args.join(" "));
                match find_program(cli.program) {
                    Some(program) => match run_quietly(&program, &args) {
                        Ok(()) => {
                            self.io.say(&format!(
                                "Removed \"{}\" from {}.\n",
                                registration.name, cli.label
                            ));
                            removed += 1;
                        }
                        Err(e) => self.io.say(&format!(
                            "Could not remove it ({e}). Run this yourself:\n  {command}\n"
                        )),
                    },
                    None => self.io.say(&format!(
                        "'{}' was not found. Run this yourself:\n  {command}\n",
                        cli.program
                    )),
                }
            }
        }
        Ok(removed)
    }

    /// 設定ファイルの`keyring`が指す、資格情報ストアの項目を消す
    fn remove_credentials(
        &mut self,
        configs: &[PathBuf],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut names: Vec<(String, &'static str)> = Vec::new();
        for config in configs {
            for entry in keyring_entries(config) {
                if !names.iter().any(|(name, _)| *name == entry.0) {
                    names.push(entry);
                }
            }
        }
        for (name, what) in names {
            // 無い項目は聞かない
            if !matches!(self.store.get(&name), Ok(Some(_))) {
                continue;
            }
            if confirm(
                self.io,
                &format!("Delete the {what} credentials '{name}' from the OS credential store?"),
                true,
            )? {
                self.store.delete(&name)?;
                self.io
                    .say(&format!("Deleted '{name}' from the OS credential store.\n"));
            }
        }
        Ok(())
    }

    /// 設定ファイルを消すかを聞く（既定は残す）
    fn remove_config_files(
        &mut self,
        configs: &[PathBuf],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for config in configs.iter().filter(|config| config.exists()) {
            if confirm(
                self.io,
                &format!("Delete the config file {}?", config.display()),
                false,
            )? {
                std::fs::remove_file(config)?;
                self.io.say(&format!("Deleted {}.\n", config.display()));
            } else {
                self.io.say(&format!("Kept {}.\n", config.display()));
            }
        }
        Ok(())
    }

    /// 消さなかったもの（実行ファイル・ログ）の消し方
    fn explain_the_rest(&mut self, removed_any: bool) {
        let mut lines = Vec::new();
        if removed_any {
            lines.push(
                "Restart the MCP clients (Claude Desktop: quit it from the system tray or menu bar and start it again)."
                    .to_string(),
            );
        }
        let exe = self.exe.clone();
        let method = exe
            .as_deref()
            .map(InstallMethod::detect)
            .unwrap_or(InstallMethod::Other);
        lines.push(match method {
            InstallMethod::Scoop => {
                "To remove mcp-rs itself: scoop uninstall mcp-rs (and scoop bucket rm n-takatsu)"
                    .to_string()
            }
            InstallMethod::Homebrew => {
                "To remove mcp-rs itself: brew uninstall mcp-rs (and brew untap n-takatsu/tap)"
                    .to_string()
            }
            InstallMethod::InstallScript => format!(
                "To remove mcp-rs itself, delete the folder {} after this command ends (it also holds the logs)",
                exe.as_deref()
                    .and_then(Path::parent)
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_else(|| "~/.mcp-rs/bin".to_string())
            ),
            InstallMethod::Other => format!(
                "To remove mcp-rs itself, delete {} (and the logs folder next to it, if any)",
                exe.as_deref()
                    .map(|exe| exe.display().to_string())
                    .unwrap_or_else(|| "the mcp-rs executable".to_string())
            ),
        });
        if let Some(audit) = crate::logging::user_audit_dir().filter(|dir| dir.exists()) {
            lines.push(format!(
                "The audit log is in {} (delete it if you no longer need it).",
                audit.display()
            ));
        }
        self.io.say(&lines.join("\n"));
    }
}

/// 登録の説明（実行ファイルと設定ファイル）
fn describe(exe: &str, config: Option<&str>) -> String {
    match config {
        Some(config) => format!("{exe} --config {config}"),
        None => exe.to_string(),
    }
}

/// 同じファイルを1つにする（区切り文字・Windowsの大文字と小文字をそろえる）
fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let key = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(key(path)))
        .collect()
}

/// 設定ファイルの`keyring`の名前（WordPress・データベース）。読めなければ無し
fn keyring_entries(path: &Path) -> Vec<(String, &'static str)> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(table) = content.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let keyring = |value: Option<&toml::Value>| {
        value
            .and_then(|v| v.get("keyring"))
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    };
    let wordpress = table.get("handlers").and_then(|h| h.get("wordpress"));
    if let Some(name) = keyring(wordpress) {
        entries.push((name, "WordPress"));
    }
    // ほかのサイト（#640）
    if let Some(sites) = wordpress
        .and_then(|w| w.get("sites"))
        .and_then(toml::Value::as_table)
    {
        for site in sites.values() {
            if let Some(name) = keyring(Some(site)) {
                if !entries.iter().any(|(existing, _)| *existing == name) {
                    entries.push((name, "WordPress"));
                }
            }
        }
    }
    if let Some(name) = keyring(table.get("database")) {
        entries.push((name, "database"));
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::keyring_store::MemoryStore;
    use std::collections::VecDeque;
    use zeroize::Zeroizing;

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
        fn ask(&mut self, question: &str) -> std::io::Result<String> {
            self.shown.push(question.to_string());
            self.answers.pop_front().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "no more answers")
            })
        }
        fn ask_hidden(&mut self, question: &str) -> std::io::Result<Zeroizing<String>> {
            self.ask(question).map(Zeroizing::new)
        }
        fn say(&mut self, text: &str) {
            self.shown.push(text.to_string());
        }
    }

    fn json_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    /// Claude Desktopの設定（mcp-rsの登録2つ、ほかのサーバー1つ）と、
    /// 設定ファイル・資格情報ストア
    fn fixture(dir: &Path) -> (PathBuf, PathBuf, MemoryStore) {
        let config = dir.join("config.toml");
        std::fs::write(
            &config,
            "[server]\n\n[transport]\ntype = \"stdio\"\n\n[handlers.wordpress]\nenabled = true\nurl = \"https://example.com\"\nkeyring = \"example.com\"\n\n[database]\nengine = \"postgresql\"\ndatabase = \"app\"\nusername = \"reader\"\nkeyring = \"app@localhost\"\n",
        )
        .unwrap();
        let desktop = dir.join("claude_desktop_config.json");
        std::fs::write(
            &desktop,
            serde_json::to_string_pretty(&serde_json::json!({
                "mcpServers": {
                    "filesystem": { "command": "npx", "args": ["x"] },
                    "mcp-rs": {
                        "command": "C:/Users/me/.mcp-rs/bin/mcp-rs.exe",
                        "args": ["--config", json_path(&config)]
                    },
                    "mcp-rs-old": { "command": "C:/old/mcp-rs.exe", "args": [] }
                },
                "preferences": { "x": 1 }
            }))
            .unwrap(),
        )
        .unwrap();
        let store = MemoryStore::default();
        store.set("example.com", "wordpress secret").unwrap();
        store.set("app@localhost", "database secret").unwrap();
        store.set("unrelated", "keep").unwrap();
        (desktop, config, store)
    }

    fn uninstall<'a>(io: &'a mut Script, store: &'a MemoryStore, desktop: &Path) -> Uninstall<'a> {
        Uninstall {
            io,
            store,
            desktop_configs: vec![desktop.to_path_buf()],
            clis: Vec::new(),
            default_config: None,
            exe: Some(PathBuf::from(
                r"C:\Users\me\scoop\apps\mcp-rs\current\mcp-rs.exe",
            )),
        }
    }

    #[test]
    fn test_uninstall_removes_registrations_and_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let (desktop, config, store) = fixture(dir.path());
        // 2つの登録を外し、2つの資格情報を消し、設定ファイルは残す
        let mut io = Script::new(&["", "", "", "", ""]);
        uninstall(&mut io, &store, &desktop).run().unwrap();

        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&desktop).unwrap()).unwrap();
        assert_eq!(
            after["mcpServers"],
            serde_json::json!({ "filesystem": { "command": "npx", "args": ["x"] } })
        );
        assert_eq!(after["preferences"], serde_json::json!({ "x": 1 }));
        assert!(matches!(store.get("example.com"), Ok(None)));
        assert!(matches!(store.get("app@localhost"), Ok(None)));
        assert!(matches!(store.get("unrelated"), Ok(Some(_))));
        assert!(config.exists());
        // 控えが残る
        assert!(std::fs::read_dir(dir.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("claude_desktop_config.json.bak-")));
        let shown = io.shown.join("\n");
        assert!(shown.contains("scoop uninstall mcp-rs"), "{shown}");
        assert!(shown.contains("Restart the MCP clients"), "{shown}");
    }

    #[test]
    fn test_uninstall_asks_before_each_step() {
        let dir = tempfile::tempdir().unwrap();
        let (desktop, config, store) = fixture(dir.path());
        let before = std::fs::read_to_string(&desktop).unwrap();
        // 1つ目の登録だけ外す。資格情報は消さない。設定ファイルを消す
        let mut io = Script::new(&["y", "n", "n", "n", "y"]);
        uninstall(&mut io, &store, &desktop).run().unwrap();

        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&desktop).unwrap()).unwrap();
        let names: Vec<&String> = after["mcpServers"].as_object().unwrap().keys().collect();
        assert_eq!(names, ["filesystem", "mcp-rs-old"]);
        assert_ne!(before, std::fs::read_to_string(&desktop).unwrap());
        assert!(matches!(store.get("example.com"), Ok(Some(_))));
        assert!(matches!(store.get("app@localhost"), Ok(Some(_))));
        assert!(!config.exists());
    }

    #[test]
    fn test_uninstall_with_nothing_registered() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = dir.path().join("claude_desktop_config.json");
        std::fs::write(&desktop, r#"{"mcpServers":{"other":{"command":"x"}}}"#).unwrap();
        let store = MemoryStore::default();
        let mut io = Script::new(&[]);
        let mut steps = uninstall(&mut io, &store, &desktop);
        steps.exe = Some(PathBuf::from("/home/me/.mcp-rs/bin/mcp-rs"));
        steps.run().unwrap();
        let shown = io.shown.join("\n");
        assert!(
            shown.contains("No mcp-rs registrations were removed"),
            "{shown}"
        );
        assert!(
            shown.contains("delete the folder /home/me/.mcp-rs/bin"),
            "{shown}"
        );
        assert!(!shown.contains("Restart the MCP clients"), "{shown}");
        // 何も変えない（控えも作らない）
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn test_keyring_entries_and_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let (_desktop, config, _store) = fixture(dir.path());
        assert_eq!(
            keyring_entries(&config),
            vec![
                ("example.com".to_string(), "WordPress"),
                ("app@localhost".to_string(), "database")
            ]
        );
        assert!(keyring_entries(&dir.path().join("missing.toml")).is_empty());
        let paths = dedup_paths(vec![
            PathBuf::from("C:/a/config.toml"),
            PathBuf::from(r"C:\a\config.toml"),
            PathBuf::from("C:/b/config.toml"),
        ]);
        assert_eq!(paths.len(), 2);
    }

    /// ほかのWordPressのサイトの項目も、消すかを聞く（#640）。同じ名前は1回
    #[test]
    fn test_keyring_entries_include_other_sites() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(
            &config,
            "[handlers.wordpress]\nenabled = true\ndefault_site = \"blog\"\n\n\
             [handlers.wordpress.sites.blog]\nurl = \"https://blog.example.com\"\nkeyring = \"blog\"\n\n\
             [handlers.wordpress.sites.shop]\nurl = \"https://shop.example.com\"\nkeyring = \"shop\"\n\n\
             [handlers.wordpress.sites.again]\nurl = \"https://blog.example.com\"\nkeyring = \"blog\"\n",
        )
        .unwrap();
        assert_eq!(
            keyring_entries(&config),
            vec![
                ("blog".to_string(), "WordPress"),
                ("shop".to_string(), "WordPress")
            ]
        );
    }

    #[test]
    fn test_run_rejects_arguments() {
        assert!(run(&["--force".to_string()]).is_err());
    }
}
