//! mcp-rs自身の状態を返すツール（#434）
//!
//! AI（Claude Desktop・Claude Code等）が、ツールを呼ぶだけでバージョン・
//! ログと監査ログの書き込み先・設定ファイル・プラグインの状態を確かめられる
//! ようにする。パスワード・キー・トークン・環境変数の値は含めない。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::mcp::{Caller, McpHandler, Tool, ToolAnnotations};

/// ツール名
pub const STATUS_TOOL_NAME: &str = "mcp_rs_status";

static PROCESS_STARTED_AT: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// プロセスが起動した時刻。`main`の最初で呼んでおくと、その時刻になる
/// （呼ばれていなければ、最初に呼ばれた時刻）
pub fn process_started_at() -> Instant {
    *PROCESS_STARTED_AT.get_or_init(Instant::now)
}

/// 起動時に決まる、状態の表示に使う情報
#[derive(Debug, Clone)]
pub struct StatusInfo {
    /// 動作モード（`stdio`・`http`）
    pub mode: String,
    /// 読み込んだ設定ファイル（`None`は設定ファイルを使っていない）
    pub config_file: Option<PathBuf>,
    pub started_at: Instant,
    /// 起動できなかったプラグイン（名前、理由）
    pub plugin_failures: Vec<(String, String)>,
}

impl Default for StatusInfo {
    fn default() -> Self {
        Self {
            mode: "unknown".to_string(),
            config_file: None,
            started_at: Instant::now(),
            plugin_failures: Vec::new(),
        }
    }
}

impl StatusInfo {
    /// HTTPモードで、認証していない呼び出し元か。ネットワーク越しの誰にでも
    /// サーバーの内部情報（ファイルのパス等）を見せないよう、バージョンだけを
    /// 返す
    fn is_unauthenticated_http(&self, caller: &Caller) -> bool {
        self.mode == "http" && caller.name.is_none()
    }
}

/// ツールの定義
pub fn status_tool() -> Tool {
    Tool {
        name: STATUS_TOOL_NAME.to_string(),
        title: Some("mcp-rs status".to_string()),
        description: "Show the status of the mcp-rs server itself: version, mode, protocol \
                      version, config file, where the log and audit log are written, and the \
                      state of each handler and plugin. Use it to check the installed version \
                      or to find the log files."
            .to_string(),
        input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            open_world_hint: Some(false),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// 状態の本文（テキスト）を作る
pub async fn render_status(
    info: &StatusInfo,
    protocol_version: &str,
    caller: &Caller,
    audit_logger: Option<&crate::security::AuditLogger>,
    handlers: &[(String, Arc<dyn McpHandler>)],
) -> String {
    let mut lines = vec![format!("mcp-rs {}", env!("CARGO_PKG_VERSION"))];
    if info.is_unauthenticated_http(caller) {
        lines.push(
            "Other details are shown only to authenticated callers in HTTP mode (set auth_enabled or [transport.http.oauth])."
                .to_string(),
        );
        return lines.join("\n");
    }

    lines.push(format!("Mode: {}", info.mode));
    lines.push(format!("Protocol version: {}", protocol_version));
    lines.push(format!(
        "OS: {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    lines.push(format!("Process ID: {}", std::process::id()));
    lines.push(format!("Instance: {}", crate::logging::instance_id()));
    lines.push(format!(
        "Uptime: {}",
        format_duration(info.started_at.elapsed())
    ));
    if let Some(name) = &caller.name {
        lines.push(format!("Caller: {}", name));
    }
    lines.push(format!(
        "Config file: {}",
        info.config_file
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none (built-in defaults)".to_string())
    ));
    lines.push(format!("Log: {}", log_status()));
    lines.push(format!(
        "Audit log: {}",
        audit_log_status(audit_logger).await
    ));

    lines.push("Handlers:".to_string());
    if handlers.is_empty() {
        lines.push("- none".to_string());
    }
    for (name, handler) in handlers {
        let summary = handler.status().await;
        lines.push(match summary {
            Some(summary) => format!("- {}: {}", name, summary),
            None => format!("- {}", name),
        });
    }
    if !info.plugin_failures.is_empty() {
        lines.push("Plugins that failed to start:".to_string());
        for (name, reason) in &info.plugin_failures {
            lines.push(format!("- {}: {}", name, reason));
        }
    }
    lines.join("\n")
}

fn log_status() -> String {
    match crate::logging::current_log_destination() {
        Some(destination) => destination.detailed_status(),
        None => "file output is disabled (logging to stderr only)".to_string(),
    }
}

async fn audit_log_status(audit_logger: Option<&crate::security::AuditLogger>) -> String {
    let Some(audit_logger) = audit_logger else {
        return "not configured".to_string();
    };
    match audit_logger.file_path() {
        Some(path) => match audit_logger.check_file_output().await {
            Ok(()) => path.to_string(),
            Err(e) => format!(
                "{} (cannot be written: {}; entries are kept in memory only)",
                path, e
            ),
        },
        None => "memory only (no file; see [audit] in the config file)".to_string(),
    }
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
        assert_eq!(format_duration(Duration::from_secs(3725)), "1h 2m 5s");
    }

    #[test]
    fn test_status_tool_is_read_only() {
        let tool = status_tool();
        assert_eq!(tool.name, STATUS_TOOL_NAME);
        assert_eq!(tool.annotations.and_then(|a| a.read_only_hint), Some(true));
    }

    #[tokio::test]
    async fn test_unauthenticated_http_sees_only_the_version() {
        let info = StatusInfo {
            mode: "http".to_string(),
            config_file: Some(PathBuf::from("C:/secret/place/mcp-config.toml")),
            ..Default::default()
        };
        let text = render_status(&info, "2025-06-18", &Caller::unrestricted(), None, &[]).await;
        assert!(text.starts_with(&format!("mcp-rs {}", env!("CARGO_PKG_VERSION"))));
        assert!(!text.contains("secret"), "{}", text);

        // 認証した呼び出し元・STDIOには、すべて見せる
        let caller = Caller::new("api_key:admin", crate::mcp::ToolAccess::All);
        let text = render_status(&info, "2025-06-18", &caller, None, &[]).await;
        assert!(text.contains("C:/secret/place/mcp-config.toml"), "{}", text);
        assert!(text.contains("Caller: api_key:admin"), "{}", text);
        let stdio = StatusInfo {
            mode: "stdio".to_string(),
            ..info
        };
        let text = render_status(&stdio, "2024-11-05", &Caller::unrestricted(), None, &[]).await;
        assert!(text.contains("Mode: stdio"), "{}", text);
        assert!(text.contains("Protocol version: 2024-11-05"), "{}", text);
        assert!(!text.contains("Caller:"), "{}", text);
    }

    #[tokio::test]
    async fn test_audit_log_status() {
        assert_eq!(audit_log_status(None).await, "not configured");
        let memory = crate::security::AuditLogger::with_defaults();
        assert!(audit_log_status(Some(&memory))
            .await
            .starts_with("memory only"));
        let dir = std::env::temp_dir().join(format!("mcp-rs-status-{}", uuid::Uuid::new_v4()));
        let path = dir.join("audit.log");
        let logger = crate::security::AuditLogger::new(crate::security::AuditConfig {
            enable_file_output: true,
            log_file_path: Some(path.to_string_lossy().into_owned()),
            ..Default::default()
        });
        assert_eq!(
            audit_log_status(Some(&logger)).await,
            path.to_string_lossy()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
