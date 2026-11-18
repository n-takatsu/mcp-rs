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

/// `initialize`を使わない版（#570）。版・クライアントの能力を、リクエスト
/// ごとに`params._meta`で受け取る（ステートレス）。`initialize`で交渉する
/// 版（`SUPPORTED_PROTOCOL_VERSIONS`）とは別に持ち、新旧の両方を受ける
/// （仕様の「dual-era」）。
pub const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";

/// `_meta`で受ける版（新しい順）。`server/discover`の`supportedVersions`
pub const MODERN_PROTOCOL_VERSIONS: &[&str] = &[MODERN_PROTOCOL_VERSION];

/// リクエストの`_meta`の、版・クライアントの能力・クライアントの情報・
/// サーバーの情報のキー（2026-07-28）
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
pub const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// 対応していない版を要求されたときのエラー（2026-07-28の
/// `UnsupportedProtocolVersion`）
pub const UNSUPPORTED_PROTOCOL_VERSION_CODE: i32 = -32022;

/// `_meta`で要求できる版か
pub fn is_modern_protocol_version(version: &str) -> bool {
    MODERN_PROTOCOL_VERSIONS.contains(&version)
}

/// 対応するすべての版（新しい順）。`_meta`で受ける版と、`initialize`で
/// 交渉する版。対応していない版へのエラーの`data.supported`に入れ、
/// クライアントが`initialize`に戻れることも伝える
pub fn all_supported_protocol_versions() -> Vec<&'static str> {
    MODERN_PROTOCOL_VERSIONS
        .iter()
        .chain(SUPPORTED_PROTOCOL_VERSIONS)
        .copied()
        .collect()
}

/// 版が、リクエストごとの`_meta`で受けるもの（2026-07-28）か、
/// `initialize`で交渉する（HTTPではヘッダーでも受ける）もの（legacy）か。
/// `mcp_rs_status`に出す
pub fn protocol_era(version: &str) -> &'static str {
    if is_modern_protocol_version(version) {
        "stateless, per-request _meta"
    } else {
        "legacy"
    }
}

/// クライアントが要求した版から、使う版を決める。
pub fn negotiate_protocol_version(requested: &str) -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|v| *v == requested)
        .unwrap_or(LATEST_PROTOCOL_VERSION)
}

/// `initialize`の結果の`serverInfo`（Implementation）。`description`・
/// `websiteUrl`は2025-11-25で加わった項目なので、その版から付ける（#475）。
/// 値はCargo.tomlの`description`・`homepage`。
pub fn server_info_for_version(version: &str) -> Value {
    let mut info = serde_json::json!({
        "name": "mcp-rs",
        "version": env!("CARGO_PKG_VERSION")
    });
    if is_at_least(version, "2025-11-25") {
        info["description"] = Value::from(env!("CARGO_PKG_DESCRIPTION"));
        info["websiteUrl"] = Value::from(env!("CARGO_PKG_HOMEPAGE"));
    }
    info
}

/// `server_info_for_version`に、mcp-rsのアイコン（`icons`、2025-11-25で
/// 加わった項目）を足したもの（#570）。図柄はサイトのファビコン
/// （`website/assets`、#613）。クライアントが外のサイトに取りに行かない
/// よう、`data:`のURIで埋め込む。
///
/// 約4KBあるので、`initialize`と`server/discover`の結果だけに付ける。
/// 2026-07-28の、ほかの結果の`_meta`のサーバーの情報には付けない。
pub fn server_info_with_icons(version: &str) -> Value {
    let mut info = server_info_for_version(version);
    if is_at_least(version, "2025-11-25") {
        info["icons"] = server_icons();
    }
    info
}

/// mcp-rsのアイコン。クライアントが必ず表示できるPNG（仕様のMUST）を
/// 2つの大きさで、拡大しても崩れないSVGを1つ
fn server_icons() -> Value {
    use base64::Engine as _;
    let data_uri = |mime_type: &str, bytes: &[u8]| {
        format!(
            "data:{};base64,{}",
            mime_type,
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    };
    serde_json::json!([
        {
            "src": data_uri("image/png", include_bytes!("../../website/assets/favicon-32.png")),
            "mimeType": "image/png",
            "sizes": ["32x32"]
        },
        {
            "src": data_uri(
                "image/png",
                include_bytes!("../../website/assets/apple-touch-icon.png")
            ),
            "mimeType": "image/png",
            "sizes": ["180x180"]
        },
        {
            "src": data_uri("image/svg+xml", include_bytes!("../../website/assets/favicon.svg")),
            "mimeType": "image/svg+xml",
            "sizes": ["any"]
        }
    ])
}

/// `version`が`since`以降の版か。版は`YYYY-MM-DD`の日付なので、文字列の
/// 比較で新旧が決まる。対応する版を足しても、項目ごとの判定を直さなくてよい
/// ように、除く版を列挙せずに比べる。`version`は交渉で決まった版
/// （`SUPPORTED_PROTOCOL_VERSIONS`のどれか）。
fn is_at_least(version: &str, since: &str) -> bool {
    version >= since
}

/// ツール定義の`title`・`outputSchema`（2025-06-18）、`annotations`
/// （2025-03-26）、ツールの結果の`structuredContent`・リソースリンク
/// （2025-06-18）、音声（2025-03-26）を使える版か。2025-03-26の項目も
/// あるが、すべてがそろうのは2025-06-18から。
fn supports_2025_features(version: &str) -> bool {
    is_at_least(version, "2025-06-18")
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
    fn test_is_at_least() {
        assert!(is_at_least("2025-11-25", "2025-11-25"));
        assert!(is_at_least("2025-11-25", "2025-06-18"));
        assert!(!is_at_least("2025-06-18", "2025-11-25"));
        assert!(!is_at_least("2024-11-05", "2025-06-18"));
        // 間の版を対応する版に足した場合も、新しい項目は付かない
        assert!(!is_at_least("2025-03-26", "2025-06-18"));
        assert!(!is_at_least("2025-03-26", "2025-11-25"));
        // 対応する版は、新しい順に並んでいる
        assert!(SUPPORTED_PROTOCOL_VERSIONS
            .windows(2)
            .all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn test_server_info_for_version() {
        let latest = server_info_for_version("2025-11-25");
        assert_eq!(latest["name"], json!("mcp-rs"));
        assert_eq!(latest["version"], json!(env!("CARGO_PKG_VERSION")));
        assert!(latest["description"]
            .as_str()
            .is_some_and(|d| !d.is_empty()));
        assert_eq!(
            latest["websiteUrl"],
            json!("https://n-takatsu.github.io/mcp-rs/")
        );

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

    #[test]
    fn test_server_info_with_icons() {
        use base64::Engine as _;
        for version in ["2026-07-28", "2025-11-25"] {
            let info = server_info_with_icons(version);
            // アイコン以外は、server_info_for_versionと同じ
            let mut without_icons = info.clone();
            without_icons.as_object_mut().unwrap().remove("icons");
            assert_eq!(without_icons, server_info_for_version(version));

            let icons = info["icons"].as_array().expect("icons");
            let expected: [(&str, &str, &[u8]); 3] = [
                (
                    "image/png",
                    "32x32",
                    include_bytes!("../../website/assets/favicon-32.png"),
                ),
                (
                    "image/png",
                    "180x180",
                    include_bytes!("../../website/assets/apple-touch-icon.png"),
                ),
                (
                    "image/svg+xml",
                    "any",
                    include_bytes!("../../website/assets/favicon.svg"),
                ),
            ];
            assert_eq!(icons.len(), expected.len());
            for (icon, (mime_type, size, bytes)) in icons.iter().zip(expected) {
                assert_eq!(icon["mimeType"], json!(mime_type));
                assert_eq!(icon["sizes"], json!([size]));
                // 外に取りに行かない、data:のURI
                let src = icon["src"].as_str().unwrap();
                let encoded = src
                    .strip_prefix(&format!("data:{};base64,", mime_type))
                    .expect("a data: URI");
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .unwrap(),
                    bytes
                );
            }
        }
        // 古い版には、iconsが無い
        for old in ["2025-06-18", "2024-11-05"] {
            assert_eq!(server_info_with_icons(old), server_info_for_version(old));
        }
    }
}
