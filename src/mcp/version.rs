//! MCPのプロトコル版（#417）
//!
//! クライアントが`initialize`で要求した版に対応していればその版で、
//! していなければ対応する最新の版で答える。応答（ツールの一覧・結果）は、
//! 決まった版で定義された項目だけにする。

use serde_json::Value;

/// 対応する最新の版
pub const LATEST_PROTOCOL_VERSION: &str = "2025-11-25";

/// 対応する版（新しい順）
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &[LATEST_PROTOCOL_VERSION, "2025-06-18", "2024-11-05"];

/// `initialize`を受け取る前に使う版。古いクライアントにも読める応答にする
pub const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// クライアントが要求した版から、使う版を決める。
pub fn negotiate_protocol_version(requested: &str) -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|v| *v == requested)
        .unwrap_or(LATEST_PROTOCOL_VERSION)
}

/// mcp-rsのウェブサイト（`serverInfo`の`websiteUrl`）
const WEBSITE_URL: &str = "https://n-takatsu.github.io/mcp-rs/";

/// `initialize`の結果の`serverInfo`（Implementation）。`description`・
/// `websiteUrl`は2025-11-25で加わった項目なので、その版だけに付ける（#475）。
pub fn server_info_for_version(version: &str) -> Value {
    let mut info = serde_json::json!({
        "name": "mcp-rs",
        "version": env!("CARGO_PKG_VERSION")
    });
    if supports_2025_11_features(version) {
        info["description"] = Value::from(env!("CARGO_PKG_DESCRIPTION"));
        info["websiteUrl"] = Value::from(WEBSITE_URL);
    }
    info
}

/// `Implementation`の`description`・`websiteUrl`（2025-11-25）を使える版か。
fn supports_2025_11_features(version: &str) -> bool {
    !matches!(version, "2025-06-18" | "2024-11-05")
}

/// ツール定義の`title`・`outputSchema`（2025-06-18）、`annotations`
/// （2025-03-26）、ツールの結果の`structuredContent`・リソースリンク
/// （2025-06-18）、音声（2025-03-26）を使える版か。
fn supports_2025_features(version: &str) -> bool {
    version != "2024-11-05"
}

/// `tools/list`の結果を、`version`で定義された項目だけにする。
pub fn tools_list_for_version(version: &str, mut result: Value) -> Value {
    if supports_2025_features(version) {
        return result;
    }
    if let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            if let Value::Object(fields) = tool {
                fields.remove("title");
                fields.remove("outputSchema");
                fields.remove("annotations");
            }
        }
    }
    result
}

/// `tools/call`の結果（CallToolResult）を、`version`で定義された項目だけに
/// する。2024-11-05では`structuredContent`を除き、その版に無い種類の
/// コンテンツ（リソースリンク・音声）をテキストに置き換える。同じ内容は
/// `content`にもあるため（仕様上、`structuredContent`を返すツールは同じ
/// 内容を`content`にも入れる）、情報は失われない。
pub fn call_tool_result_for_version(version: &str, mut result: Value) -> Value {
    if supports_2025_features(version) {
        return result;
    }
    let Value::Object(fields) = &mut result else {
        return result;
    };
    fields.remove("structuredContent");
    if let Some(Value::Array(content)) = fields.get_mut("content") {
        for item in content.iter_mut() {
            if let Some(text) = content_as_text(item) {
                *item = serde_json::json!({ "type": "text", "text": text });
            }
        }
    }
    result
}

/// 2024-11-05に無い種類のコンテンツを、テキストで表す。その版にある種類
/// なら`None`。
fn content_as_text(item: &Value) -> Option<String> {
    let str_field = |name: &str| item.get(name).and_then(Value::as_str);
    match str_field("type")? {
        "resource_link" => {
            let uri = str_field("uri").unwrap_or("");
            let name = str_field("title").or(str_field("name")).unwrap_or(uri);
            let mut text = format!("Resource: {} ({})", name, uri);
            if let Some(description) = str_field("description") {
                text.push_str(&format!("\n{}", description));
            }
            Some(text)
        }
        "audio" => Some(format!(
            "[audio content ({}) omitted: not supported by protocol version 2024-11-05]",
            str_field("mimeType").unwrap_or("unknown type")
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_negotiate_protocol_version() {
        assert_eq!(negotiate_protocol_version("2025-11-25"), "2025-11-25");
        assert_eq!(negotiate_protocol_version("2025-06-18"), "2025-06-18");
        assert_eq!(negotiate_protocol_version("2024-11-05"), "2024-11-05");
        // 対応していない版には、対応する最新の版で答える
        assert_eq!(negotiate_protocol_version("2025-03-26"), "2025-11-25");
        assert_eq!(negotiate_protocol_version("2099-01-01"), "2025-11-25");
        assert_eq!(negotiate_protocol_version(""), "2025-11-25");
    }

    #[test]
    fn test_server_info_for_version() {
        let latest = server_info_for_version("2025-11-25");
        assert_eq!(latest["name"], json!("mcp-rs"));
        assert_eq!(latest["version"], json!(env!("CARGO_PKG_VERSION")));
        assert!(latest["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()));
        assert_eq!(latest["websiteUrl"], json!(WEBSITE_URL));

        // 古い版には、その版で定義された項目（name・version）だけ
        for old in ["2025-06-18", "2024-11-05"] {
            assert_eq!(
                server_info_for_version(old),
                json!({ "name": "mcp-rs", "version": env!("CARGO_PKG_VERSION") }),
                "version {}",
                old
            );
        }
    }

    #[test]
    fn test_tools_list_for_version() {
        let result = json!({ "tools": [{
            "name": "t",
            "title": "T",
            "description": "d",
            "inputSchema": { "type": "object" },
            "outputSchema": { "type": "object" },
            "annotations": { "readOnlyHint": true }
        }]});
        assert_eq!(tools_list_for_version("2025-11-25", result.clone()), result);
        assert_eq!(tools_list_for_version("2025-06-18", result.clone()), result);
        let old = tools_list_for_version("2024-11-05", result);
        assert_eq!(
            old,
            json!({ "tools": [{
                "name": "t",
                "description": "d",
                "inputSchema": { "type": "object" }
            }]})
        );
    }

    #[test]
    fn test_call_tool_result_for_version() {
        let result = json!({
            "content": [
                { "type": "text", "text": "hello" },
                { "type": "resource_link", "uri": "file:///a.txt", "name": "a.txt", "description": "A file" },
                { "type": "audio", "data": "AAAA", "mimeType": "audio/wav" }
            ],
            "structuredContent": { "content": "hello" },
            "isError": false
        });
        assert_eq!(
            call_tool_result_for_version("2025-11-25", result.clone()),
            result
        );
        assert_eq!(
            call_tool_result_for_version("2025-06-18", result.clone()),
            result
        );
        let old = call_tool_result_for_version("2024-11-05", result);
        assert!(old.get("structuredContent").is_none());
        assert_eq!(old["isError"], json!(false));
        let content = old["content"].as_array().unwrap();
        assert_eq!(content[0], json!({ "type": "text", "text": "hello" }));
        assert_eq!(
            content[1],
            json!({ "type": "text", "text": "Resource: a.txt (file:///a.txt)\nA file" })
        );
        assert_eq!(content[2]["type"], json!("text"));
        assert!(content[2]["text"].as_str().unwrap().contains("audio/wav"));
    }
}
