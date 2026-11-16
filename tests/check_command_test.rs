//! `mcp-rs check`（#565）を、実行ファイルとして動かして確かめる。終了コード
//! （0・1・2）と、表示に秘密情報が出ないこと

use std::process::Command;

fn mcp_rs() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mcp-rs"))
}

/// 設定ファイルを書いて、`mcp-rs check --config <file> --offline`を動かす
fn check(text: &str) -> (Option<i32>, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp-config.toml");
    std::fs::write(&path, text).unwrap();
    let output = mcp_rs()
        .args(["check", "--offline", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn check_reports_toml_error_with_exit_code_1() {
    // `"`の位置がずれた設定（2026-11-15にVS Codeで起きた）
    let (code, stdout) = check(
        "[handlers.wordpress]\nurl = \"https://example.com\n\"keyring = \"example\"\nenabled = true\n",
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert!(
        stdout.contains("ERROR  config file: TOML parse error at line 3"),
        "{stdout}"
    );
    assert!(stdout.contains("1 error, 0 warnings."), "{stdout}");
}

#[test]
fn check_reports_missing_credentials() {
    // keyringの行が無い（2026-11-15にVS Codeで起きた）
    let (code, stdout) = check(
        "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://example.com\"\nenabled = true\n",
    );
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("ERROR  WordPress credentials:"), "{stdout}");
    assert!(stdout.contains("keyring"), "{stdout}");
}

#[test]
fn check_passes_with_warning_and_hides_secrets() {
    let (code, stdout) = check(
        "[server]\n[transport]\n[handlers.wordpress]\nurl = \"https://example.com\"\nusername = \"someone\"\npassword = \"plain-secret\"\nenabled = true\n[audit]\nenabled = false\n",
    );
    // 平文のパスワードは警告だけで、エラーではない
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("WARN   WordPress credentials:"), "{stdout}");
    assert!(stdout.contains("(not connected: --offline)"), "{stdout}");
    assert!(!stdout.contains("plain-secret"), "{stdout}");
    assert!(!stdout.contains("someone"), "{stdout}");
}

#[test]
fn check_rejects_unknown_arguments_with_exit_code_2() {
    let output = mcp_rs().args(["check", "--confg", "x"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown argument '--confg'"), "{stderr}");
    assert!(stderr.contains("Usage: mcp-rs check"), "{stderr}");
}

#[test]
fn check_help() {
    let output = mcp_rs().args(["check", "--help"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--offline"), "{stdout}");
}
