//! `mcp-rs wordpress-keyring`: register, check and delete WordPress
//! credentials in the OS credential store (#533).
//!
//! Like `encrypt-wordpress-credentials`, secrets are never taken as
//! command-line arguments: on a terminal they are read with echo disabled,
//! otherwise as lines from standard input. Only the entry name, which is not
//! secret, is an argument.

use std::io::{self, BufRead, IsTerminal, Write};

use zeroize::Zeroizing;

use super::credential_tool::read_hidden;
use super::keyring_store::{
    delete_wordpress_credentials, load_wordpress_credentials, store_wordpress_credentials,
    OsKeyring, SecretStore, WordPressKeyringCredentials, KEYRING_SERVICE,
};

/// The subcommand name, as passed as the first argument to `mcp-rs`.
pub const SUBCOMMAND: &str = "wordpress-keyring";

const HELP: &str = "\
Usage:
  mcp-rs wordpress-keyring set <name>
  mcp-rs wordpress-keyring show <name>
  mcp-rs wordpress-keyring delete <name>

Keep the WordPress username and Application Password in the OS credential
store (Windows Credential Manager, macOS Keychain, Secret Service on Linux),
so mcp-rs reads them the same way whether Claude Desktop, Claude Code or a
terminal starts it. No environment variables are needed.

  set     Register or replace the credentials named <name>. On a terminal it
          asks for the site URL, the username and the Application Password
          (input hidden). Without a terminal it reads them as three lines
          from standard input.
  show    Print the site URL and the username stored under <name> (never
          the password).
  delete  Delete the credentials named <name>.

<name> is any label, for example the site's host name. Refer to it from the
config file:

  [handlers.wordpress]
  url = \"https://example.com\"
  enabled = true
  keyring = \"example.com\"

Leave username, password and encrypted_credentials out of that section.
The credentials are used only when url has the same scheme, host and port
as the site URL stored with them, so they are not sent to another site.
Secrets are never accepted as command-line arguments.";

/// What the arguments ask for.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Help,
    Set(String),
    Show(String),
    Delete(String),
}

fn parse(args: &[String]) -> Result<Action, String> {
    let usage = format!("See `mcp-rs {SUBCOMMAND} --help`.");
    match args {
        [] => Err(format!("{SUBCOMMAND} needs set, show or delete. {usage}")),
        [flag, ..] if flag == "-h" || flag == "--help" => Ok(Action::Help),
        [action, name] => {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(format!("the entry name is empty. {usage}"));
            }
            match action.as_str() {
                "set" => Ok(Action::Set(name)),
                "show" => Ok(Action::Show(name)),
                "delete" => Ok(Action::Delete(name)),
                other => Err(format!(
                    "unknown action '{other}' (use set, show or delete). {usage}"
                )),
            }
        }
        [action] if matches!(action.as_str(), "set" | "show" | "delete") => {
            Err(format!("{action} needs an entry name. {usage}"))
        }
        // The arguments are not echoed back: a password passed by mistake
        // would otherwise be printed once more.
        _ => Err(format!(
            "{SUBCOMMAND} takes an action and an entry name only. Secrets are read from the terminal or standard input, never from arguments. {usage}"
        )),
    }
}

/// Run the subcommand. `args` are the arguments after the subcommand name.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    run_with(args, &OsKeyring)
}

fn run_with(args: &[String], store: &dyn SecretStore) -> Result<(), Box<dyn std::error::Error>> {
    match parse(args)? {
        Action::Help => println!("{HELP}"),
        Action::Set(name) => {
            let credentials = if io::stdin().is_terminal() {
                read_interactive(&name)?
            } else {
                read_lines(io::stdin().lock())?
            };
            store_wordpress_credentials(store, &name, &credentials)?;
            let stored = load_wordpress_credentials(store, &name)?;
            eprintln!(
                "Stored the credentials for '{}' on {} as '{name}' in the OS credential store (service '{KEYRING_SERVICE}').",
                stored.username, stored.site_url
            );
            eprintln!("In the config file, write this under [handlers.wordpress] and remove username, password and encrypted_credentials:");
            eprintln!("  keyring = \"{name}\"");
        }
        Action::Show(name) => {
            let credentials = load_wordpress_credentials(store, &name)?;
            if credentials.site_url.is_empty() {
                println!(
                    "site: (not stored: register again with `mcp-rs {SUBCOMMAND} set {name}`)"
                );
            } else {
                println!("site: {}", credentials.site_url);
            }
            println!("username: {}", credentials.username);
            println!("password: (stored, not shown)");
        }
        Action::Delete(name) => {
            if delete_wordpress_credentials(store, &name)? {
                eprintln!("Deleted the credentials named '{name}'.");
            } else {
                return Err(format!("no credentials named '{name}' to delete").into());
            }
        }
    }
    Ok(())
}

/// Non-interactive input: the site URL, the username and the Application
/// Password as the first three lines.
fn read_lines(
    mut reader: impl BufRead,
) -> Result<WordPressKeyringCredentials, Box<dyn std::error::Error>> {
    let mut next_line = |what: &str| -> Result<Zeroizing<String>, Box<dyn std::error::Error>> {
        let mut line = Zeroizing::new(String::new());
        if reader.read_line(&mut line)? == 0 {
            return Err(format!(
                "standard input ended before the {what}. Provide three lines: site URL, username, Application Password"
            )
            .into());
        }
        Ok(Zeroizing::new(line.trim().to_string()))
    };
    let site_url = next_line("site URL")?.to_string();
    let username = next_line("username")?.to_string();
    let application_password = next_line("Application Password")?;
    check_input(site_url, username, application_password)
}

fn read_interactive(name: &str) -> Result<WordPressKeyringCredentials, Box<dyn std::error::Error>> {
    eprintln!("Store WordPress credentials as '{name}' in the OS credential store.");
    eprintln!("The Application Password is not shown while typing.");
    eprintln!();

    eprint!("WordPress site URL (as in url of the config file, e.g. https://example.com): ");
    io::stderr().flush()?;
    let mut site_url = String::new();
    io::stdin().lock().read_line(&mut site_url)?;
    eprint!("WordPress username: ");
    io::stderr().flush()?;
    let mut username = String::new();
    io::stdin().lock().read_line(&mut username)?;
    let application_password =
        Zeroizing::new(read_hidden("Application Password: ")?.trim().to_string());
    check_input(
        site_url.trim().to_string(),
        username.trim().to_string(),
        application_password,
    )
}

fn check_input(
    site_url: String,
    username: String,
    application_password: Zeroizing<String>,
) -> Result<WordPressKeyringCredentials, Box<dyn std::error::Error>> {
    // 入力は文に入れない。2行の形のまま渡すと、1行目はユーザー名で、
    // 取り違えるとパスワードが表示される（PR #551のレビュー）
    if super::keyring_store::site_origin(&site_url).is_none() {
        return Err(
            "the first line (the site URL) is not a URL starting with https:// or http://. Provide three lines: site URL, username, Application Password"
                .into(),
        );
    }
    if username.is_empty() {
        return Err("the WordPress username is empty".into());
    }
    if application_password.is_empty() {
        return Err("the Application Password is empty".into());
    }
    Ok(WordPressKeyringCredentials {
        site_url,
        username,
        application_password,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::keyring_store::MemoryStore;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_parse() {
        assert_eq!(parse(&args(&["--help"])), Ok(Action::Help));
        assert_eq!(
            parse(&args(&["set", " example.com "])),
            Ok(Action::Set("example.com".to_string()))
        );
        assert_eq!(
            parse(&args(&["show", "a"])),
            Ok(Action::Show("a".to_string()))
        );
        assert_eq!(
            parse(&args(&["delete", "a"])),
            Ok(Action::Delete("a".to_string()))
        );
        assert!(parse(&[]).unwrap_err().contains("set, show or delete"));
        assert!(parse(&args(&["set"])).unwrap_err().contains("entry name"));
        assert!(parse(&args(&["set", " "])).unwrap_err().contains("empty"));
        assert!(parse(&args(&["get", "a"]))
            .unwrap_err()
            .contains("unknown action"));
    }

    #[test]
    fn test_parse_does_not_echo_extra_arguments() {
        let err = parse(&args(&["set", "a", "my-secret-password"])).unwrap_err();
        assert!(err.contains("never from arguments"));
        assert!(!err.contains("my-secret-password"));
    }

    #[test]
    fn test_read_lines() {
        let parsed =
            read_lines(" https://example.com \r\n  editor \r\n abcd EFGH 1234 \r\n".as_bytes())
                .unwrap();
        assert_eq!(parsed.site_url, "https://example.com");
        assert_eq!(parsed.username, "editor");
        assert_eq!(parsed.application_password.as_str(), "abcd EFGH 1234");

        let err = read_lines("https://example.com\neditor\n".as_bytes())
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("Application Password"));
        let err = read_lines("https://example.com\neditor\n\n".as_bytes())
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("empty"));
        // 1行目がURLでなければ止める（以前の2行の形で渡したとき等）。
        // 入力は文に出さない
        let err = read_lines("secret-password\neditor\npw\n".as_bytes())
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("the first line (the site URL)"), "{err}");
        assert!(!err.contains("secret-password"), "{err}");
    }

    #[test]
    fn test_show_and_delete() {
        let store = MemoryStore::default();
        assert!(run_with(&args(&["show", "a"]), &store).is_err());
        assert!(run_with(&args(&["delete", "a"]), &store).is_err());

        store_wordpress_credentials(
            &store,
            "a",
            &WordPressKeyringCredentials {
                site_url: "https://example.com".to_string(),
                username: "editor".to_string(),
                application_password: Zeroizing::new("pw".to_string()),
            },
        )
        .unwrap();
        run_with(&args(&["show", "a"]), &store).unwrap();
        run_with(&args(&["delete", "a"]), &store).unwrap();
        assert!(store.get("a").unwrap().is_none());
    }
}
