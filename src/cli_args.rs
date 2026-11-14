//! `mcp-rs`の実行ファイルの引数（#431）
//!
//! `encrypt-wordpress-credentials`・`wordpress-keyring`サブコマンドは、別に
//! `security::credential_tool`・`security::keyring_tool`が処理する。

/// 引数から決めた動作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `--version`・`-V`: バージョンを表示して終わる
    Version,
    /// `--help`・`-h`: 使い方を表示して終わる
    Help,
    /// サーバーとして起動する
    Run(RunArgs),
    /// 引数の誤り（知らない引数・パスの無い`--config`、#437）。理由を表示
    /// して、起動せずに終わる
    Invalid(String),
}

/// サーバーとして起動するときの引数
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunArgs {
    /// `--config <path>`（`--config=<path>`も可）
    pub config: Option<String>,
}

/// 引数（先頭はプログラム名）を解釈する。`--version`・`--help`は、他の
/// 引数より優先する（最初に現れたもの）。知らない引数・パスの無い
/// `--config`は`Invalid`にする（打ち間違いに気づかず、別の設定で起動する
/// のを防ぐ、#437）
pub fn parse_args(args: &[String]) -> Command {
    let mut run = RunArgs::default();
    let mut problems = Vec::new();
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--version" | "-V" => return Command::Version,
            "--help" | "-h" => return Command::Help,
            "--config" => match rest.next() {
                Some(path) => run.config = Some(path.clone()),
                None => problems.push("--config needs a path".to_string()),
            },
            _ => match arg.strip_prefix("--config=") {
                Some("") => problems.push("--config needs a path".to_string()),
                Some(path) => run.config = Some(path.to_string()),
                None => problems.push(format!("unknown argument '{}'", arg)),
            },
        }
    }
    if problems.is_empty() {
        Command::Run(run)
    } else {
        Command::Invalid(problems.join(", "))
    }
}

/// `--version`の表示
pub fn version_text() -> String {
    format!("mcp-rs {}", env!("CARGO_PKG_VERSION"))
}

/// `--help`の表示
pub fn help_text() -> String {
    format!(
        "{version}
MCP server for WordPress and other tools (STDIO or HTTP mode)

Usage:
  mcp-rs [--config <path>]
  mcp-rs {keyring} set|show|delete <name>
  mcp-rs {credentials}
  mcp-rs --version
  mcp-rs --help

Options:
  --config <path>  Config file to use. Without it, mcp-rs reads
                   ~/.config/mcp-rs/config.toml only. Config files in the
                   current folder are not read without --config.
                   With --config, the WordPress credentials come only from
                   that file (keyring, ${{VAR}}, encrypted_credentials or
                   plain values); WORDPRESS_URL, WORDPRESS_USERNAME and
                   WORDPRESS_PASSWORD are not read on their own
  -V, --version    Print the version and exit
  -h, --help       Print this help and exit

Commands:
  {keyring} set|show|delete <name>
                   Keep the WordPress username and Application Password in
                   the OS credential store, for `keyring = \"<name>\"` in the
                   config file (run with --help for details)
  {credentials}
                   Encrypt a WordPress Application Password for the config
                   file (run with --help for details)
",
        version = version_text(),
        credentials = crate::security::credential_tool::SUBCOMMAND,
        keyring = crate::security::keyring_tool::SUBCOMMAND,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Command {
        let args: Vec<String> = std::iter::once("mcp-rs")
            .chain(args.iter().copied())
            .map(str::to_string)
            .collect();
        parse_args(&args)
    }

    #[test]
    fn test_version_and_help() {
        assert_eq!(parse(&["--version"]), Command::Version);
        assert_eq!(parse(&["-V"]), Command::Version);
        assert_eq!(parse(&["--help"]), Command::Help);
        assert_eq!(parse(&["-h"]), Command::Help);
        // 他の引数があっても、表示して終わる
        assert_eq!(
            parse(&["--config", "a.toml", "--version"]),
            Command::Version
        );
        assert_eq!(parse(&["--help", "--version"]), Command::Help);
    }

    #[test]
    fn test_run_args() {
        assert_eq!(parse(&[]), Command::Run(RunArgs::default()));
        assert_eq!(
            parse(&["--config", "C:/x/mcp-config.toml"]),
            Command::Run(RunArgs {
                config: Some("C:/x/mcp-config.toml".to_string()),
            })
        );
        assert_eq!(
            parse(&["--config=a.toml"]),
            Command::Run(RunArgs {
                config: Some("a.toml".to_string()),
            })
        );
    }

    #[test]
    fn test_invalid_args() {
        assert_eq!(
            parse(&["--confg", "a.toml"]),
            Command::Invalid("unknown argument '--confg', unknown argument 'a.toml'".to_string())
        );
        assert_eq!(
            parse(&["--config"]),
            Command::Invalid("--config needs a path".to_string())
        );
        assert_eq!(
            parse(&["--config="]),
            Command::Invalid("--config needs a path".to_string())
        );
        assert_eq!(
            parse(&["--config", "a.toml", "--verbose"]),
            Command::Invalid("unknown argument '--verbose'".to_string())
        );
        // --version・--helpは、誤りがあっても優先する
        assert_eq!(parse(&["--verbose", "--version"]), Command::Version);
        assert_eq!(parse(&["--confg", "--help"]), Command::Help);
    }

    #[test]
    fn test_texts() {
        assert_eq!(
            version_text(),
            format!("mcp-rs {}", env!("CARGO_PKG_VERSION"))
        );
        let help = help_text();
        assert!(help.contains("--config <path>"));
        assert!(help.contains("encrypt-wordpress-credentials"));
        assert!(help.contains("wordpress-keyring set|show|delete <name>"));
        assert!(help.contains("${VAR}"));
        assert!(help.contains("--version"));
    }
}
