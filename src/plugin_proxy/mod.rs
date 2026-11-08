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
//! 子プロセスとして起動した場合（段階1）は、ファイルシステム・
//! ネットワークを隔離しない。信頼できるプラグインにのみ使う。
//!
//! 設定に`container`を書くと、`docker run -i`でコンテナの中で起動する
//! （段階2、#411）。既定で、ネットワークなし・全ケーパビリティ削除・
//! ルートファイルシステム読み取り専用・メモリとプロセス数の上限付きで、
//! マウントは設定したものだけ（既定で読み取り専用）。通信は子プロセスと
//! 同じく標準入出力で、dockerコマンドが中継する。
//!
//! 設定に`kubernetes`を書くと、KubernetesのJobとして起動し、
//! `kubectl attach -i`で中継する（段階3、#413。`kubernetes.rs`）。

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

use crate::config::{McpConfig, PluginConfig, PluginContainerConfig};

mod kubernetes;
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolAnnotations,
    ToolCallParams, LATEST_PROTOCOL_VERSION,
};
use crate::security::{AuditCategory, AuditLevel, AuditLogEntry, AuditLogger, RateLimiter};

/// プラグイン名とツール名の区切り
pub const TOOL_NAME_SEPARATOR: &str = "__";

const DEFAULT_CALL_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_PLUGIN_NAME_LEN: usize = 64;
/// `tools/list`のページ数の上限（応答し続けるプラグインで止まらないように）
const MAX_TOOL_LIST_PAGES: usize = 100;
/// プラグインへの`initialize`で要求する版（#417）。プラグインが古い版で
/// 答えた場合も、そのまま使う（mcp-rsは古い版の項目も読める）
const PROTOCOL_VERSION: &str = LATEST_PROTOCOL_VERSION;
/// プラグインが答えてよい版。これ以外は警告を出す（使い続ける）
const KNOWN_PLUGIN_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// 標準エラー出力の1行としてログに記録する長さの上限
const MAX_STDERR_LINE_BYTES: u64 = 8192;
const RATE_LIMIT_CLIENT_ID: &str = "plugin-tool-calls";

/// `env_clear`しても引き継ぐ本体の環境変数。Windowsでは、`SystemRoot`が
/// 無いと起動できないプログラムがある（ソケットの初期化等）。
#[cfg(windows)]
const INHERITED_ENV_VARS: &[&str] = &["SystemRoot"];
#[cfg(not(windows))]
const INHERITED_ENV_VARS: &[&str] = &[];

/// コンテナで起動する場合に、dockerコマンド自体に渡す本体の環境変数
/// （dockerの設定ファイルの場所・接続先等）。コンテナには渡らない。
/// このほか`DOCKER_`で始まるものも渡す。
const DOCKER_CLI_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ProgramFiles",
    "SystemRoot",
    "TEMP",
    "TMP",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "CONTAINER_HOST",
    "CONTAINER_CONNECTION",
];
const DEFAULT_CONTAINER_NETWORK: &str = "none";
const DEFAULT_CONTAINER_MEMORY: &str = "512m";
const DEFAULT_CONTAINER_PIDS_LIMIT: u32 = 256;
const DEFAULT_DOCKER_COMMAND: &str = "docker";
/// 読み取り専用のルートファイルシステムで、書き込める作業領域
const CONTAINER_TMPFS: &str = "/tmp:rw,noexec,nosuid,size=64m";
/// コンテナに付けるラベル（値はプラグイン名）
const CONTAINER_LABEL: &str = "mcp-rs.plugin";

fn is_docker_cli_env_var(key: &str) -> bool {
    DOCKER_CLI_ENV_VARS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(key))
        || key.to_ascii_uppercase().starts_with("DOCKER_")
}

/// プラグインに渡す環境変数（設定の`env`の`${VAR}`を展開したもの）。
/// 参照先が見つからない値は渡さない（APIキーの代わりに"[ERROR:...]"
/// という文字列を渡さないため）。名前順。
fn plugin_env(config: &PluginConfig) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for (key, value) in &config.env {
        let expanded = McpConfig::expand_env_vars(value);
        if expanded.contains("[ERROR:") {
            warn!(
                "Plugin '{}': environment variable '{}' is not passed because a variable it refers to is not set",
                config.name, key
            );
            continue;
        }
        env.push((key.clone(), expanded));
    }
    env.sort();
    env
}

/// コマンドラインのオプションの値として使う文字列を検証する。
fn validate_option_value(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.starts_with('-')
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(format!(
            "{} '{}' is not valid (it must not be empty, start with '-' or contain spaces)",
            field, value
        ));
    }
    Ok(())
}

/// `--mount`の値はカンマ区切りのため、パスにカンマ・引用符は使えない。
fn validate_mount_path(field: &str, path: &str) -> Result<(), String> {
    if path.is_empty()
        || path
            .chars()
            .any(|c| matches!(c, ',' | '"' | '\'') || c.is_control())
    {
        return Err(format!(
            "container mount {} '{}' must not be empty or contain commas or quotes",
            field, path
        ));
    }
    Ok(())
}

/// コンテナで起動するための、dockerコマンドの引数
#[derive(Debug)]
struct ContainerLaunch {
    program: String,
    args: Vec<String>,
    container_name: String,
}

/// `docker run`の引数を組み立てる。`env_keys`は、値をdockerコマンドの
/// 環境変数で渡す変数名（コマンドラインには名前だけを載せ、値は他の
/// ユーザーからプロセス一覧で見えないようにする）。
fn container_launch(
    config: &PluginConfig,
    container: &PluginContainerConfig,
    env_keys: &[&str],
    instance: &str,
) -> Result<ContainerLaunch, String> {
    let program = container
        .docker_command
        .as_deref()
        .unwrap_or(DEFAULT_DOCKER_COMMAND)
        .trim()
        .to_string();
    if program.is_empty() {
        return Err("container.docker_command is empty".to_string());
    }
    validate_option_value("container.image", container.image.trim())?;
    let network = container
        .network
        .as_deref()
        .unwrap_or(DEFAULT_CONTAINER_NETWORK);
    validate_option_value("container.network", network)?;
    if !network
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(format!(
            "container.network '{}' may only contain letters, digits, '_', '.' and '-'",
            network
        ));
    }
    if network == "host" {
        warn!(
            "Plugin '{}' uses the host network: it can reach every service on this machine and the network",
            config.name
        );
    }
    let memory = container
        .memory
        .as_deref()
        .unwrap_or(DEFAULT_CONTAINER_MEMORY);
    validate_option_value("container.memory", memory)?;

    let container_name = format!("mcp-rs-{}-{}", config.name, instance);
    let mut args: Vec<String> = vec![
        "run".into(),
        "-i".into(),
        "--rm".into(),
        "--init".into(),
        "--pull=never".into(),
        format!("--name={}", container_name),
        format!("--label={}={}", CONTAINER_LABEL, config.name),
        format!("--network={}", network),
        "--cap-drop=ALL".into(),
        "--security-opt=no-new-privileges".into(),
    ];
    if container.read_only.unwrap_or(true) {
        args.push("--read-only".into());
        args.push(format!("--tmpfs={}", CONTAINER_TMPFS));
    }
    args.push(format!("--memory={}", memory));
    args.push(format!(
        "--pids-limit={}",
        container.pids_limit.unwrap_or(DEFAULT_CONTAINER_PIDS_LIMIT)
    ));
    if let Some(cpus) = &container.cpus {
        validate_option_value("container.cpus", cpus)?;
        args.push(format!("--cpus={}", cpus));
    }
    if let Some(user) = &container.user {
        validate_option_value("container.user", user)?;
        args.push(format!("--user={}", user));
    }
    if let Some(dir) = &config.working_dir {
        if !dir.starts_with('/') {
            return Err(format!(
                "working_dir '{}' must be a path in the container (starting with '/')",
                dir
            ));
        }
        args.push(format!("--workdir={}", dir));
    }
    for key in env_keys {
        if key.is_empty() || key.contains('=') || key.chars().any(char::is_whitespace) {
            return Err(format!("environment variable name '{}' is not valid", key));
        }
        if is_docker_cli_env_var(key) {
            return Err(format!(
                "environment variable '{}' cannot be passed to a container plugin (mcp-rs passes it to the docker command itself)",
                key
            ));
        }
        args.push(format!("--env={}", key));
    }
    for mount in &container.mounts {
        validate_mount_path("source", &mount.source)?;
        validate_mount_path("target", &mount.target)?;
        if !std::path::Path::new(&mount.source).is_absolute() {
            return Err(format!(
                "container mount source '{}' must be an absolute path",
                mount.source
            ));
        }
        if !std::path::Path::new(&mount.source).exists() {
            return Err(format!(
                "container mount source '{}' does not exist",
                mount.source
            ));
        }
        if !mount.target.starts_with('/') || mount.target == "/" {
            return Err(format!(
                "container mount target '{}' must be a path in the container (starting with '/', not '/' itself)",
                mount.target
            ));
        }
        let mut value = format!("type=bind,source={},target={}", mount.source, mount.target);
        if mount.read_only.unwrap_or(true) {
            value.push_str(",readonly");
        }
        args.push(format!("--mount={}", value));
    }
    args.push(container.image.trim().to_string());
    if !config.command.trim().is_empty() {
        args.push(config.command.clone());
    }
    args.extend(config.args.iter().cloned());
    Ok(ContainerLaunch {
        program,
        args,
        container_name,
    })
}

/// プラグインを止めたときに、コンテナ・Jobを消すコマンド。dockerや
/// kubectlのプロセスを止めても、コンテナ・Jobが残ることがないように、
/// 終了時に実行する（`docker rm -f`・`kubectl delete job`）。
struct Cleanup {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

impl Cleanup {
    /// コマンドを実行する（終了を待たない）。既に消えていれば何もしない。
    fn run(self) {
        let mut command = std::process::Command::new(&self.program);
        command
            .args(&self.args)
            .env_clear()
            .envs(self.env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        // 終了を待って回収するため、別スレッドで実行する
        std::thread::spawn(move || {
            let _ = command.status();
        });
    }
}

/// dockerコマンド自体に渡す、本体の環境変数
fn docker_cli_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(key, _)| is_docker_cli_env_var(key))
        .collect()
}

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
/// だけにする）。ツール呼び出しは`audit_logger`に記録する（本体の他の
/// ハンドラーと同じものを渡す、#416）。
pub async fn start_plugins(
    configs: &[PluginConfig],
    audit_logger: Arc<AuditLogger>,
) -> Vec<(String, Arc<PluginHandler>)> {
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
        match PluginHandler::start_with_audit_logger(config, audit_logger.clone()).await {
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
    /// コンテナ・Kubernetesで起動した場合に、終了時に実行する後片付け
    cleanup: Option<Cleanup>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.kill();
    }
}

impl Connection {
    /// プロセスを止め、コンテナ・Jobで起動していればそれも消す。
    fn kill(&mut self) {
        let _ = self.child.start_kill();
        if let Some(cleanup) = self.cleanup.take() {
            cleanup.run();
        }
    }

    fn not_running_error(&self, reason: &str) -> McpError {
        McpError::ExternalApi(format!(
            "plugin '{}' is not running ({}). Restart mcp-rs to start it again",
            self.plugin, reason
        ))
    }

    fn stop(&mut self, reason: String) {
        warn!("Plugin '{}' stopped: {}", self.plugin, reason);
        self.kill();
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

/// プラグインのツールの結果（CallToolResult）から、仕様（2025-06-18）で
/// 定義された項目だけを残す。クライアントと話している版に合わせて、さらに
/// 項目を除くのは`McpServer`（`call_tool_result_for_version`、#417）。
fn call_tool_result_for_client(plugin: &str, result: Value) -> Result<Value, McpError> {
    const KEPT_FIELDS: &[&str] = &["content", "structuredContent", "isError", "_meta"];

    let Value::Object(mut fields) = result else {
        return Err(McpError::ExternalApi(format!(
            "plugin '{}' returned an invalid tool result",
            plugin
        )));
    };
    fields.retain(|key, _| KEPT_FIELDS.contains(&key.as_str()));
    if !fields.get("content").is_some_and(Value::is_array) {
        return Err(McpError::ExternalApi(format!(
            "plugin '{}' returned a tool result without content",
            plugin
        )));
    }
    Ok(Value::Object(fields))
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
    audit_logger: Arc<AuditLogger>,
}

impl PluginHandler {
    /// 子プロセス（またはコンテナ）を起動し、`initialize`・`tools/list`
    /// まで行う。監査ログはメモリにだけ残す（ファイルに書くには
    /// `start_with_audit_logger`）。
    pub async fn start(config: &PluginConfig) -> Result<Self, String> {
        Self::start_with_audit_logger(config, Arc::new(AuditLogger::with_defaults())).await
    }

    /// `start`と同じ。ツール呼び出しを`audit_logger`に記録する。
    pub async fn start_with_audit_logger(
        config: &PluginConfig,
        audit_logger: Arc<AuditLogger>,
    ) -> Result<Self, String> {
        validate_plugin_name(&config.name)?;
        if config.container.is_some() && config.kubernetes.is_some() {
            return Err("container and kubernetes cannot both be set".to_string());
        }
        if config.container.is_none()
            && config.kubernetes.is_none()
            && config.command.trim().is_empty()
        {
            return Err("command is empty".to_string());
        }
        let result = Self::start_inner(config, audit_logger).await;
        result.map_err(|e| {
            if config.container.is_some() {
                format!(
                    "{} (docker's own message, if any, is in the log lines starting with \"[plugin {}]\". The image must be pulled beforehand with `docker pull`)",
                    e, config.name
                )
            } else if config.kubernetes.is_some() {
                format!(
                    "{} (kubectl's own message, if any, is in the log lines starting with \"[plugin {}]\")",
                    e, config.name
                )
            } else {
                e
            }
        })
    }

    async fn start_inner(
        config: &PluginConfig,
        audit_logger: Arc<AuditLogger>,
    ) -> Result<Self, String> {
        let call_timeout = Duration::from_secs(
            config
                .call_timeout_seconds
                .unwrap_or(DEFAULT_CALL_TIMEOUT_SECS),
        );
        let max_response_bytes = config
            .max_response_bytes
            .unwrap_or(DEFAULT_MAX_RESPONSE_BYTES);

        let env = plugin_env(config);
        let mut cleanup = None;
        let mut start_timeout = call_timeout;
        let mut command = match (&config.container, &config.kubernetes) {
            (None, Some(kubernetes)) => {
                let prepared = kubernetes::prepare(config, kubernetes, &env).await?;
                start_timeout = prepared.start_timeout;
                cleanup = Some(prepared.cleanup);
                prepared.command
            }
            (None, None) => {
                let mut command = Command::new(&config.command);
                command.args(&config.args).env_clear();
                for key in INHERITED_ENV_VARS {
                    if let Ok(value) = std::env::var(key) {
                        command.env(key, value);
                    }
                }
                command.envs(env);
                if let Some(dir) = &config.working_dir {
                    command.current_dir(dir);
                }
                command
            }
            (Some(container), _) => {
                let env_keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
                let instance = uuid::Uuid::new_v4().simple().to_string();
                let launch = container_launch(config, container, &env_keys, &instance[..12])?;
                info!(
                    "Plugin '{}' runs in container '{}' (image '{}', network '{}')",
                    config.name,
                    launch.container_name,
                    container.image.trim(),
                    container
                        .network
                        .as_deref()
                        .unwrap_or(DEFAULT_CONTAINER_NETWORK)
                );
                let docker_env = docker_cli_env();
                let mut command = Command::new(&launch.program);
                command
                    .args(&launch.args)
                    .env_clear()
                    .envs(docker_env.clone())
                    // `--env=NAME`の値は、dockerコマンドの環境変数から渡る
                    .envs(env);
                cleanup = Some(Cleanup {
                    program: launch.program,
                    args: vec!["rm".into(), "-f".into(), launch.container_name],
                    env: docker_env,
                });
                command
            }
        };
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

        let program = command
            .as_std()
            .get_program()
            .to_string_lossy()
            .into_owned();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                // Kubernetesでは、Job等を作った後に失敗しうる
                if let Some(cleanup) = cleanup {
                    cleanup.run();
                }
                return Err(format!("could not start '{}': {}", program, e));
            }
        };
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            if let Some(cleanup) = cleanup {
                cleanup.run();
            }
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
            // 起動に失敗して`connection`を破棄するときも、コンテナ・Jobを消す
            cleanup,
        };

        let raw_tools = match tokio::time::timeout(
            start_timeout,
            Self::handshake(&mut connection, max_response_bytes),
        )
        .await
        {
            Ok(Ok(tools)) => tools,
            Ok(Err(e)) => {
                connection.kill();
                return Err(e.to_string());
            }
            Err(_) => {
                connection.kill();
                return Err(format!(
                    "the plugin did not finish starting within {} seconds",
                    start_timeout.as_secs()
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
                // 新しい版の項目（#417）。クライアントと話している版に
                // 無い項目は、`McpServer`が除く
                title: raw.get("title").and_then(Value::as_str).map(str::to_string),
                output_schema: raw.get("outputSchema").filter(|v| v.is_object()).cloned(),
                annotations: raw
                    .get("annotations")
                    .and_then(|v| serde_json::from_value::<ToolAnnotations>(v.clone()).ok()),
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
            audit_logger,
        })
    }

    async fn handshake(
        connection: &mut Connection,
        max_bytes: usize,
    ) -> Result<Vec<Value>, McpError> {
        let initialized = connection
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
        let version = initialized
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        if KNOWN_PLUGIN_PROTOCOL_VERSIONS.contains(&version) {
            debug!(
                "Plugin '{}' uses MCP protocol version {}",
                connection.plugin, version
            );
        } else {
            warn!(
                "Plugin '{}' answered with MCP protocol version '{}', which mcp-rs does not know. Continuing anyway",
                connection.plugin, version
            );
        }
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

    async fn audit_tool_call(
        &self,
        tool_name: &str,
        result: &Result<Value, McpError>,
        started: std::time::Instant,
    ) {
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
        // 呼び出し元と所要時間（#55）
        let entry = crate::mcp::audit_tool_call_context(entry, started);
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

        call_tool_result_for_client(&self.name, result)
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
        let started = std::time::Instant::now();
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
        self.audit_tool_call(&params.name, &result, started).await;
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
    use crate::config::{PluginKubernetesConfig, PluginMountConfig};

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
                        let mut tool = json!({
                            "name": name,
                            "description": format!("{} tool", name),
                            "inputSchema": { "type": "object" }
                        });
                        if name == "echo" {
                            // 新しい版（2025-06-18）の項目
                            tool["title"] = json!("Echo");
                            tool["outputSchema"] = json!({ "type": "object" });
                            tool["annotations"] = json!({ "readOnlyHint": true });
                        }
                        tool
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
                        "echo" => {
                            // 新しい仕様のサーバーと同じく、structuredContentも返す
                            let text = args["text"].as_str().unwrap_or("").to_string();
                            json!({
                                "content": [{ "type": "text", "text": text }],
                                "structuredContent": { "content": text },
                                "isError": false,
                                "_meta": { "note": "kept" },
                                "somethingElse": 1
                            })
                        }
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
            container: None,
            kubernetes: None,
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
    fn test_call_tool_result_for_client() {
        let filtered = call_tool_result_for_client(
            "p",
            json!({
                "content": [{ "type": "text", "text": "[FILE] test.txt" }],
                "structuredContent": { "content": "[FILE] test.txt" }
            }),
        )
        .unwrap();
        // structuredContentは残す（クライアントの版に合わせて除くのはMcpServer）
        assert_eq!(
            filtered,
            json!({
                "content": [{ "type": "text", "text": "[FILE] test.txt" }],
                "structuredContent": { "content": "[FILE] test.txt" }
            })
        );

        for invalid in [
            json!("text"),
            json!({}),
            json!({ "content": "not an array" }),
        ] {
            assert!(call_tool_result_for_client("p", invalid).is_err());
        }
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
        // 新しい版の項目も中継する（#417）
        assert_eq!(tools[0].title.as_deref(), Some("Echo"));
        assert_eq!(tools[0].output_schema, Some(json!({ "type": "object" })));
        assert_eq!(
            tools[0].annotations.as_ref().and_then(|a| a.read_only_hint),
            Some(true)
        );
        assert!(tools[1].annotations.is_none());

        let result = call(&handler, "demo__echo", json!({ "text": "hello" }))
            .await
            .unwrap();
        assert_eq!(text(&result), "hello");
        // CallToolResultの項目だけを返す（仕様に無い項目は除く）
        let mut keys: Vec<&String> = result.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["_meta", "content", "isError", "structuredContent"]
        );

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

        let started = start_plugins(
            &[missing, bad_name, disabled, good, duplicate],
            Arc::new(AuditLogger::with_defaults()),
        )
        .await;
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

    fn container_config(image: &str) -> PluginContainerConfig {
        PluginContainerConfig {
            image: image.to_string(),
            network: None,
            mounts: Vec::new(),
            memory: None,
            cpus: None,
            pids_limit: None,
            read_only: None,
            user: None,
            docker_command: None,
        }
    }

    fn container_plugin_config(name: &str, container: PluginContainerConfig) -> PluginConfig {
        PluginConfig {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            container: Some(container),
            ..test_plugin_config(name)
        }
    }

    fn mount_source() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    #[test]
    fn test_container_launch_isolates_by_default() {
        let mut container = container_config("example/fs:1.0");
        container.mounts = vec![
            PluginMountConfig {
                source: mount_source(),
                target: "/data".to_string(),
                read_only: None,
            },
            PluginMountConfig {
                source: mount_source(),
                target: "/out".to_string(),
                read_only: Some(false),
            },
        ];
        let mut config = container_plugin_config("fs", container.clone());
        config.args = vec!["/data".to_string()];
        let launch = container_launch(&config, &container, &["API_KEY", "MODE"], "abc123").unwrap();

        assert_eq!(launch.program, "docker");
        assert_eq!(launch.container_name, "mcp-rs-fs-abc123");
        let source = mount_source();
        let expected: Vec<String> = [
            "run",
            "-i",
            "--rm",
            "--init",
            "--pull=never",
            "--name=mcp-rs-fs-abc123",
            "--label=mcp-rs.plugin=fs",
            "--network=none",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--read-only",
            "--tmpfs=/tmp:rw,noexec,nosuid,size=64m",
            "--memory=512m",
            "--pids-limit=256",
            // 値はコマンドラインに載せない
            "--env=API_KEY",
            "--env=MODE",
            &format!("--mount=type=bind,source={},target=/data,readonly", source),
            &format!("--mount=type=bind,source={},target=/out", source),
            "example/fs:1.0",
            // commandを省略すると、argsはイメージのENTRYPOINTに渡る
            "/data",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(launch.args, expected);
    }

    #[test]
    fn test_container_launch_applies_options() {
        let mut container = container_config("python:3.12-alpine");
        container.network = Some("bridge".to_string());
        container.memory = Some("1g".to_string());
        container.cpus = Some("0.5".to_string());
        container.pids_limit = Some(64);
        container.read_only = Some(false);
        container.user = Some("1000:1000".to_string());
        container.docker_command = Some("podman".to_string());
        let mut config = container_plugin_config("py", container.clone());
        config.command = "python".to_string();
        config.args = vec!["-c".to_string(), "print(1)".to_string()];
        config.working_dir = Some("/work".to_string());
        let launch = container_launch(&config, &container, &[], "x").unwrap();

        assert_eq!(launch.program, "podman");
        for expected in [
            "--network=bridge",
            "--memory=1g",
            "--cpus=0.5",
            "--pids-limit=64",
            "--user=1000:1000",
            "--workdir=/work",
        ] {
            assert!(launch.args.iter().any(|a| a == expected), "{}", expected);
        }
        assert!(!launch.args.iter().any(|a| a == "--read-only"));
        assert_eq!(
            launch.args[launch.args.len() - 4..],
            ["python:3.12-alpine", "python", "-c", "print(1)"]
        );
    }

    #[test]
    fn test_container_launch_rejects_invalid_settings() {
        let check = |modify: &dyn Fn(&mut PluginConfig, &mut PluginContainerConfig),
                     env_keys: &[&str],
                     expected: &str| {
            let mut container = container_config("example/image");
            let mut config = container_plugin_config("bad", container.clone());
            modify(&mut config, &mut container);
            let error = container_launch(&config, &container, env_keys, "x").unwrap_err();
            assert!(error.contains(expected), "{}: {}", expected, error);
        };
        check(&|_, c| c.image = String::new(), &[], "image");
        check(&|_, c| c.image = "--privileged".to_string(), &[], "image");
        check(&|_, c| c.image = "a b".to_string(), &[], "image");
        check(
            &|_, c| c.network = Some("container:other".to_string()),
            &[],
            "network",
        );
        check(&|_, c| c.memory = Some("-1".to_string()), &[], "memory");
        check(
            &|_, c| c.docker_command = Some(" ".to_string()),
            &[],
            "docker_command",
        );
        check(
            &|cfg, _| cfg.working_dir = Some("relative".to_string()),
            &[],
            "working_dir",
        );
        // dockerコマンド自体の環境変数と区別できない名前
        check(&|_, _| {}, &["PATH"], "PATH");
        check(&|_, _| {}, &["DOCKER_HOST"], "DOCKER_HOST");
        check(&|_, _| {}, &["A=B"], "not valid");
        let mount = |source: String, target: &str| PluginMountConfig {
            source,
            target: target.to_string(),
            read_only: None,
        };
        check(
            &|_, c| c.mounts = vec![mount("relative/dir".to_string(), "/data")],
            &[],
            "absolute",
        );
        check(
            &|_, c| c.mounts = vec![mount(format!("{},readonly=false", mount_source()), "/data")],
            &[],
            "commas",
        );
        check(
            &|_, c| c.mounts = vec![mount(mount_source(), "data")],
            &[],
            "target",
        );
        check(
            &|_, c| c.mounts = vec![mount(mount_source(), "/")],
            &[],
            "target",
        );
        let missing = std::env::temp_dir().join("mcp-rs-surely-missing-mount-source");
        check(
            &|_, c| c.mounts = vec![mount(missing.to_string_lossy().into_owned(), "/data")],
            &[],
            "does not exist",
        );
    }

    #[tokio::test]
    async fn test_container_plugin_without_docker_fails_to_start_with_hint() {
        let mut container = container_config("example/image");
        container.docker_command = Some("mcp-rs-surely-missing-docker-command".to_string());
        let error = PluginHandler::start(&container_plugin_config("nodocker", container))
            .await
            .err()
            .unwrap();
        assert!(error.contains("could not start"), "{}", error);
        assert!(error.contains("docker pull"), "{}", error);
    }

    /// 実際のDockerでの隔離の確認。Docker（Linuxコンテナ）が必要なため、
    /// `MCP_RS_DOCKER_TESTS=1`のときだけ実行する（CIのLinuxで実行する）。
    const DOCKER_TEST_IMAGE: &str = "python:3.12-alpine";
    const DOCKER_TEST_SERVER: &str = r#"
import json, os, socket, sys

def reply(i, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": i, "result": result}) + "\n")
    sys.stdout.flush()

def attempt(action):
    try:
        action()
        return "ok"
    except Exception:
        return "blocked"

def write(path):
    with open(path, "w") as f:
        f.write("x")

def read(path):
    try:
        with open(path) as f:
            return f.read()
    except Exception:
        return None

def connect_api():
    # Kubernetes内では、APIサーバー（クラスタ内の通信先）に接続してみる
    host = os.environ.get("KUBERNETES_SERVICE_HOST")
    if not host:
        return "n/a"
    port = int(os.environ.get("KUBERNETES_SERVICE_PORT", "443"))
    return attempt(lambda: socket.create_connection((host, port), timeout=3).close())

for line in sys.stdin:
    message = json.loads(line)
    if "id" not in message:
        continue
    method = message.get("method")
    if method == "initialize":
        reply(message["id"], {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                              "serverInfo": {"name": "docker-test", "version": "1"}})
    elif method == "tools/list":
        reply(message["id"], {"tools": [{"name": "probe", "inputSchema": {"type": "object"}}]})
    elif method == "tools/call":
        cap_eff = [l.split()[1] for l in open("/proc/self/status") if l.startswith("CapEff")][0]
        probe = {
            "env": sorted(os.environ),
            "token": os.environ.get("PLUGIN_TOKEN"),
            "network": attempt(lambda: socket.create_connection(("1.1.1.1", 53), timeout=3).close()),
            "write_root": attempt(lambda: write("/probe")),
            "write_tmp": attempt(lambda: write("/tmp/probe")),
            "network_api": connect_api(),
            "read_mount": read("/data/hello.txt"),
            "write_mount": attempt(lambda: write("/data/probe")),
            "cap_eff": cap_eff,
            "uid": os.getuid(),
            "sa_token": os.path.exists("/var/run/secrets/kubernetes.io/serviceaccount/token"),
        }
        reply(message["id"], {"content": [{"type": "text", "text": json.dumps(probe)}]})
    else:
        reply(message["id"], {})
"#;

    fn docker_tests_enabled() -> bool {
        std::env::var("MCP_RS_DOCKER_TESTS").ok().as_deref() == Some("1")
    }

    fn docker_containers_with_label(plugin: &str) -> String {
        let output = std::process::Command::new("docker")
            .args([
                "ps",
                "-a",
                "-q",
                "--filter",
                &format!("label={}={}", CONTAINER_LABEL, plugin),
            ])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[tokio::test]
    async fn test_container_plugin_is_isolated_in_docker() {
        if !docker_tests_enabled() {
            return;
        }
        let pulled = std::process::Command::new("docker")
            .args(["pull", "-q", DOCKER_TEST_IMAGE])
            .status()
            .unwrap();
        assert!(pulled.success());

        let data_dir = std::env::temp_dir().join(format!(
            "mcp-rs-docker-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("hello.txt"), "hello from host").unwrap();

        let mut container = container_config(DOCKER_TEST_IMAGE);
        container.mounts = vec![PluginMountConfig {
            source: data_dir.to_string_lossy().into_owned(),
            target: "/data".to_string(),
            read_only: None,
        }];
        let mut config = container_plugin_config("dockertest", container);
        config.command = "python".to_string();
        config.args = vec![
            "-u".to_string(),
            "-c".to_string(),
            DOCKER_TEST_SERVER.to_string(),
        ];
        config
            .env
            .insert("PLUGIN_TOKEN".to_string(), "secret-value".to_string());
        config.call_timeout_seconds = Some(60);
        let handler = PluginHandler::start(&config).await.unwrap();

        let result = call(&handler, "dockertest__probe", json!({}))
            .await
            .unwrap();
        let probe: Value = serde_json::from_str(text(&result)).unwrap();
        assert_eq!(probe["token"], json!("secret-value"));
        // 本体（テストの実行環境）の環境変数は渡らない
        let env: Vec<&str> = probe["env"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for (host_key, _) in std::env::vars() {
            if host_key.starts_with("CARGO") || host_key.starts_with("MCP_RS") {
                assert!(!env.contains(&host_key.as_str()), "{} leaked", host_key);
            }
        }
        assert_eq!(probe["network"], json!("blocked"));
        assert_eq!(probe["write_root"], json!("blocked"));
        assert_eq!(probe["write_tmp"], json!("ok"));
        assert_eq!(probe["read_mount"], json!("hello from host"));
        assert_eq!(probe["write_mount"], json!("blocked"));
        assert_eq!(probe["cap_eff"], json!("0000000000000000"));

        // 止めると、コンテナも残らない
        assert!(!docker_containers_with_label("dockertest").is_empty());
        drop(handler);
        let mut remaining = String::new();
        for _ in 0..50 {
            remaining = docker_containers_with_label("dockertest");
            if remaining.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        assert!(remaining.is_empty(), "containers left: {}", remaining);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    fn kubernetes_plugin_config(name: &str, kubernetes: PluginKubernetesConfig) -> PluginConfig {
        PluginConfig {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            kubernetes: Some(kubernetes),
            ..test_plugin_config(name)
        }
    }

    fn kubernetes_config(image: &str) -> PluginKubernetesConfig {
        PluginKubernetesConfig {
            image: image.to_string(),
            namespace: None,
            context: None,
            network: None,
            memory: None,
            cpu: None,
            run_as_user: None,
            read_only: None,
            image_pull_policy: None,
            start_timeout_seconds: None,
            kubectl_command: None,
        }
    }

    #[tokio::test]
    async fn test_kubernetes_plugin_without_kubectl_fails_to_start_with_hint() {
        let mut kubernetes = kubernetes_config("example/image");
        kubernetes.kubectl_command = Some("mcp-rs-surely-missing-kubectl-command".to_string());
        let error = PluginHandler::start(&kubernetes_plugin_config("nokubectl", kubernetes))
            .await
            .err()
            .unwrap();
        assert!(error.contains("could not start"), "{}", error);
        assert!(error.contains("kubectl's own message"), "{}", error);
    }

    #[tokio::test]
    async fn test_plugin_with_both_container_and_kubernetes_is_rejected() {
        let mut config = kubernetes_plugin_config("both", kubernetes_config("example/image"));
        config.container = Some(container_config("example/image"));
        let error = PluginHandler::start(&config).await.err().unwrap();
        assert!(error.contains("cannot both be set"), "{}", error);
    }

    /// 実際のKubernetesでの隔離の確認。kubectlで使えるクラスタ（CIでは
    /// kind）が必要なため、`MCP_RS_K8S_TESTS=1`のときだけ実行する。
    fn kubernetes_tests_enabled() -> bool {
        std::env::var("MCP_RS_K8S_TESTS").ok().as_deref() == Some("1")
    }

    fn kubernetes_resources_with_label(plugin: &str) -> String {
        let output = std::process::Command::new("kubectl")
            .args([
                "get",
                "jobs,secrets",
                "-o",
                "name",
                &format!("--selector={}={}", CONTAINER_LABEL, plugin),
            ])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    async fn start_kubernetes_probe(name: &str, network: Option<&str>) -> PluginHandler {
        let mut kubernetes = kubernetes_config(DOCKER_TEST_IMAGE);
        kubernetes.network = network.map(str::to_string);
        kubernetes.image_pull_policy = Some("IfNotPresent".to_string());
        let mut config = kubernetes_plugin_config(name, kubernetes);
        config.command = "python".to_string();
        config.args = vec![
            "-u".to_string(),
            "-c".to_string(),
            DOCKER_TEST_SERVER.to_string(),
        ];
        config
            .env
            .insert("PLUGIN_TOKEN".to_string(), "secret-value".to_string());
        config.call_timeout_seconds = Some(30);
        PluginHandler::start(&config).await.unwrap()
    }

    async fn probe(handler: &PluginHandler, name: &str) -> Value {
        let result = call(handler, &format!("{}__probe", name), json!({}))
            .await
            .unwrap();
        serde_json::from_str(text(&result)).unwrap()
    }

    #[tokio::test]
    async fn test_kubernetes_plugin_is_isolated() {
        if !kubernetes_tests_enabled() {
            return;
        }
        let (isolated, allowed) = tokio::join!(
            start_kubernetes_probe("k8snone", None),
            start_kubernetes_probe("k8sallow", Some("allow"))
        );

        let result = probe(&isolated, "k8snone").await;
        eprintln!("probe (network = none): {}", result);
        assert_eq!(result["token"], json!("secret-value"));
        let env: Vec<&str> = result["env"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for (host_key, _) in std::env::vars() {
            if host_key.starts_with("CARGO") || host_key.starts_with("MCP_RS") {
                assert!(!env.contains(&host_key.as_str()), "{} leaked", host_key);
            }
        }
        // NetworkPolicyで、外に出られない。Podが動くノード自体への通信
        // （kindでは同じノードのAPIサーバー）を止めるかはネットワーク
        // プラグイン次第のため、ここでは確かめない
        assert_eq!(result["network"], json!("blocked"));
        assert_eq!(result["write_root"], json!("blocked"));
        assert_eq!(result["write_tmp"], json!("ok"));
        assert_eq!(result["cap_eff"], json!("0000000000000000"));
        assert_eq!(result["uid"], json!(65532));
        assert_eq!(result["sa_token"], json!(false));

        // network = "allow"なら通信できる（拒否がNetworkPolicyによることの確認）
        let result = probe(&allowed, "k8sallow").await;
        eprintln!("probe (network = allow): {}", result);
        assert_eq!(result["network"], json!("ok"));

        // 止めると、JobとSecretも残らない
        assert!(!kubernetes_resources_with_label("k8snone").is_empty());
        drop(isolated);
        drop(allowed);
        for plugin in ["k8snone", "k8sallow"] {
            let mut remaining = String::new();
            for _ in 0..60 {
                remaining = kubernetes_resources_with_label(plugin);
                if remaining.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            assert!(remaining.is_empty(), "left for {}: {}", plugin, remaining);
        }
    }
}
