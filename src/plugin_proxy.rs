//! プラグイン（#394、段階1）
//!
//! 各プラグインは、別プロセスで動くMCPサーバー（標準入出力で行区切りの
//! JSON-RPCを話す）として起動する。mcp-rsはMCPクライアントとして接続し、
//! プラグインのツールを`<プラグイン名>__<ツール名>`の名前で中継する。
//!
//! 本体側でかける制御:
//! - 子プロセスに本体の環境変数を引き継がない（設定の`env`で明示した
//!   ものだけを渡す）。WordPressの認証情報等がプラグインに渡らないように
//!   するため
//! - ツール呼び出しのレート制限・監査ログ・タイムアウト・応答サイズの上限
//! - プラグインの標準エラー出力は本体のログに記録し、本体の標準出力
//!   （STDIOのJSON-RPC）には混ぜない
//! - プラグインが異常終了・無応答でも本体は落ちず、そのプラグインの
//!   ツールがエラーを返す（自動再起動はしない）
//!
//! 段階1は、ファイルシステム・ネットワークを隔離しない。信頼できる
//! プラグインにのみ使う（コンテナによる隔離は段階2、#392）。

use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::config::{McpConfig, PluginConfig};
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolCallParams,
};
use crate::security::{AuditCategory, AuditLevel, AuditLogEntry, AuditLogger, RateLimiter};

/// プラグイン名とツール名の区切り
pub const TOOL_NAME_SEPARATOR: &str = "__";

const DEFAULT_CALL_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_PLUGIN_NAME_LEN: usize = 64;
/// `tools/list`のページ数の上限（応答し続けるプラグインで止まらないように）
const MAX_TOOL_LIST_PAGES: usize = 100;
const PROTOCOL_VERSION: &str = "2024-11-05";
/// 標準エラー出力の1行としてログに記録する長さの上限
const MAX_STDERR_LINE_BYTES: u64 = 8192;
const RATE_LIMIT_CLIENT_ID: &str = "plugin-tool-calls";

/// `env_clear`しても引き継ぐ本体の環境変数。Windowsでは、`SystemRoot`が
/// 無いと起動できないプログラムがある（ソケットの初期化等）。
#[cfg(windows)]
const INHERITED_ENV_VARS: &[&str] = &["SystemRoot"];
#[cfg(not(windows))]
const INHERITED_ENV_VARS: &[&str] = &[];

/// プラグイン名を検証する。ツール名の前に付けるため、英小文字・数字・
/// `_`・`-`のみ。区切りと紛らわしい`__`は使えない。
pub fn validate_plugin_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_PLUGIN_NAME_LEN {
        return Err(format!(
            "plugin name must be 1 to {} characters",
            MAX_PLUGIN_NAME_LEN
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(format!(
            "plugin name '{}' may only contain lowercase letters, digits, '_' and '-'",
            name
        ));
    }
    if name.contains(TOOL_NAME_SEPARATOR) {
        return Err(format!(
            "plugin name '{}' must not contain '{}'",
            name, TOOL_NAME_SEPARATOR
        ));
    }
    Ok(())
}

/// 設定されたプラグインを起動する。起動できなかったプラグインは警告を
/// 出して飛ばし、本体の起動は続ける（そのプラグインのツールが使えない
/// だけにする）。
pub async fn start_plugins(configs: &[PluginConfig]) -> Vec<(String, Arc<PluginHandler>)> {
    let mut started = Vec::new();
    let mut seen = HashSet::new();
    for config in configs {
        if !config.enabled.unwrap_or(true) {
            info!("Plugin '{}' is disabled", config.name);
            continue;
        }
        if !seen.insert(config.name.clone()) {
            warn!(
                "Plugin '{}' is not started: another plugin has the same name",
                config.name
            );
            continue;
        }
        match PluginHandler::start(config).await {
            Ok(handler) => {
                info!(
                    "Plugin '{}' started with {} tools",
                    config.name,
                    handler.tools.len()
                );
                started.push((config.name.clone(), Arc::new(handler)));
            }
            Err(e) => warn!("Plugin '{}' is not available: {}", config.name, e),
        }
    }
    started
}

enum ReadLine {
    Line(Vec<u8>),
    /// 上限を超えた行（行末まで読み捨てた）
    Oversized,
}

/// 子プロセスとの接続。リクエストは1つずつ送り、応答を待つ。
struct Connection {
    plugin: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// 読みかけの行。タイムアウトで読み取りを中断しても失わないよう、
    /// 呼び出しをまたいで持つ（遅れて届いた応答は`id`で読み飛ばす）
    line: Vec<u8>,
    /// 上限を超えた行を、行末まで読み捨てている途中
    discarding: bool,
    next_id: u64,
    /// 終了・通信できなくなった理由
    stopped: Option<String>,
}

impl Connection {
    fn not_running_error(&self, reason: &str) -> McpError {
        McpError::ExternalApi(format!(
            "plugin '{}' is not running ({}). Restart mcp-rs to start it again",
            self.plugin, reason
        ))
    }

    fn stop(&mut self, reason: String) {
        warn!("Plugin '{}' stopped: {}", self.plugin, reason);
        let _ = self.child.start_kill();
        self.stopped = Some(reason);
    }

    async fn write_message(&mut self, message: &Value) -> Result<(), McpError> {
        let mut bytes = serde_json::to_vec(message)?;
        bytes.push(b'\n');
        let result = async {
            self.stdin.write_all(&bytes).await?;
            self.stdin.flush().await
        }
        .await;
        if let Err(e) = result {
            let reason = format!("could not write to the plugin: {}", e);
            let error = self.not_running_error(&reason);
            self.stop(reason);
            return Err(error);
        }
        Ok(())
    }

    /// 1行読む。`fill_buf`・`consume`だけで読むので、途中で中断（タイム
    /// アウト）されても、読んだ分は`self.line`に残る。
    async fn read_line(&mut self, max_bytes: usize) -> Result<ReadLine, McpError> {
        loop {
            let available = match self.stdout.fill_buf().await {
                Ok(available) => available,
                Err(e) => {
                    let reason = format!("could not read from the plugin: {}", e);
                    let error = self.not_running_error(&reason);
                    self.stop(reason);
                    return Err(error);
                }
            };
            if available.is_empty() {
                let reason = "the plugin process exited".to_string();
                let error = self.not_running_error(&reason);
                self.stop(reason);
                return Err(error);
            }
            let (len, newline) = match available.iter().position(|b| *b == b'\n') {
                Some(pos) => (pos + 1, true),
                None => (available.len(), false),
            };
            if !self.discarding {
                self.line.extend_from_slice(&available[..len]);
            }
            self.stdout.consume(len);

            // 行末の改行（と\r）は上限に数えない
            if !self.discarding && self.line.len() > max_bytes + 2 {
                self.discarding = true;
                self.line = Vec::new();
            }
            if newline {
                if self.discarding {
                    self.discarding = false;
                    return Ok(ReadLine::Oversized);
                }
                return Ok(ReadLine::Line(std::mem::take(&mut self.line)));
            }
        }
    }

    async fn notify(&mut self, method: &str) -> Result<(), McpError> {
        self.write_message(&json!({ "jsonrpc": "2.0", "method": method }))
            .await
    }

    /// リクエストを送り、同じ`id`の応答を待つ。間に届いた通知・JSONでない
    /// 行・以前のリクエストへの遅れた応答は読み飛ばす。プラグインからの
    /// リクエスト（サンプリング等）には、未対応のエラーを返す。
    async fn request(
        &mut self,
        method: &str,
        params: Value,
        max_bytes: usize,
    ) -> Result<Value, McpError> {
        if let Some(reason) = self.stopped.clone() {
            return Err(self.not_running_error(&reason));
        }
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .await?;

        loop {
            let bytes = match self.read_line(max_bytes).await? {
                ReadLine::Line(bytes) => bytes,
                ReadLine::Oversized => {
                    return Err(McpError::ExternalApi(format!(
                        "plugin '{}' sent a message larger than max_response_bytes ({} bytes)",
                        self.plugin, max_bytes
                    )));
                }
            };
            let message: Value = match serde_json::from_slice(&bytes) {
                Ok(message) => message,
                Err(_) => {
                    let text = String::from_utf8_lossy(&bytes);
                    if !text.trim().is_empty() {
                        warn!(
                            "Plugin '{}' wrote a line that is not JSON-RPC to its output (ignored): {}",
                            self.plugin,
                            truncate(text.trim(), 200)
                        );
                    }
                    continue;
                }
            };

            if let Some(request_method) = message.get("method").and_then(Value::as_str) {
                match message.get("id") {
                    Some(request_id) => {
                        debug!(
                            "Plugin '{}' sent request '{}', which is not supported",
                            self.plugin, request_method
                        );
                        self.write_message(&json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "error": {
                                "code": -32601,
                                "message": "mcp-rs does not support requests from plugins"
                            }
                        }))
                        .await?;
                    }
                    None => debug!(
                        "Plugin '{}' sent notification '{}' (ignored)",
                        self.plugin, request_method
                    ),
                }
                continue;
            }

            if message.get("id") != Some(&json!(id)) {
                debug!(
                    "Plugin '{}' sent a response for another request (ignored)",
                    self.plugin
                );
                continue;
            }
            if let Some(error) = message.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| error.to_string());
                return Err(McpError::ExternalApi(format!(
                    "plugin '{}' returned an error: {}",
                    self.plugin, detail
                )));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

/// プラグインの標準エラー出力を、1行ずつ本体のログに記録する。
fn spawn_stderr_logger(plugin: String, stderr: tokio::process::ChildStderr) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match (&mut reader)
                .take(MAX_STDERR_LINE_BYTES)
                .read_until(b'\n', &mut buf)
                .await
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf);
                    let line = line.trim_end();
                    if !line.is_empty() {
                        info!("[plugin {}] {}", plugin, line);
                    }
                }
            }
        }
    });
}

/// プラグイン1つ分の中継用ハンドラー
pub struct PluginHandler {
    name: String,
    connection: Mutex<Connection>,
    /// 名前空間付きのツール
    tools: Vec<Tool>,
    /// 名前空間付きのツール名 → プラグインでのツール名
    original_names: HashMap<String, String>,
    call_timeout: Duration,
    max_response_bytes: usize,
    rate_limiter: RateLimiter,
    audit_logger: AuditLogger,
}

impl PluginHandler {
    /// 子プロセスを起動し、`initialize`・`tools/list`まで行う。
    pub async fn start(config: &PluginConfig) -> Result<Self, String> {
        validate_plugin_name(&config.name)?;
        if config.command.trim().is_empty() {
            return Err("command is empty".to_string());
        }
        let call_timeout = Duration::from_secs(
            config
                .call_timeout_seconds
                .unwrap_or(DEFAULT_CALL_TIMEOUT_SECS),
        );
        let max_response_bytes = config
            .max_response_bytes
            .unwrap_or(DEFAULT_MAX_RESPONSE_BYTES);

        let mut command = Command::new(&config.command);
        command.args(&config.args).env_clear();
        for key in INHERITED_ENV_VARS {
            if let Ok(value) = std::env::var(key) {
                command.env(key, value);
            }
        }
        for (key, value) in &config.env {
            let expanded = McpConfig::expand_env_vars(value);
            if expanded.contains("[ERROR:") {
                // 見つからない変数を含む値は渡さない（APIキーの代わりに
                // "[ERROR:...]"という文字列を渡さないため）
                warn!(
                    "Plugin '{}': environment variable '{}' is not passed because a variable it refers to is not set",
                    config.name, key
                );
                continue;
            }
            command.env(key, expanded);
        }
        if let Some(dir) = &config.working_dir {
            command.current_dir(dir);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // コンソールウィンドウを開かない（CREATE_NO_WINDOW）
            command.creation_flags(0x0800_0000);
        }

        let mut child = command
            .spawn()
            .map_err(|e| format!("could not start '{}': {}", config.command, e))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err("could not open the plugin's standard input/output".to_string());
        };
        spawn_stderr_logger(config.name.clone(), stderr);

        let mut connection = Connection {
            plugin: config.name.clone(),
            child,
            stdin,
            stdout: BufReader::new(stdout),
            line: Vec::new(),
            discarding: false,
            next_id: 1,
            stopped: None,
        };

        let raw_tools = match tokio::time::timeout(
            call_timeout,
            Self::handshake(&mut connection, max_response_bytes),
        )
        .await
        {
            Ok(Ok(tools)) => tools,
            Ok(Err(e)) => {
                let _ = connection.child.start_kill();
                return Err(e.to_string());
            }
            Err(_) => {
                let _ = connection.child.start_kill();
                return Err(format!(
                    "the plugin did not finish starting within {} seconds",
                    call_timeout.as_secs()
                ));
            }
        };

        let mut tools = Vec::new();
        let mut original_names = HashMap::new();
        for raw in raw_tools {
            let Some(original) = raw.get("name").and_then(Value::as_str) else {
                warn!(
                    "Plugin '{}' listed a tool without a name (ignored)",
                    config.name
                );
                continue;
            };
            let name = format!("{}{}{}", config.name, TOOL_NAME_SEPARATOR, original);
            if original_names.contains_key(&name) {
                warn!(
                    "Plugin '{}' listed tool '{}' twice (ignored the second)",
                    config.name, original
                );
                continue;
            }
            let description = raw.get("description").and_then(Value::as_str).unwrap_or("");
            tools.push(Tool {
                name: name.clone(),
                // どのプラグインのツールかを、AIと利用者が見分けられるようにする
                description: format!("[plugin: {}] {}", config.name, description)
                    .trim_end()
                    .to_string(),
                input_schema: raw
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object" })),
            });
            original_names.insert(name, original.to_string());
        }

        Ok(Self {
            name: config.name.clone(),
            connection: Mutex::new(connection),
            tools,
            original_names,
            call_timeout,
            max_response_bytes,
            rate_limiter: RateLimiter::new(config.rate_limit.clone().unwrap_or_default()),
            audit_logger: AuditLogger::with_defaults(),
        })
    }

    async fn handshake(
        connection: &mut Connection,
        max_bytes: usize,
    ) -> Result<Vec<Value>, McpError> {
        connection
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": "mcp-rs",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
                max_bytes,
            )
            .await?;
        connection.notify("notifications/initialized").await?;

        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_TOOL_LIST_PAGES {
            let params = match &cursor {
                Some(cursor) => json!({ "cursor": cursor }),
                None => json!({}),
            };
            let result = connection.request("tools/list", params, max_bytes).await?;
            if let Some(page) = result.get("tools").and_then(Value::as_array) {
                tools.extend(page.iter().cloned());
            }
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                return Ok(tools);
            }
        }
        warn!(
            "Plugin '{}' returned more than {} pages of tools; the rest are ignored",
            connection.plugin, MAX_TOOL_LIST_PAGES
        );
        Ok(tools)
    }

    async fn audit_tool_call(&self, tool_name: &str, result: &Result<Value, McpError>) {
        let success = result.is_ok();
        let status = if success { "SUCCESS" } else { "FAILURE" };
        let mut entry = AuditLogEntry::new(
            if success {
                AuditLevel::Info
            } else {
                AuditLevel::Error
            },
            AuditCategory::DataAccess,
            format!("Plugin tool call: {} - {}", tool_name, status),
        )
        .with_action(format!("plugin:{}", self.name), tool_name.to_string())
        .with_result(status.to_string());
        if let Err(e) = result {
            entry = entry.add_metadata("error".to_string(), e.to_string());
        }
        if let Err(e) = self.audit_logger.log(entry).await {
            warn!("Failed to write audit log entry for {}: {}", tool_name, e);
        }
    }

    async fn forward_tool_call(
        &self,
        original: &str,
        arguments: Option<HashMap<String, Value>>,
    ) -> Result<Value, McpError> {
        let params = json!({
            "name": original,
            "arguments": arguments.unwrap_or_default()
        });
        let result = tokio::time::timeout(self.call_timeout, async {
            let mut connection = self.connection.lock().await;
            connection
                .request("tools/call", params, self.max_response_bytes)
                .await
        })
        .await
        .unwrap_or_else(|_| {
            Err(McpError::ExternalApi(format!(
                "plugin '{}' did not respond within {} seconds",
                self.name,
                self.call_timeout.as_secs()
            )))
        })?;

        // CallToolResult（content等を持つオブジェクト）をそのまま返す
        if !result.is_object() {
            return Err(McpError::ExternalApi(format!(
                "plugin '{}' returned an invalid tool result",
                self.name
            )));
        }
        Ok(result)
    }

    #[cfg(test)]
    fn audit_logger(&self) -> &AuditLogger {
        &self.audit_logger
    }
}

#[async_trait]
impl McpHandler for PluginHandler {
    async fn initialize(&self, _params: InitializeParams) -> Result<Value, McpError> {
        // プラグインとのinitializeは起動時に済ませている
        Ok(json!({}))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        // 異常終了した後も一覧には残し、呼び出し時に理由を返す
        Ok(self.tools.clone())
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<Value, McpError> {
        let Some(original) = self.original_names.get(&params.name) else {
            return Err(McpError::ToolNotFound(params.name));
        };

        let result = match self
            .rate_limiter
            .check_rate_limit(RATE_LIMIT_CLIENT_ID)
            .await
        {
            Err(e) => {
                warn!("Plugin '{}' tool call rate limited: {}", self.name, e);
                Err(McpError::Other(format!("Rate limit exceeded: {}", e)))
            }
            Ok(_) => self.forward_tool_call(original, params.arguments).await,
        };
        // プラグインのツールは何をするか本体には分からないため、すべて記録する
        self.audit_tool_call(&params.name, &result).await;
        result
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        // 段階1はツールのみ中継する
        Ok(Vec::new())
    }

    async fn read_resource(&self, params: ResourceReadParams) -> Result<Value, McpError> {
        Err(McpError::ResourceNotFound(params.uri))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PLUGIN_ENV: &str = "MCP_RS_TEST_PLUGIN";

    /// テスト用のMCPサーバー。このテストバイナリ自身をプラグインとして
    /// 起動し、この関数だけを実行させる（`--exact`・`--ignored`）。通常の
    /// テスト実行では、環境変数が無いのですぐに終わる。
    #[test]
    #[ignore]
    fn test_plugin_server_main() {
        use std::io::{BufRead, Write};

        if std::env::var(TEST_PLUGIN_ENV).ok().as_deref() != Some("1") {
            return;
        }
        eprintln!("test plugin started");
        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        let mut out = std::io::stdout();
        // テストランナーが標準出力に書いた"test ... "の行を終わらせる
        // （終わらせないと、最初の応答が同じ行に続いてJSONとして読めない）
        out.write_all(b"\n").unwrap();
        out.flush().unwrap();
        let mut send = |value: Value| {
            let mut text = value.to_string();
            text.push('\n');
            out.write_all(text.as_bytes()).unwrap();
            out.flush().unwrap();
        };
        let text_result = |text: String| json!({ "content": [{ "type": "text", "text": text }] });

        while let Some(Ok(line)) = lines.next() {
            let request: Value = serde_json::from_str(&line).unwrap();
            let Some(id) = request.get("id").cloned() else {
                continue; // 通知
            };
            let method = request["method"].as_str().unwrap_or("");
            let result = match method {
                "initialize" => json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "test-plugin", "version": "1.0.0" }
                }),
                "tools/list" => {
                    let tool = |name: &str| {
                        json!({
                            "name": name,
                            "description": format!("{} tool", name),
                            "inputSchema": { "type": "object" }
                        })
                    };
                    // ページ分けを確かめるため、2ページに分ける
                    if request["params"]["cursor"] == json!("page2") {
                        json!({ "tools": [tool("big"), tool("exit"), tool("noisy"), tool("fail")] })
                    } else {
                        json!({
                            "tools": [tool("echo"), tool("env"), tool("sleep")],
                            "nextCursor": "page2"
                        })
                    }
                }
                "tools/call" => {
                    let args = &request["params"]["arguments"];
                    match request["params"]["name"].as_str().unwrap_or("") {
                        "echo" => text_result(args["text"].as_str().unwrap_or("").to_string()),
                        "env" => {
                            let mut keys: Vec<String> = std::env::vars().map(|(k, _)| k).collect();
                            keys.sort();
                            text_result(keys.join(","))
                        }
                        "sleep" => {
                            let ms = args["ms"].as_u64().unwrap_or(0);
                            std::thread::sleep(Duration::from_millis(ms));
                            text_result("slept".to_string())
                        }
                        "big" => {
                            text_result("x".repeat(args["bytes"].as_u64().unwrap_or(0) as usize))
                        }
                        "exit" => std::process::exit(3),
                        "fail" => {
                            send(json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": { "code": -32000, "message": "something went wrong" }
                            }));
                            continue;
                        }
                        "noisy" => {
                            // JSONでない行・通知・プラグインからのリクエストを
                            // 挟んでから応答する
                            {
                                let mut raw = std::io::stdout();
                                raw.write_all(b"this is not json\n").unwrap();
                                raw.flush().unwrap();
                            }
                            send(
                                json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": {} }),
                            );
                            send(
                                json!({ "jsonrpc": "2.0", "id": "srv-1", "method": "sampling/createMessage", "params": {} }),
                            );
                            let reply: Value =
                                serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
                            let ok = reply["id"] == json!("srv-1")
                                && reply["error"]["code"] == json!(-32601);
                            text_result(format!("noisy done, reply ok: {}", ok))
                        }
                        other => text_result(format!("unknown {}", other)),
                    }
                }
                _ => json!({}),
            };
            send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
        }
    }

    fn test_plugin_config(name: &str) -> PluginConfig {
        let exe = std::env::current_exe().unwrap();
        let mut env = HashMap::new();
        env.insert(TEST_PLUGIN_ENV.to_string(), "1".to_string());
        // 本体の環境変数を参照して展開できること（PATHはどの環境にもある）
        env.insert("PLUGIN_TOKEN".to_string(), "${PATH}".to_string());
        // 見つからない変数を参照する値は渡さない
        env.insert(
            "MISSING_TOKEN".to_string(),
            "${MCP_RS_TEST_SURELY_UNSET_VAR}".to_string(),
        );
        PluginConfig {
            name: name.to_string(),
            command: exe.to_string_lossy().into_owned(),
            args: vec![
                "--exact".to_string(),
                "plugin_proxy::tests::test_plugin_server_main".to_string(),
                "--ignored".to_string(),
                "--nocapture".to_string(),
                "--test-threads=1".to_string(),
            ],
            env,
            working_dir: None,
            enabled: None,
            call_timeout_seconds: Some(10),
            max_response_bytes: Some(4096),
            rate_limit: None,
        }
    }

    async fn call(handler: &PluginHandler, tool: &str, args: Value) -> Result<Value, McpError> {
        let arguments = args
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        handler
            .call_tool(ToolCallParams {
                name: tool.to_string(),
                arguments,
            })
            .await
    }

    fn text(result: &Value) -> &str {
        result["content"][0]["text"].as_str().unwrap()
    }

    #[test]
    fn test_validate_plugin_name() {
        for ok in ["example", "my-plugin", "tool_1", "a"] {
            assert!(validate_plugin_name(ok).is_ok(), "{}", ok);
        }
        for bad in [
            "",
            "Example",
            "my plugin",
            "a__b",
            "日本語",
            "a.b",
            &"a".repeat(65),
        ] {
            assert!(validate_plugin_name(bad).is_err(), "{}", bad);
        }
    }

    #[tokio::test]
    async fn test_plugin_tools_are_namespaced_and_calls_are_relayed() {
        let handler = PluginHandler::start(&test_plugin_config("demo"))
            .await
            .unwrap();

        // 2ページ分のツールが、名前空間付きで並ぶ
        let names: Vec<String> = handler
            .list_tools()
            .await
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "demo__echo",
                "demo__env",
                "demo__sleep",
                "demo__big",
                "demo__exit",
                "demo__noisy",
                "demo__fail"
            ]
        );
        let tools = handler.list_tools().await.unwrap();
        assert_eq!(tools[0].description, "[plugin: demo] echo tool");

        let result = call(&handler, "demo__echo", json!({ "text": "hello" }))
            .await
            .unwrap();
        assert_eq!(text(&result), "hello");

        // プラグインのエラー応答は、ツールのエラーとして返す
        let error = call(&handler, "demo__fail", json!({})).await.unwrap_err();
        assert!(
            error.to_string().contains("something went wrong"),
            "{}",
            error
        );

        // 名前空間の無い名前・他のプラグインの名前は受け付けない
        assert!(matches!(
            call(&handler, "echo", json!({})).await,
            Err(McpError::ToolNotFound(_))
        ));

        // すべての呼び出しを監査ログに記録する
        let entries = handler.audit_logger().get_all_entries().await;
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn test_plugin_gets_only_configured_environment_variables() {
        let handler = PluginHandler::start(&test_plugin_config("envcheck"))
            .await
            .unwrap();
        let result = call(&handler, "envcheck__env", json!({})).await.unwrap();
        let mut keys: Vec<&str> = text(&result)
            .split(',')
            .filter(|k| !INHERITED_ENV_VARS.iter().any(|i| i.eq_ignore_ascii_case(k)))
            .collect();
        keys.sort();
        // 本体の環境変数（PATH等）は渡らず、設定したものだけが渡る。
        // 参照先が見つからないMISSING_TOKENは渡さない
        assert_eq!(keys, vec![TEST_PLUGIN_ENV, "PLUGIN_TOKEN"]);
    }

    #[tokio::test]
    async fn test_noise_notifications_and_plugin_requests_are_handled() {
        let handler = PluginHandler::start(&test_plugin_config("noisy"))
            .await
            .unwrap();
        let result = call(&handler, "noisy__noisy", json!({})).await.unwrap();
        assert_eq!(text(&result), "noisy done, reply ok: true");
    }

    #[tokio::test]
    async fn test_timeout_returns_error_and_later_calls_still_work() {
        let mut config = test_plugin_config("slow");
        config.call_timeout_seconds = Some(1);
        let handler = PluginHandler::start(&config).await.unwrap();

        let error = call(&handler, "slow__sleep", json!({ "ms": 1500 }))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("did not respond"), "{}", error);

        // 遅れて届いた応答は読み飛ばし、次の呼び出しの応答を返す
        let result = call(&handler, "slow__echo", json!({ "text": "after" }))
            .await
            .unwrap();
        assert_eq!(text(&result), "after");
    }

    #[tokio::test]
    async fn test_oversized_response_returns_error_and_later_calls_still_work() {
        let handler = PluginHandler::start(&test_plugin_config("big"))
            .await
            .unwrap();
        let error = call(&handler, "big__big", json!({ "bytes": 10000 }))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("max_response_bytes"),
            "{}",
            error
        );

        let result = call(&handler, "big__echo", json!({ "text": "fine" }))
            .await
            .unwrap();
        assert_eq!(text(&result), "fine");
    }

    #[tokio::test]
    async fn test_exited_plugin_returns_errors_but_keeps_its_tools() {
        let handler = PluginHandler::start(&test_plugin_config("crash"))
            .await
            .unwrap();
        let error = call(&handler, "crash__exit", json!({})).await.unwrap_err();
        assert!(error.to_string().contains("exited"), "{}", error);

        let error = call(&handler, "crash__echo", json!({ "text": "x" }))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not running"), "{}", error);
        assert_eq!(handler.list_tools().await.unwrap().len(), 7);
    }

    #[tokio::test]
    async fn test_start_plugins_skips_plugins_that_cannot_start() {
        let missing = PluginConfig {
            command: "mcp-rs-surely-missing-plugin-command".to_string(),
            ..test_plugin_config("missing")
        };
        let bad_name = test_plugin_config("Bad Name");
        let disabled = PluginConfig {
            enabled: Some(false),
            ..test_plugin_config("disabled")
        };
        let good = test_plugin_config("good");
        let duplicate = test_plugin_config("good");

        let started = start_plugins(&[missing, bad_name, disabled, good, duplicate]).await;
        let names: Vec<&str> = started.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["good"]);
    }

    #[tokio::test]
    async fn test_plugin_that_is_not_an_mcp_server_fails_to_start() {
        // 応答しないプロセス（テストバイナリを、存在しないテスト名で起動
        // すると、何も出力せずに終わる）
        let mut config = test_plugin_config("notmcp");
        config.args = vec!["--exact".to_string(), "no_such_test".to_string()];
        config.call_timeout_seconds = Some(5);
        let error = PluginHandler::start(&config).await.err().unwrap();
        assert!(error.contains("exited"), "{}", error);
    }
}
