//! `mcp-rs`の実行ファイルの引数（#431）
//!
//! `encrypt-wordpress-credentials`サブコマンドは、別に
//! `security::credential_tool`が処理する。

/// 引数から決めた動作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `--version`・`-V`: バージョンを表示して終わる
    Version,
    /// `--help`・`-h`: 使い方を表示して終わる
    Help,
    /// サーバーとして起動する
    Run(RunArgs),
}

/// サーバーとして起動するときの引数
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunArgs {
    /// `--config <path>`（`--config=<path>`も可）
    pub config: Option<String>,
    /// 知らない引数。今は警告だけ出して無視する（将来のバージョンでエラーに
    /// する予定）
    pub unknown: Vec<String>,
    /// `--config`の後ろにパスが無い
    pub missing_config_value: bool,
}

/// 引数（先頭はプログラム名）を解釈する。`--version`・`--help`は、他の
/// 引数より優先する（最初に現れたもの）
pub fn parse_args(args: &[String]) -> Command {
    let mut run = RunArgs::default();
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--version" | "-V" => return Command::Version,
            "--help" | "-h" => return Command::Help,
            "--config" => match rest.next() {
                Some(path) => run.config = Some(path.clone()),
                None => run.missing_config_value = true,
            },
            _ => match arg.strip_prefix("--config=") {
                Some("") => run.missing_config_value = true,
                Some(path) => run.config = Some(path.to_string()),
                None => run.unknown.push(arg.clone()),
            },
        }
    }
    Command::Run(run)
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
  mcp-rs {credentials}
  mcp-rs --version
  mcp-rs --help

Options:
  --config <path>  Config file to use. Without it, mcp-rs uses the first
                   of mcp-config.toml, config.toml, config/mcp.toml (in the
                   current folder) and ~/.config/mcp-rs/config.toml
  -V, --version    Print the version and exit
  -h, --help       Print this help and exit

Commands:
  {credentials}
                   Encrypt a WordPress Application Password for the config
                   file (run with --help for details)
",
        version = version_text(),
        credentials = crate::security::credential_tool::SUBCOMMAND,
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
                ..Default::default()
            })
        );
        assert_eq!(
            parse(&["--config=a.toml"]),
            Command::Run(RunArgs {
                config: Some("a.toml".to_string()),
                ..Default::default()
            })
        );
        assert_eq!(
            parse(&["--confg", "a.toml"]),
            Command::Run(RunArgs {
                unknown: vec!["--confg".to_string(), "a.toml".to_string()],
                ..Default::default()
            })
        );
        assert_eq!(
            parse(&["--config"]),
            Command::Run(RunArgs {
                missing_config_value: true,
                ..Default::default()
            })
        );
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
        assert!(help.contains("--version"));
    }
}
