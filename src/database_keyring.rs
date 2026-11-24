//! `mcp-rs database-keyring`: `[database]`のパスワードを、OSの資格情報
//! ストアに入れる・見る・消す（#628）
//!
//! `[database]`の`keyring`（#590）が指す項目を、`mcp-rs init`を使わずに
//! 扱う。`[database]`を手で書いた人や、パスワードを変えた人のため。
//! WordPressの`wordpress-keyring`とは分ける（使い方を変えないため）。
//!
//! - `set <名前>`: 設定ファイルに`[database]`があれば、そのホスト・ポート・
//!   ユーザー名を既定にしてパスワードを入れ直し、`[database]`を`keyring`を
//!   使う形に書き換えるかを聞く（コメント等は残す）。無ければ、
//!   `mcp-rs init`と同じ質問で項目を作り、`[database]`を書き足すかを聞く
//! - `show <名前>`: 項目のホスト・ポート・ユーザー名（パスワードは出さない）
//! - `delete <名前>`: 項目を消す。データベースの項目でなければ消さない
//!
//! パスワードは引数で受け取らない。画面にも出さない。

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::config::DatabaseSettings;
use crate::init::{
    append_to_config_file, ask_database, ask_database_password, ask_with_default, check_store,
    confirm, database_toml, make_backup, write_atomically, Prompt, Terminal,
};
use crate::security::keyring_store::{
    database_entry, store_database_credentials, DatabaseEntry, DatabaseKeyringCredentials,
    OsKeyring, SecretStore,
};

/// サブコマンドの名前
pub const SUBCOMMAND: &str = "database-keyring";

const HELP: &str = "Usage: mcp-rs database-keyring set <name> [--config <path>]
       mcp-rs database-keyring show <name>
       mcp-rs database-keyring delete <name>

Keeps the password of [database] in the OS credential store (Windows
Credential Manager, macOS Keychain, Secret Service on Linux), for
`keyring = \"<name>\"` in [database] of the config file.

  set     Stores the password as <name>. If the config file has [database],
          its host, port and user are the defaults, and it offers to change
          [database] to use keyring = \"<name>\" (removing password; a backup
          copy is kept, and comments are kept). Without [database], it asks
          the same questions as mcp-rs init and offers to add [database]
  show    Shows the host, port and user the entry is for (not the password)
  delete  Deletes the entry

The entry is tied to the host, port and user: with other values in
[database], mcp-rs does not use it, so a config file someone else wrote
cannot send the password to another server. Passwords are never taken as
arguments and are not shown while typing. mcp-rs init can also set up
[database]. For WordPress, use mcp-rs wordpress-keyring.

Options:
  --config <path>  Config file to read and change (set only). Without it,
                   ~/.config/mcp-rs/config.toml
  -h, --help       Print this help and exit";

/// 引数が求めること
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Help,
    Set {
        name: String,
        config: Option<String>,
    },
    Show(String),
    Delete(String),
}

fn parse(args: &[String]) -> Result<Action, String> {
    let usage = format!("See `mcp-rs {SUBCOMMAND} --help`.");
    let mut rest = args.iter();
    let action = match rest.next().map(String::as_str) {
        None => return Err(format!("{SUBCOMMAND} needs set, show or delete. {usage}")),
        Some("-h" | "--help") => return Ok(Action::Help),
        Some(action @ ("set" | "show" | "delete")) => action,
        Some(other) => {
            return Err(format!(
                "unknown action '{other}' (use set, show or delete). {usage}"
            ))
        }
    };
    let mut name = None;
    let mut config = None;
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--config" if action == "set" => match rest.next() {
                Some(path) => config = Some(path.clone()),
                None => return Err(format!("--config needs a path. {usage}")),
            },
            _ if arg.starts_with("--config=") && action == "set" => {
                config = Some(arg["--config=".len()..].to_string());
            }
            _ if name.is_none() && !arg.starts_with('-') => name = Some(arg.trim().to_string()),
            // 引数は表示しない。誤ってパスワードを渡したとき、もう一度
            // 表示してしまうため（wordpress-keyringと同じ）
            _ => {
                return Err(format!(
                    "{SUBCOMMAND} {action} takes an entry name{} only. Passwords are read from the terminal, never from arguments. {usage}",
                    if action == "set" { " and --config <path>" } else { "" }
                ))
            }
        }
    }
    let name = match name {
        Some(name) if !name.is_empty() => name,
        Some(_) => return Err(format!("the entry name is empty. {usage}")),
        None => return Err(format!("{action} needs an entry name. {usage}")),
    };
    Ok(match action {
        "set" => Action::Set { name, config },
        "show" => Action::Show(name),
        _ => Action::Delete(name),
    })
}

/// サブコマンドを実行する。`args`は、サブコマンドの名前より後ろの引数
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match parse(args)? {
        Action::Help => println!("{HELP}"),
        Action::Set { name, config } => {
            if !std::io::stdin().is_terminal() {
                return Err(format!(
                    "{SUBCOMMAND} set asks questions, so run it in a terminal. Without one, use password = \"${{VAR}}\" in [database] instead"
                )
                .into());
            }
            let path = config_path(config.as_deref())?;
            set(&mut Terminal, &OsKeyring, &name, &path)?;
        }
        Action::Show(name) => println!("{}", show(&OsKeyring, &name)?),
        Action::Delete(name) => eprintln!("{}", delete(&OsKeyring, &name)?),
    }
    Ok(())
}

/// 読み書きする設定ファイル（`mcp-rs init`と同じ決め方）
fn config_path(config: Option<&str>) -> Result<PathBuf, String> {
    let path = match config {
        Some(path) => crate::config::expand_config_path(path),
        None => crate::config::user_config_path().ok_or_else(|| {
            "cannot find your home folder: pass the config file with --config <path>".to_string()
        })?,
    };
    std::path::absolute(&path).map_err(|e| format!("cannot use {path}: {e}"))
}

/// `show`の表示
fn show(store: &dyn SecretStore, name: &str) -> Result<String, String> {
    match database_entry(store, name)? {
        Some(DatabaseEntry::Database {
            host,
            port,
            username,
        }) => Ok(format!(
            "host: {host}\nport: {port}\nusername: {username}\npassword: (stored, not shown)"
        )),
        Some(DatabaseEntry::Other) => Err(not_database(name)),
        None => Err(format!("no database credentials named '{name}'")),
    }
}

/// `delete`。消したら、その知らせ
fn delete(store: &dyn SecretStore, name: &str) -> Result<String, String> {
    match database_entry(store, name)? {
        Some(DatabaseEntry::Database { .. }) => {
            store.delete(name.trim()).map_err(|e| e.to_string())?;
            Ok(format!(
                "Deleted the database credentials named '{}'. Remove keyring = \"{}\" from [database] too.",
                name.trim(),
                name.trim()
            ))
        }
        // WordPressの資格情報等は、このコマンドでは消さない
        Some(DatabaseEntry::Other) => Err(not_database(name)),
        None => Err(format!("no database credentials named '{name}' to delete")),
    }
}

fn not_database(name: &str) -> String {
    format!(
        "the entry named '{name}' is not mcp-rs database credentials (for WordPress credentials, use mcp-rs wordpress-keyring)"
    )
}

/// `set`
fn set(
    io: &mut dyn Prompt,
    store: &dyn SecretStore,
    name: &str,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    check_store(store)?;
    // WordPressの資格情報等を、データベースの資格情報で上書きしない。
    // データベースの資格情報なら、今までどおり入れ直せる（#692）
    if let Some(DatabaseEntry::Other) = database_entry(store, name)? {
        return Err(not_database(name).into());
    }
    let content = std::fs::read_to_string(path).ok();
    let existing = match &content {
        Some(content) => database_section(content).map_err(|e| {
            format!(
                "cannot read [database] in {}: {e}. Fix it, or pass another file with --config",
                path.display()
            )
        })?,
        None => None,
    };
    match (content, existing) {
        (Some(content), Some(settings)) => update(io, store, name, path, &content, &settings),
        (content, _) => {
            io.say(&format!(
                "{} has no [database]. Answer a few questions to set it up.\n",
                path.display()
            ));
            let answers = ask_database(io, store, Some(name), &[])?;
            if content.is_some() {
                if confirm(io, &format!("Add [database] to {}?", path.display()), true)? {
                    let backup = append_to_config_file(path, &database_toml(&answers)?)?;
                    io.say(&format!(
                        "Added [database] to {} (kept the previous file as {}).",
                        path.display(),
                        backup.display()
                    ));
                    return Ok(());
                }
            } else {
                io.say(&format!(
                    "There is no config file at {}. Run mcp-rs init to create one.",
                    path.display()
                ));
            }
            io.say("Add this to the config file:\n");
            io.say(&database_toml(&answers)?);
            Ok(())
        }
    }
}

/// 設定ファイルの`[database]`。無ければ`None`
fn database_section(content: &str) -> Result<Option<DatabaseSettings>, String> {
    let table: toml::Table = content
        .parse()
        .map_err(|e: toml::de::Error| e.to_string())?;
    match table.get("database") {
        None => Ok(None),
        Some(value) => value
            .clone()
            .try_into()
            .map(Some)
            .map_err(|e: toml::de::Error| e.message().to_string()),
    }
}

/// つなぐ先の値を聞く。展開できなかった`${VAR}`は既定にせず、値そのものを
/// 聞く。既定がある場合も、`${...}`の答えは断る。資格情報には、mcp-rsが
/// 実際につなぐ先を入れるため（#692・PR #696のレビュー）。`expanded`は、
/// `[database]`の展開ができたか。できたなら、値に`${`があっても既定にする
/// （展開した値はそのまま使われるため。#774）
fn ask_resolved(
    io: &mut dyn Prompt,
    label: &str,
    value: &str,
    expanded: bool,
) -> std::io::Result<String> {
    let has_default = expanded || !value.contains("${");
    loop {
        let answer = if has_default {
            ask_with_default(io, label, value)?
        } else {
            io.ask(&format!(
                "{label} that mcp-rs connects to ({value} could not be expanded): "
            ))?
        };
        if !answer.is_empty() && (!answer.contains("${") || (has_default && answer == value)) {
            return Ok(answer);
        }
        io.say("Enter the value itself, not an environment variable.");
    }
}

/// `[database]`がある場合: そのホスト・ポート・ユーザー名を既定にして
/// パスワードを入れ直し、`[database]`を`keyring`を使う形にするかを聞く
fn update(
    io: &mut dyn Prompt,
    store: &dyn SecretStore,
    name: &str,
    path: &Path,
    content: &str,
    written: &DatabaseSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    // 既定は、`${VAR}`を展開した値（項目は、mcp-rsが実際につなぐ先と
    // 結び付けるため）。展開できなければ、書いてあるまま見せ、展開できな
    // かった値は既定にしない（`ask_resolved`）
    let mut expanded = written.clone();
    let expanded_ok = match crate::config::McpConfig::expand_database_config(&mut expanded) {
        // 無効な`[database]`は展開しない。値はそのまま
        Ok(()) => expanded.is_enabled(),
        Err(e) => {
            expanded = written.clone();
            io.say(&format!("Could not expand [database] ({e})."));
            false
        }
    };
    io.say(&format!(
        "[database] in {}: {}",
        path.display(),
        expanded.describe()
    ));
    let host = ask_resolved(io, "Host", expanded.host(), expanded_ok)?;
    let default_port = expanded.port();
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
    let username = ask_resolved(io, "User name", &expanded.username, expanded_ok)?;
    let password = ask_database_password(io)?;
    let credentials = DatabaseKeyringCredentials {
        host: host.clone(),
        port,
        username: username.clone(),
        password,
    };

    let changes = Changes {
        keyring: written.keyring.as_deref() != Some(name),
        password: written.password.is_some(),
        host: (host != expanded.host()).then(|| host.clone()),
        port: (port != expanded.port()).then_some(port),
        username: (username != expanded.username).then(|| username.clone()),
    };
    if changes.is_empty() {
        save(store, name, &credentials)?;
        io.say(&format!(
            "Stored the password as '{name}' in the OS credential store."
        ));
        io.say(
            "[database] already uses it. mcp-rs reads the new password the next time it starts.",
        );
        return Ok(());
    }
    let summary = changes.summary(name);
    // 設定が既にこの項目を指していて、つなぐ先が変わる: 項目だけを変えると、
    // 設定と食い違い、起動で使えなくなる。保存の前に聞き、設定を書けな
    // かったら項目を前の中身に戻す（#774）
    let target_changes =
        changes.host.is_some() || changes.port.is_some() || changes.username.is_some();
    if !changes.keyring && target_changes {
        if !confirm(
            io,
            &format!(
                "[database] in {} already uses '{name}'. Store the password and change [database]: {summary}?",
                path.display()
            ),
            true,
        )? {
            io.say(&format!(
                "Kept '{name}' and the config file as they are."
            ));
            return Ok(());
        }
        let previous = store.get(name.trim()).map_err(|e| e.to_string())?;
        save(store, name, &credentials)?;
        io.say(&format!(
            "Stored the password as '{name}' in the OS credential store."
        ));
        if let Err(e) = change_config(io, path, content, name, &changes) {
            let restored = match &previous {
                Some(previous) => store.set(name.trim(), previous),
                None => store.delete(name.trim()).map(|_| ()),
            };
            return Err(match restored {
                Ok(()) => format!("{e} (put '{name}' back as it was)"),
                Err(r) => format!("{e} (and could not put '{name}' back as it was: {r})"),
            }
            .into());
        }
        return Ok(());
    }
    save(store, name, &credentials)?;
    io.say(&format!(
        "Stored the password as '{name}' in the OS credential store."
    ));
    if confirm(
        io,
        &format!("Change [database] in {}: {summary}?", path.display()),
        true,
    )? {
        change_config(io, path, content, name, &changes)?;
    } else {
        io.say(&format!(
            "Kept the config file as it is. To use the stored password, change [database]: {summary}."
        ));
    }
    Ok(())
}

/// 項目を保存する。保存の直前に、別の種類の項目になっていないかを
/// もう一度確かめる（質問の間に別の端末で作られた場合。#774）
fn save(
    store: &dyn SecretStore,
    name: &str,
    credentials: &DatabaseKeyringCredentials,
) -> Result<(), String> {
    if let Some(DatabaseEntry::Other) = database_entry(store, name)? {
        return Err(not_database(name));
    }
    store_database_credentials(store, name, credentials)
}

/// `[database]`に`changes`を加えて書く。前のファイルは控えに残す
fn change_config(
    io: &mut dyn Prompt,
    path: &Path,
    content: &str,
    name: &str,
    changes: &Changes,
) -> Result<(), Box<dyn std::error::Error>> {
    let updated = edit_database(content, name, changes)?;
    let backup = make_backup(path, true)?;
    write_atomically(path, &updated, true)?;
    io.say(&format!(
        "Changed [database] in {} (kept the previous file as {}). mcp-rs uses it the next time it starts.",
        path.display(),
        backup.display()
    ));
    Ok(())
}

/// `[database]`に加える変更
#[derive(Debug, Default, PartialEq, Eq)]
struct Changes {
    /// `keyring = "<名前>"`にする
    keyring: bool,
    /// `password`を消す
    password: bool,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
}

impl Changes {
    fn is_empty(&self) -> bool {
        *self == Changes::default()
    }

    fn summary(&self, name: &str) -> String {
        let mut parts = Vec::new();
        if self.keyring {
            parts.push(format!("keyring = \"{name}\""));
        }
        if self.password {
            parts.push("remove password".to_string());
        }
        if let Some(host) = &self.host {
            parts.push(format!("host = \"{host}\""));
        }
        if let Some(port) = self.port {
            parts.push(format!("port = {port}"));
        }
        if let Some(username) = &self.username {
            parts.push(format!("username = \"{username}\""));
        }
        parts.join(", ")
    }
}

/// `[database]`に`changes`を加えた設定ファイルの内容。ほかの部分（コメント・
/// 順番・書き方）は変えない
fn edit_database(content: &str, name: &str, changes: &Changes) -> Result<String, String> {
    let mut document: toml_edit::DocumentMut = content.parse().map_err(|e| format!("{e}"))?;
    let database = document
        .get_mut("database")
        .and_then(toml_edit::Item::as_table_like_mut)
        .ok_or_else(|| "the config file has no [database] table".to_string())?;
    if changes.password {
        database.remove("password");
    }
    if changes.keyring {
        database.insert("keyring", toml_edit::value(name));
    }
    if let Some(host) = &changes.host {
        database.insert("host", toml_edit::value(host.as_str()));
    }
    if let Some(port) = changes.port {
        database.insert("port", toml_edit::value(i64::from(port)));
    }
    if let Some(username) = &changes.username {
        database.insert("username", toml_edit::value(username.as_str()));
    }
    Ok(document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::keyring_store::{
        load_database_credentials, store_wordpress_credentials, MemoryStore,
        WordPressKeyringCredentials,
    };
    use std::collections::VecDeque;
    use zeroize::Zeroizing;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
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

    const HAND_WRITTEN: &str = r#"# mcp-rs (written by hand)
[server]

[transport]
type = "stdio"

[handlers]

# the app's database
[database]
engine = "postgresql"
host = "db.example.com"   # production
database = "app"
username = "reader"
password = "${MCP_DB_PASSWORD}"
ssl_mode = "verify-full"
"#;

    fn config_in_temp(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn test_parse() {
        assert_eq!(parse(&args(&["--help"])).unwrap(), Action::Help);
        assert_eq!(
            parse(&args(&["set", "app@db"])).unwrap(),
            Action::Set {
                name: "app@db".to_string(),
                config: None
            }
        );
        assert_eq!(
            parse(&args(&["set", "app@db", "--config", "C:/x.toml"])).unwrap(),
            Action::Set {
                name: "app@db".to_string(),
                config: Some("C:/x.toml".to_string())
            }
        );
        assert_eq!(
            parse(&args(&["set", "--config=C:/x.toml", "a"])).unwrap(),
            Action::Set {
                name: "a".to_string(),
                config: Some("C:/x.toml".to_string())
            }
        );
        assert_eq!(
            parse(&args(&["show", "a"])).unwrap(),
            Action::Show("a".to_string())
        );
        assert_eq!(
            parse(&args(&["delete", "a"])).unwrap(),
            Action::Delete("a".to_string())
        );
        assert!(parse(&args(&[])).is_err());
        assert!(parse(&args(&["set"])).unwrap_err().contains("entry name"));
        assert!(parse(&args(&["set", " "])).unwrap_err().contains("empty"));
        assert!(parse(&args(&["show", "a", "--config", "x"])).is_err());
        // 誤って渡したパスワードを、エラーに出さない
        let err = parse(&args(&["set", "a", "my-secret-password"])).unwrap_err();
        assert!(!err.contains("my-secret-password"), "{err}");
    }

    /// `[database]`がある: パスワードを入れ直し、`keyring`を使う形に
    /// 書き換える。コメントは残る
    #[test]
    fn test_set_updates_an_existing_section() {
        let (_dir, path) = config_in_temp(HAND_WRITTEN);
        let store = MemoryStore::default();
        let mut io = Script::new(&["", "", "", "s3cret", ""]);
        set(&mut io, &store, "app@db", &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# the app's database"), "{content}");
        assert!(content.contains("# production"), "{content}");
        assert!(!content.contains("password"), "{content}");
        assert!(content.contains("keyring = \"app@db\""), "{content}");
        assert!(!content.contains("s3cret"));
        let stored =
            load_database_credentials(&store, "app@db", "db.example.com", 5432, "reader").unwrap();
        assert_eq!(*stored.password, "s3cret");
        // 書き換えた設定ファイルで、パスワードが読める
        let mut db = database_section(&content).unwrap().unwrap();
        crate::config::McpConfig::resolve_database_credentials_with(&mut db, &store).unwrap();
        assert_eq!(db.password.as_deref(), Some("s3cret"));
        assert!(io
            .shown
            .iter()
            .any(|s| s.contains("keyring = \"app@db\", remove password")));
    }

    /// 既に`keyring`を使っている: パスワードだけを入れ直し、ファイルは変えない
    #[test]
    fn test_set_only_replaces_the_password_when_keyring_is_used() {
        let config =
            HAND_WRITTEN.replace("password = \"${MCP_DB_PASSWORD}\"", "keyring = \"app@db\"");
        let (_dir, path) = config_in_temp(&config);
        let store = MemoryStore::default();
        let mut io = Script::new(&["", "", "", "new-pass"]);
        set(&mut io, &store, "app@db", &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), config);
        let stored =
            load_database_credentials(&store, "app@db", "db.example.com", 5432, "reader").unwrap();
        assert_eq!(*stored.password, "new-pass");
        assert!(io.shown.iter().any(|s| s.contains("already uses it")));
    }

    /// ホスト等を変えたら、`[database]`も変える。断れば、ファイルは変えない
    #[test]
    fn test_set_with_another_host_and_declining_the_change() {
        let (_dir, path) = config_in_temp(HAND_WRITTEN);
        let store = MemoryStore::default();
        let mut io = Script::new(&["db2.example.com", "6543", "", "s3cret", "y"]);
        set(&mut io, &store, "app@db2", &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("host = \"db2.example.com\""), "{content}");
        assert!(content.contains("port = 6543"), "{content}");
        assert!(content.contains("username = \"reader\""), "{content}");

        let (_dir, path) = config_in_temp(HAND_WRITTEN);
        let mut io = Script::new(&["", "", "", "s3cret", "n"]);
        set(&mut io, &store, "app@db", &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), HAND_WRITTEN);
        assert!(io.shown.iter().any(|s| s.contains("Kept the config file")));
    }

    /// 設定が既にこの項目を指していて、ホストを変える: 保存の前に聞く。
    /// 断れば、項目も設定ファイルも変えない（#774）
    #[test]
    fn test_set_asks_before_changing_the_entry_the_config_uses() {
        let config =
            HAND_WRITTEN.replace("password = \"${MCP_DB_PASSWORD}\"", "keyring = \"app@db\"");
        let store = MemoryStore::default();
        store_database_credentials(
            &store,
            "app@db",
            &DatabaseKeyringCredentials {
                host: "db.example.com".to_string(),
                port: 5432,
                username: "reader".to_string(),
                password: Zeroizing::new("old-pass".to_string()),
            },
        )
        .unwrap();

        let (_dir, path) = config_in_temp(&config);
        let mut io = Script::new(&["db2.example.com", "", "", "new-pass", "n"]);
        set(&mut io, &store, "app@db", &path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), config);
        let stored =
            load_database_credentials(&store, "app@db", "db.example.com", 5432, "reader").unwrap();
        assert_eq!(*stored.password, "old-pass");

        // 認めれば、項目と設定ファイルの両方を変える
        let mut io = Script::new(&["db2.example.com", "", "", "new-pass", "y"]);
        set(&mut io, &store, "app@db", &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("host = \"db2.example.com\""), "{content}");
        let stored =
            load_database_credentials(&store, "app@db", "db2.example.com", 5432, "reader").unwrap();
        assert_eq!(*stored.password, "new-pass");
    }

    /// パスワードを聞いている間に、同じ名前でWordPressの資格情報が
    /// 作られた: 上書きしない（#774）
    #[test]
    fn test_set_checks_the_entry_again_before_saving() {
        struct Racing<'a> {
            script: Script,
            store: &'a MemoryStore,
        }
        impl Prompt for Racing<'_> {
            fn ask(&mut self, question: &str) -> std::io::Result<String> {
                self.script.ask(question)
            }
            fn ask_hidden(&mut self, question: &str) -> std::io::Result<Zeroizing<String>> {
                store_wordpress_credentials(
                    self.store,
                    "app@db",
                    &WordPressKeyringCredentials {
                        site_url: "https://example.com".to_string(),
                        username: "editor".to_string(),
                        application_password: Zeroizing::new("app pass".to_string()),
                    },
                )
                .unwrap();
                self.script.ask_hidden(question)
            }
            fn say(&mut self, text: &str) {
                self.script.say(text);
            }
        }
        let (_dir, path) = config_in_temp(HAND_WRITTEN);
        let store = MemoryStore::default();
        let mut io = Racing {
            script: Script::new(&["", "", "", "s3cret", ""]),
            store: &store,
        };
        let err = set(&mut io, &store, "app@db", &path)
            .unwrap_err()
            .to_string();
        assert!(err.contains("is not mcp-rs database credentials"), "{err}");
        assert!(
            crate::security::keyring_store::load_wordpress_credentials(&store, "app@db").is_ok()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), HAND_WRITTEN);
    }

    /// 展開できた値は、`${`を含んでも既定にする（#774）
    #[test]
    fn test_set_uses_an_expanded_value_with_a_dollar_brace() {
        std::env::set_var("MCP_RS_TEST_DB_HOST_774", "svc${blue}");
        let config = HAND_WRITTEN
            .replace(
                "host = \"db.example.com\"",
                "host = \"${MCP_RS_TEST_DB_HOST_774}\"",
            )
            .replace("password = \"${MCP_DB_PASSWORD}\"", "password = \"plain\"");
        let (_dir, path) = config_in_temp(&config);
        let store = MemoryStore::default();
        let mut io = Script::new(&["", "", "", "s3cret", "n"]);
        set(&mut io, &store, "app@db", &path).unwrap();
        let stored =
            load_database_credentials(&store, "app@db", "svc${blue}", 5432, "reader").unwrap();
        assert_eq!(stored.host, "svc${blue}");
        assert!(!io
            .shown
            .iter()
            .any(|s| s == "Enter the value itself, not an environment variable."));
    }

    /// WordPressの資格情報等の名前には、データベースの資格情報を入れない
    /// （#692）
    #[test]
    fn test_set_refuses_an_entry_of_another_kind() {
        let (_dir, path) = config_in_temp(HAND_WRITTEN);
        let store = MemoryStore::default();
        store_wordpress_credentials(
            &store,
            "site",
            &WordPressKeyringCredentials {
                site_url: "https://example.com".to_string(),
                username: "editor".to_string(),
                application_password: Zeroizing::new("app pass".to_string()),
            },
        )
        .unwrap();
        let mut io = Script::new(&[]);
        let err = set(&mut io, &store, "site", &path).unwrap_err().to_string();
        assert!(err.contains("is not mcp-rs database credentials"), "{err}");
        // WordPressの資格情報は、そのまま
        assert!(crate::security::keyring_store::load_wordpress_credentials(&store, "site").is_ok());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), HAND_WRITTEN);
    }

    /// 展開できない`${VAR}`のホストは既定にせず、値そのものを聞く。
    /// 資格情報には、mcp-rsが実際につなぐ先を入れる（#692）
    #[test]
    fn test_set_does_not_store_an_unexpanded_host() {
        let config = HAND_WRITTEN.replace(
            "host = \"db.example.com\"",
            "host = \"${MCP_RS_TEST_UNSET_DB_HOST_692}\"",
        );
        let (_dir, path) = config_in_temp(&config);
        let store = MemoryStore::default();
        let mut io = Script::new(&[
            "",               // 既定は無いので、聞き直す
            "${OTHER}",       // 環境変数は断る
            "db.example.com", // ホスト
            "",               // ポート
            "",               // ユーザー名
            "s3cret",
            "n",
        ]);
        set(&mut io, &store, "app@db", &path).unwrap();
        let stored =
            load_database_credentials(&store, "app@db", "db.example.com", 5432, "reader").unwrap();
        assert_eq!(stored.host, "db.example.com");
        assert!(io
            .shown
            .iter()
            .any(|s| s.starts_with("Could not expand [database]")));
        assert!(io
            .shown
            .iter()
            .any(|s| s == "Enter the value itself, not an environment variable."));
    }

    /// 既定がある（展開できた）ときも、`${...}`の答えは断る。空の答えは
    /// 今までどおり既定（PR #696のレビュー）
    #[test]
    fn test_set_refuses_a_variable_as_the_answer() {
        let (_dir, path) = config_in_temp(HAND_WRITTEN);
        let store = MemoryStore::default();
        let mut io = Script::new(&[
            "${OTHER}", // ホストに環境変数は断る
            "",         // 既定: db.example.com
            "",         // ポート
            "${USER}",  // ユーザー名に環境変数は断る
            "",         // 既定: reader
            "s3cret", "n",
        ]);
        set(&mut io, &store, "app@db", &path).unwrap();
        let stored =
            load_database_credentials(&store, "app@db", "db.example.com", 5432, "reader").unwrap();
        assert_eq!(stored.host, "db.example.com");
        assert_eq!(stored.username, "reader");
        assert_eq!(
            io.shown
                .iter()
                .filter(|s| *s == "Enter the value itself, not an environment variable.")
                .count(),
            2
        );
    }

    /// `[database]`が無い: `mcp-rs init`と同じ質問で作り、書き足す
    #[test]
    fn test_set_adds_a_section() {
        let without = "[server]\n\n[transport]\ntype = \"stdio\"\n\n[handlers]\n";
        let (_dir, path) = config_in_temp(without);
        let store = MemoryStore::default();
        let mut io = Script::new(&[
            "mysql", "", "", "shop", "app", "", // パスワードは空にできない
            "pw", "", // ssl_mode: prefer
            "", // 書き込みは許さない
            "", // 書き足す
        ]);
        set(&mut io, &store, "shop@localhost", &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with(without));
        let db = database_section(&content).unwrap().unwrap();
        assert_eq!(db.engine, "mysql");
        assert_eq!(db.keyring.as_deref(), Some("shop@localhost"));
        assert!(io.shown.iter().any(|s| s == "The password is empty."));
        // 名前は聞かない（引数の名前を使う）
        assert!(!io
            .shown
            .iter()
            .any(|s| s.contains("Name for this password")));
    }

    /// 設定ファイルが無い: 項目は作り、書く内容を表示する
    #[test]
    fn test_set_without_a_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let store = MemoryStore::default();
        let mut io = Script::new(&["", "", "", "app", "reader", "pw", "", ""]);
        set(&mut io, &store, "app@localhost", &path).unwrap();
        assert!(!path.exists());
        assert!(io.shown.iter().any(|s| s.contains("Run mcp-rs init")));
        assert!(io
            .shown
            .iter()
            .any(|s| s.contains("keyring = \"app@localhost\"")));
    }

    #[test]
    fn test_show_and_delete() {
        let store = MemoryStore::default();
        store_database_credentials(
            &store,
            "app@db",
            &DatabaseKeyringCredentials {
                host: "db.example.com".to_string(),
                port: 5432,
                username: "reader".to_string(),
                password: Zeroizing::new("s3cret".to_string()),
            },
        )
        .unwrap();
        let shown = show(&store, "app@db").unwrap();
        assert!(shown.contains("host: db.example.com"));
        assert!(shown.contains("port: 5432"));
        assert!(shown.contains("username: reader"));
        assert!(!shown.contains("s3cret"));

        // WordPressの資格情報は、見せず、消さない
        store_wordpress_credentials(
            &store,
            "site",
            &WordPressKeyringCredentials {
                site_url: "https://example.com".to_string(),
                username: "editor".to_string(),
                application_password: Zeroizing::new("app pass".to_string()),
            },
        )
        .unwrap();
        assert!(show(&store, "site")
            .unwrap_err()
            .contains("wordpress-keyring"));
        assert!(delete(&store, "site")
            .unwrap_err()
            .contains("wordpress-keyring"));
        assert!(database_entry(&store, "site").unwrap().is_some());

        assert!(delete(&store, "app@db").unwrap().contains("Deleted"));
        assert_eq!(database_entry(&store, "app@db").unwrap(), None);
        assert!(delete(&store, "app@db").is_err());
        assert!(show(&store, "missing").is_err());
    }
}
