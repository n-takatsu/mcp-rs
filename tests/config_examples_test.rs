//! `configs/`の設定ファイルの例が、今のmcp-rsで読め、読まれない（黙って
//! 無視される）キーが無いことを確かめる（#557）。
//!
//! mcp-rsは知らないキーを拒否しないので、古い例のキーは、エラーにならずに
//! 無視されていた（`[filesystem]`・`[security]`・`[logging]`等）。読んだ
//! 設定を書き戻し、元のファイルにあって書き戻しに無いキーを、読まれない
//! キーとみなす。
//!
//! 限界（PR #559のレビュー）:
//! - 中身が自由な表（`[[plugins]]`の`env`等）の下は、どんなキーも読んだ
//!   まま書き戻されるので、見つけられない
//! - 読み方は、`--config`で指定したファイルと同じ`toml::from_str`。
//!   `--config`無しの読み込み（`McpConfig::load`、`config`クレートと
//!   環境変数）とは道が違う

use mcp_rs::config::McpConfig;
use std::path::Path;

/// mcp-rsの設定ファイルではないもの（ライブラリの機能の設定の例、説明用の
/// 参考）。新しく足したファイルは、ここに無ければ確かめる
const NOT_MCP_RS_CONFIG: &[&str] = &[
    "configs/examples/mfa-config.toml",
    "configs/examples/waf-config.toml",
    "configs/security/network-policy.toml",
    "configs/threat-intelligence.toml",
];

/// 設定ファイルに書き写す一部分（`[database]`だけ等）。前に足して確かめる
const SNIPPETS: &[(&str, &str)] = &[
    (
        "configs/database/mariadb.toml",
        "[server]\n[transport]\n[handlers]\n",
    ),
    (
        "configs/database/mysql.toml",
        "[server]\n[transport]\n[handlers]\n",
    ),
    (
        "configs/database/postgres.toml",
        "[server]\n[transport]\n[handlers]\n",
    ),
    (
        "configs/database/mongodb.toml",
        "[server]\n[transport]\n[handlers]\n",
    ),
    (
        "configs/database/redis.toml",
        "[server]
[transport]
[handlers]
",
    ),
    (
        "configs/database/sqlite.toml",
        "[server]\n[transport]\n[handlers]\n",
    ),
];

fn toml_files(dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            toml_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "toml") {
            out.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// `value`のキーを、`a.b.c`・`a[0].b`の形で集める
fn key_paths(prefix: &str, value: &toml::Value, out: &mut Vec<String>) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.push(path.clone());
                key_paths(&path, value, out);
            }
        }
        toml::Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                key_paths(&format!("{prefix}[{i}]"), value, out);
            }
        }
        _ => {}
    }
}

/// 読まれないキー（親が読まれないなら、親だけ）
fn ignored_keys(text: &str) -> Result<Vec<String>, String> {
    let mut original: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    // `[transport]`の`type`は、`transport_type`の別名（書き戻すと名前が変わる）
    if let Some(transport) = original.get_mut("transport").and_then(|t| t.as_table_mut()) {
        if let Some(value) = transport.remove("type") {
            transport.insert("transport_type".to_string(), value);
        }
    }
    let config: McpConfig = toml::from_str(text).map_err(|e| e.message().to_string())?;
    let written = toml::Value::try_from(&config).map_err(|e| e.to_string())?;

    let (mut before, mut after) = (Vec::new(), Vec::new());
    key_paths("", &original, &mut before);
    key_paths("", &written, &mut after);
    let missing: Vec<String> = before.into_iter().filter(|k| !after.contains(k)).collect();
    Ok(missing
        .iter()
        .filter(|key| {
            !missing.iter().any(|parent| {
                key.starts_with(&format!("{parent}.")) || key.starts_with(&format!("{parent}["))
            })
        })
        .cloned()
        .collect())
}

#[test]
fn config_examples_load_without_ignored_keys() {
    let mut files = Vec::new();
    toml_files(Path::new("configs"), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no config files found");

    let mut problems = Vec::new();
    for file in &files {
        if NOT_MCP_RS_CONFIG.contains(&file.as_str()) {
            continue;
        }
        let mut text = std::fs::read_to_string(file).unwrap();
        if let Some((_, prefix)) = SNIPPETS.iter().find(|(name, _)| *name == file) {
            text = format!("{prefix}{text}");
        }
        match ignored_keys(&text) {
            Ok(keys) if keys.is_empty() => {}
            Ok(keys) => problems.push(format!("{file}: not read by mcp-rs: {keys:?}")),
            Err(e) => problems.push(format!("{file}: does not load: {e}")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn ignored_keys_are_found() {
    // このテストの確かめ方が、読まれないキーを見つけられること
    let text = "[server]\nlog_level = \"info\"\n[transport]\ntype = \"stdio\"\n[handlers]\n[filesystem]\nroot_path = \".\"\n[handlers.wordpress]\nenabled = true\nurl = \"https://example.com\"\nusername = \"u\"\npassword = \"p\"\ntimeout = 30\n";
    // 順番は、tomlの`preserve_order`で、ファイルに書いた順になる（#602）
    let mut keys = ignored_keys(text).unwrap();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "filesystem".to_string(),
            "handlers.wordpress.timeout".to_string()
        ]
    );
}
