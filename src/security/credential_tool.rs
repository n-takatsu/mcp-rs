//! `mcp-rs encrypt-wordpress-credentials`: generate the
//! `[handlers.wordpress.encrypted_credentials]` section of a config file
//! (#355).
//!
//! Before this existed, the only way to produce `encrypted_credentials` was
//! to write Rust against `SecureCredentials::encrypt`, so users of the
//! release binary or Docker image kept plaintext `password`s.
//!
//! Security properties:
//! - Secrets are never taken as command-line arguments (they would end up in
//!   shell history and process listings). On a terminal they are read with
//!   echo disabled; otherwise they are read as lines from standard input.
//! - Prompts and messages go to standard error. Standard output carries only
//!   the generated TOML, so it can be redirected or piped.
//! - The master password is never printed, and secrets are never included in
//!   error messages.
//! - The generated values are decrypted again before being printed, so a
//!   printed section is known to round-trip.

use std::io::{self, BufRead, IsTerminal, Write};

use serde::Serialize;
use zeroize::Zeroizing;

use super::encryption::{EncryptedCredentials, EncryptionError, SecureCredentials};
use secrecy::ExposeSecret;

/// The subcommand name, as passed as the first argument to `mcp-rs`.
pub const SUBCOMMAND: &str = "encrypt-wordpress-credentials";

/// Minimum master password length. The ciphertext ends up in a config file,
/// which an attacker who obtains it can brute-force offline, so a short
/// master password would defeat the encryption.
pub const MIN_MASTER_PASSWORD_LEN: usize = 12;

const HELP: &str = "\
Usage: mcp-rs encrypt-wordpress-credentials

Encrypt a WordPress Application Password and print the
[handlers.wordpress.encrypted_credentials] section to paste into the
mcp-rs config file. At startup, mcp-rs decrypts it with the master password
from the WORDPRESS_MASTER_PASSWORD environment variable.

Run it in a terminal and it asks for:
  1. the WordPress username
  2. the Application Password (input hidden)
  3. a master password, twice (input hidden, at least 12 characters)

Without a terminal (standard input redirected), it reads those three values
as three lines from standard input instead, with no confirmation:
  mcp-rs encrypt-wordpress-credentials < secrets.txt > encrypted.toml

Secrets are never accepted as command-line arguments. Prompts go to standard
error; only the generated TOML goes to standard output.";

/// The values needed to produce `encrypted_credentials`. The secret fields
/// are wiped from memory when dropped.
pub struct CredentialInput {
    pub username: String,
    pub application_password: Zeroizing<String>,
    pub master_password: Zeroizing<String>,
}

/// Run the subcommand. `args` are the arguments after the subcommand name.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        None => {}
        Some("-h") | Some("--help") => {
            println!("{HELP}");
            return Ok(());
        }
        // The argument is deliberately not echoed back: if it is a password
        // passed by mistake, printing it would expose it once more.
        Some(_) => {
            return Err(format!(
                "{SUBCOMMAND} takes no arguments. Secrets are read from the terminal or standard input, never from arguments. See `mcp-rs {SUBCOMMAND} --help`."
            )
            .into());
        }
    }

    let input = if io::stdin().is_terminal() {
        read_interactive()?
    } else {
        read_lines(io::stdin().lock())?
    };

    let toml = encrypted_credentials_toml(&input)?;

    let mut stdout = io::stdout().lock();
    stdout.write_all(toml.as_bytes())?;
    stdout.flush()?;

    eprintln!();
    eprintln!("Paste the section above into the config file, after [handlers.wordpress], and remove any plaintext `password`.");
    eprintln!("Then start mcp-rs with the master password in the WORDPRESS_MASTER_PASSWORD environment variable.");
    Ok(())
}

/// Encrypt the credentials and render the TOML section to print.
///
/// Validates the input, encrypts it, and decrypts the result again to make
/// sure what gets printed is what `WordPressConfig::resolve_encrypted_credentials`
/// will be able to read back at startup.
pub fn encrypted_credentials_toml(input: &CredentialInput) -> Result<String, EncryptionError> {
    validate(input)?;

    let credentials = SecureCredentials::new(
        input.username.clone(),
        input.application_password.as_str().to_string(),
    );
    let encrypted = credentials.encrypt(&input.master_password)?;

    let decrypted = SecureCredentials::from_encrypted(&encrypted, &input.master_password)?;
    if decrypted.username != input.username
        || decrypted.get_password().expose_secret() != input.application_password.as_str()
    {
        return Err(EncryptionError::EncryptionFailed(
            "the generated encrypted_credentials did not decrypt back to the input".to_string(),
        ));
    }

    render_toml(encrypted)
}

fn validate(input: &CredentialInput) -> Result<(), EncryptionError> {
    if input.username.is_empty() {
        return Err(EncryptionError::InvalidInput(
            "the WordPress username is empty".to_string(),
        ));
    }
    if input.application_password.is_empty() {
        return Err(EncryptionError::InvalidInput(
            "the Application Password is empty".to_string(),
        ));
    }
    if input.master_password.chars().count() < MIN_MASTER_PASSWORD_LEN {
        return Err(EncryptionError::InvalidInput(format!(
            "the master password must be at least {MIN_MASTER_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

/// Nest the credentials under `handlers.wordpress` so the output starts with
/// the `[handlers.wordpress.encrypted_credentials]` header, with every value
/// escaped by the TOML serializer.
fn render_toml(encrypted: EncryptedCredentials) -> Result<String, EncryptionError> {
    #[derive(Serialize)]
    struct Root {
        handlers: Handlers,
    }
    #[derive(Serialize)]
    struct Handlers {
        wordpress: WordPress,
    }
    #[derive(Serialize)]
    struct WordPress {
        encrypted_credentials: EncryptedCredentials,
    }

    toml::to_string(&Root {
        handlers: Handlers {
            wordpress: WordPress {
                encrypted_credentials: encrypted,
            },
        },
    })
    .map_err(|e| EncryptionError::EncryptionFailed(format!("TOML serialization failed: {e}")))
}

/// Non-interactive input: username, Application Password and master password
/// as the first three lines. Only the line ending is stripped from the
/// master password, since it must match `WORDPRESS_MASTER_PASSWORD` exactly.
pub fn read_lines(mut reader: impl BufRead) -> Result<CredentialInput, Box<dyn std::error::Error>> {
    let mut next_line = |what: &str| -> Result<Zeroizing<String>, Box<dyn std::error::Error>> {
        let mut line = Zeroizing::new(String::new());
        if reader.read_line(&mut line)? == 0 {
            return Err(format!(
                "standard input ended before the {what}. Provide three lines: username, Application Password, master password"
            )
            .into());
        }
        let trimmed_len = line.trim_end_matches(['\r', '\n']).len();
        line.truncate(trimmed_len);
        Ok(line)
    };

    let username = next_line("username")?.trim().to_string();
    let application_password =
        Zeroizing::new(next_line("Application Password")?.trim().to_string());
    let master_password = next_line("master password")?;

    Ok(CredentialInput {
        username,
        application_password,
        master_password,
    })
}

fn read_interactive() -> Result<CredentialInput, Box<dyn std::error::Error>> {
    eprintln!("Encrypt a WordPress Application Password for the mcp-rs config file.");
    eprintln!("Passwords are not shown while typing.");
    eprintln!();

    eprint!("WordPress username: ");
    io::stderr().flush()?;
    let mut username = String::new();
    io::stdin().lock().read_line(&mut username)?;
    let username = username.trim().to_string();

    let application_password =
        Zeroizing::new(read_hidden("Application Password: ")?.trim().to_string());

    eprintln!("Master password (at least {MIN_MASTER_PASSWORD_LEN} characters; set it as WORDPRESS_MASTER_PASSWORD when starting mcp-rs).");
    let master_password = read_hidden("Master password: ")?;
    let confirmation = read_hidden("Master password (again): ")?;
    if *master_password != *confirmation {
        return Err("the master passwords did not match".into());
    }

    Ok(CredentialInput {
        username,
        application_password,
        master_password,
    })
}

/// Restores the terminal even if reading fails or panics.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Read a line from the terminal without echoing it.
pub(crate) fn read_hidden(prompt: &str) -> io::Result<Zeroizing<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    eprint!("{prompt}");
    io::stderr().flush()?;

    let mut value = Zeroizing::new(String::new());
    let result = {
        crossterm::terminal::enable_raw_mode()?;
        let _guard = RawModeGuard;
        loop {
            match event::read()? {
                // Windows reports key releases too; the Enter release from
                // launching the command would otherwise end the input early.
                Event::Key(key) if key.kind == KeyEventKind::Release => {}
                Event::Key(key) => match key.code {
                    KeyCode::Enter => break Ok(()),
                    KeyCode::Esc => break Err(io::ErrorKind::Interrupted),
                    KeyCode::Char('c') | KeyCode::Char('d')
                        if key.modifiers == KeyModifiers::CONTROL =>
                    {
                        // Raw mode turns off the terminal's own Ctrl+C handling.
                        break Err(io::ErrorKind::Interrupted);
                    }
                    KeyCode::Backspace => {
                        value.pop();
                    }
                    // Ignore other Ctrl shortcuts, but keep AltGr characters,
                    // which some keyboards report as Ctrl+Alt.
                    KeyCode::Char(_)
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) => {}
                    KeyCode::Char(c) => value.push(c),
                    _ => {}
                },
                Event::Paste(text) => value.push_str(&text),
                _ => {}
            }
        }
    };
    eprintln!();

    match result {
        Ok(()) => Ok(value),
        Err(kind) => Err(io::Error::new(kind, "input cancelled")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{WordPressConfig, WORDPRESS_MASTER_PASSWORD_ENV_LOCK};

    fn input(username: &str, app_password: &str, master: &str) -> CredentialInput {
        CredentialInput {
            username: username.to_string(),
            application_password: Zeroizing::new(app_password.to_string()),
            master_password: Zeroizing::new(master.to_string()),
        }
    }

    /// The printed section, pasted after `[handlers.wordpress]`, must load
    /// through the real startup path (`resolve_encrypted_credentials`).
    #[test]
    fn test_generated_section_loads_through_resolve_encrypted_credentials() {
        let _env_guard = WORDPRESS_MASTER_PASSWORD_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let master = "correct horse battery staple";
        let generated = encrypted_credentials_toml(&input(
            "editor\"quote",
            "abcd EFGH 1234 ijkl MNOP 5678",
            master,
        ))
        .unwrap();
        assert!(generated.starts_with("[handlers.wordpress.encrypted_credentials]"));
        assert!(!generated.contains(master));
        assert!(!generated.contains("abcd EFGH"));

        let config_file = format!(
            "[handlers.wordpress]\nurl = \"https://example.com\"\nenabled = true\n\n{generated}"
        );
        let table: toml::Table = config_file.parse().unwrap();
        let mut wordpress: WordPressConfig =
            table["handlers"]["wordpress"].clone().try_into().unwrap();

        unsafe {
            std::env::set_var("WORDPRESS_MASTER_PASSWORD", master);
        }
        let resolved = wordpress.resolve_encrypted_credentials();
        unsafe {
            std::env::remove_var("WORDPRESS_MASTER_PASSWORD");
        }
        resolved.unwrap();

        assert_eq!(wordpress.username, "editor\"quote");
        assert_eq!(wordpress.password, "abcd EFGH 1234 ijkl MNOP 5678");
    }

    #[test]
    fn test_rejects_invalid_input_without_echoing_secrets() {
        let long_master = "long enough master password";
        assert!(encrypted_credentials_toml(&input("", "app-password", long_master)).is_err());
        assert!(encrypted_credentials_toml(&input("editor", "", long_master)).is_err());

        let short = "short-pw";
        let err = encrypted_credentials_toml(&input("editor", "app-password", short))
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains(&MIN_MASTER_PASSWORD_LEN.to_string()));
        assert!(!err.contains(short));
        assert!(!err.contains("app-password"));
    }

    #[test]
    fn test_read_lines() {
        let parsed =
            read_lines("  editor  \r\nabcd efgh ijkl \r\n master with spaces \n".as_bytes())
                .unwrap();
        assert_eq!(parsed.username, "editor");
        assert_eq!(parsed.application_password.as_str(), "abcd efgh ijkl");
        // Only the line ending is removed: it must match the env var exactly.
        assert_eq!(parsed.master_password.as_str(), " master with spaces ");

        // No trailing newline on the last line is fine.
        let parsed = read_lines("editor\npw\nmaster-password-123".as_bytes()).unwrap();
        assert_eq!(parsed.master_password.as_str(), "master-password-123");

        let err = read_lines("editor\npw\n".as_bytes())
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("master password"));
    }

    #[test]
    fn test_run_rejects_arguments() {
        let err = run(&["my-secret-password".to_string()])
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("takes no arguments"));
        assert!(!err.contains("my-secret-password"));
    }
}
