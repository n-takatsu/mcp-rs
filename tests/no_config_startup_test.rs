//! 設定ファイルが無くても、STDIOモードで`mcp_rs_status`だけで起動する（#667）。
//! 実行ファイルとして動かして確かめる

use std::io::Write;
use std::process::{Command, Stdio};

/// ホームフォルダ・今のフォルダを空のフォルダにした`mcp-rs`
fn mcp_rs(home: &std::path::Path, cwd: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mcp-rs"));
    command
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_STATE_HOME", home.join("state"))
        // 新しい版の問い合わせ（#635）をGitHubに届けない
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "");
    for name in [
        "MCP_RS_CONFIG",
        "MCP_RS_MCPB",
        "WORDPRESS_URL",
        "MCP_WORDPRESS_URL",
        "MCP_SERVER_BIND_ADDR",
        "MCP_TRANSPORT_TYPE",
    ] {
        command.env_remove(name);
    }
    command
}

const REQUESTS: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
    "\n",
    r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    "\n",
    r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"mcp_rs_status","arguments":{}}}"#,
    "\n",
);

#[test]
fn starts_without_config_file_in_stdio_mode() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    // 今のフォルダの、以前の名前の設定ファイルは読まない（#541）
    std::fs::write(
        cwd.path().join("config.toml"),
        "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://cwd.example.com\"\nusername = \"u\"\npassword = \"p\"\nenabled = true\n",
    )
    .unwrap();

    let mut child = mcp_rs(home.path(), cwd.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(REQUESTS.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");

    let responses: Vec<serde_json::Value> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 3, "{stdout}");
    assert!(responses[0]["result"]["protocolVersion"].is_string());

    // ツールは`mcp_rs_status`だけ（WordPressのツールは無い）
    let tools: Vec<&str> = responses[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(tools, ["mcp_rs_status"], "{stdout}");

    // 設定ファイルが無いことと、直し方
    let status = responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(
        status.contains("Config file: none. no config file"),
        "{status}"
    );
    assert!(status.contains("mcp-rs init"), "{status}");
    assert!(status.contains("--config"), "{status}");
    assert!(
        status.contains("config.toml in the current folder is not read"),
        "{status}"
    );
    assert!(!status.contains("cwd.example.com"), "{status}");

    // 標準エラー（ログ）にも警告を出す
    assert!(stderr.contains("no config file"), "{stderr}");
    assert!(
        stderr.contains("only the mcp_rs_status tool is available"),
        "{stderr}"
    );
}

#[test]
fn missing_config_option_file_is_still_an_error() {
    let home = tempfile::tempdir().unwrap();
    let missing = home.path().join("missing.toml");
    let output = mcp_rs(home.path(), home.path())
        .arg("--config")
        .arg(&missing)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_ne!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
}

#[test]
fn check_without_config_file_is_still_an_error() {
    let home = tempfile::tempdir().unwrap();
    let output = mcp_rs(home.path(), home.path())
        .args(["check", "--offline"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("no config file"), "{stdout}");
    assert!(stdout.contains("mcp-rs init"), "{stdout}");
}
