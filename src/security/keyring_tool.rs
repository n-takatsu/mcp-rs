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
          asks for the username and the Application Password (input hidden).
          Without a terminal it reads them as two lines from standard input.
  show    Print the username stored under <name> (never the password).
  delete  Delete the credentials named <name>.

<name> is any label, for example the site's host name. Refer to it from the
config file:

  [handlers.wordpress]
  url = \"https://example.com\"
  enabled = true
  keyring = \"example.com\"

Leave username, password and encrypted_credentials out of that section.
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
            eprintln!(
                "Stored the credentials for '{}' as '{name}' in the OS credential store (service '{KEYRING_SERVICE}').",
                credentials.username
            );
            eprintln!("In the config file, write this under [handlers.wordpress] and remove username, password and encrypted_credentials:");
            eprintln!("  keyring = \"{name}\"");
        }
        Action::Show(name) => {
            let credentials = load_wordpress_credentials(store, &name)?;
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

/// Non-interactive input: the username and the Application Password as the
/// first two lines.
fn read_lines(
    mut reader: impl BufRead,
) -> Result<WordPressKeyringCredentials, Box<dyn std::error::Error>> {
    let mut next_line = |what: &str| -> Result<Zeroizing<String>, Box<dyn std::error::Error>> {
        let mut line = Zeroizing::new(String::new());
        if reader.read_line(&mut line)? == 0 {
            return Err(format!(
                "standard input ended before the {what}. Provide two lines: username, Application Password"
            )
            .into());
        }
        Ok(Zeroizing::new(line.trim().to_string()))
    };
    let username = next_line("username")?.to_string();
    let application_password = next_line("Application Password")?;
    check_input(username, application_password)
}

fn read_interactive(name: &str) -> Result<WordPressKeyringCredentials, Box<dyn std::error::Error>> {
    eprintln!("Store WordPress credentials as '{name}' in the OS credential store.");
    eprintln!("The Application Password is not shown while typing.");
    eprintln!();

    eprint!("WordPress username: ");
    io::stderr().flush()?;
    let mut username = String::new();
    io::stdin().lock().read_line(&mut username)?;
    let application_password =
        Zeroizing::new(read_hidden("Application Password: ")?.trim().to_string());
    check_input(username.trim().to_string(), application_password)
}

fn check_input(
    username: String,
    application_password: Zeroizing<String>,
) -> Result<WordPressKeyringCredentials, Box<dyn std::error::Error>> {
    if username.is_empty() {
        return Err("the WordPress username is empty".into());
    }
    if application_password.is_empty() {
        return Err("the Application Password is empty".into());
    }
    Ok(WordPressKeyringCredentials {
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
        let parsed = read_lines("  editor \r\n abcd EFGH 1234 \r\n".as_bytes()).unwrap();
        assert_eq!(parsed.username, "editor");
        assert_eq!(parsed.application_password.as_str(), "abcd EFGH 1234");

        let err = read_lines("editor\n".as_bytes()).err().unwrap().to_string();
        assert!(err.contains("Application Password"));
        let err = read_lines("editor\n\n".as_bytes())
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("empty"));
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
