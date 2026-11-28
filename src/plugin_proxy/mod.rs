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
/// プラグインが答えてよい版。これ以外は、`notifications/initialized`を送る
/// 前に起動のエラーにする（仕様では、クライアントは切断すべき、#750）
const KNOWN_PLUGIN_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// 上限を超えて読み捨てる行のうち、`id`を読むために残す先頭の長さ（#750）
const OVERSIZED_LINE_PREFIX_BYTES: usize = 1024;
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

/// 許可した行き先にだけ通信させる`container.network`の値（#615）
const ALLOWLIST_NETWORK: &str = "allowlist";

/// 中継のプロキシが外に出るネットワーク（Dockerの既定のブリッジ）
const EGRESS_NETWORK: &str = "bridge";

/// 中継のプロキシの既定のイメージ（このmcp-rsと同じ版）
const DEFAULT_EGRESS_PROXY_IMAGE: &str =
    concat!("ghcr.io/n-takatsu/mcp-rs:", env!("CARGO_PKG_VERSION"));

/// `network = "allowlist"`のプラグインの名前の長さの上限。プロキシの
/// コンテナの名前（`mcp-rs-`・`-`・12文字・`-egress`の27文字を足す）が、
/// DNSのラベルの上限の63文字に収まるように
const MAX_PLUGIN_NAME_LEN_WITH_ALLOWLIST: usize = 63 - 27;

/// 中継のプロキシの使ってよいメモリ・プロセス数
const EGRESS_PROXY_MEMORY: &str = "64m";
const EGRESS_PROXY_PIDS_LIMIT: u32 = 64;

/// プラグインにプロキシを教える環境変数。`env`では渡せない
const PROXY_ENV_VARS: &[&str] = &["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];

fn is_docker_cli_env_var(key: &str) -> bool {
    DOCKER_CLI_ENV_VARS
        .iter()
        .any(|k| k.eq_ignore_ascii_case(key))
        || key.to_ascii_uppercase().starts_with("DOCKER_")
}

/// プラグインに渡す環境変数（設定の`env`の`${VAR}`を展開したもの）。
/// 参照先が見つからない値・`${`の書き方がおかしい値は渡さない（#454）。
/// 名前順。
fn plugin_env(config: &PluginConfig) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for (key, value) in &config.env {
        match McpConfig::expand_env_vars(value) {
            Ok(expanded) => env.push((key.clone(), expanded)),
            Err(e) => warn!(
                "Plugin '{}': environment variable '{}' is not passed ({})",
                config.name, key, e
            ),
        }
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
    /// `network = "allowlist"`のとき、内部のネットワークと中継のプロキシ
    egress: Option<EgressLaunch>,
}

/// `network = "allowlist"`（#615）で、プラグインの前に用意するもの
#[derive(Debug)]
struct EgressLaunch {
    /// プラグインを入れる、外に出られない内部のネットワーク
    network: String,
    /// 中継のプロキシのコンテナの名前
    proxy_name: String,
    /// 中継のプロキシを起動する`docker run`の引数
    proxy_args: Vec<String>,
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
    let allowlist = network == ALLOWLIST_NETWORK;
    if !allowlist && container.allowed_hosts.is_some() {
        return Err(format!(
            "container.allowed_hosts needs network = \"{ALLOWLIST_NETWORK}\""
        ));
    }
    if !allowlist && container.egress_proxy_image.is_some() {
        return Err(format!(
            "container.egress_proxy_image needs network = \"{ALLOWLIST_NETWORK}\""
        ));
    }
    let memory = container
        .memory
        .as_deref()
        .unwrap_or(DEFAULT_CONTAINER_MEMORY);
    validate_option_value("container.memory", memory)?;

    let container_name = format!("mcp-rs-{}-{}", config.name, instance);
    let egress = if allowlist {
        Some(egress_launch(config, container, &container_name)?)
    } else {
        None
    };
    let network = egress
        .as_ref()
        .map_or(network, |egress| egress.network.as_str());
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
        if egress.is_some() && PROXY_ENV_VARS.contains(key) {
            return Err(format!(
                "environment variable '{}' cannot be set with network = \"{ALLOWLIST_NETWORK}\" (mcp-rs sets it to its proxy)",
                key
            ));
        }
        args.push(format!("--env={}", key));
    }
    if let Some(egress) = &egress {
        // プロキシの名前は、内部のネットワークでDockerが引く。値は秘密ではない
        for key in PROXY_ENV_VARS {
            args.push(format!(
                "--env={}=http://{}:{}",
                key,
                egress.proxy_name,
                crate::egress_proxy::LISTEN_PORT
            ));
        }
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
        egress,
    })
}

/// `network = "allowlist"`の内部のネットワークと中継のプロキシ（#615）。
/// プロキシは、内部のネットワークと`bridge`の両方につなぎ（`--network`を
/// 2つ。Docker 25以降）、プラグインと同じく、ケーパビリティを外し、読み取り
/// 専用で動かす
fn egress_launch(
    config: &PluginConfig,
    container: &PluginContainerConfig,
    container_name: &str,
) -> Result<EgressLaunch, String> {
    // プロキシのコンテナの名前（`mcp-rs-<名前>-<12文字>-egress`）は、
    // プラグインが`HTTPS_PROXY`で引くホスト名になる。DNSのラベルの上限
    // （63文字）に収まる名前だけにする（PR #828のレビュー）。`mcp-rs check`
    // （`instance`が短い）も同じ上限で確かめる
    if config.name.len() > MAX_PLUGIN_NAME_LEN_WITH_ALLOWLIST {
        return Err(format!(
            "plugin name '{}' must be at most {} characters with network = \"{ALLOWLIST_NETWORK}\" (the proxy's container name is used as a host name)",
            config.name, MAX_PLUGIN_NAME_LEN_WITH_ALLOWLIST
        ));
    }
    let hosts = container.allowed_hosts.as_deref().unwrap_or_default();
    crate::egress_proxy::parse_allowed_hosts(hosts).map_err(|e| {
        format!("container.{e} (network = \"{ALLOWLIST_NETWORK}\" needs allowed_hosts, such as [\"api.example.com:443\"])")
    })?;
    let image = container
        .egress_proxy_image
        .as_deref()
        .unwrap_or(DEFAULT_EGRESS_PROXY_IMAGE)
        .trim();
    validate_option_value("container.egress_proxy_image", image)?;
    let network = format!("{container_name}-net");
    let proxy_name = format!("{container_name}-egress");
    let mut proxy_args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--init".into(),
        "--pull=never".into(),
        format!("--name={proxy_name}"),
        format!("--label={}={}", CONTAINER_LABEL, config.name),
        format!("--network={network}"),
        format!("--network={EGRESS_NETWORK}"),
        "--cap-drop=ALL".into(),
        "--security-opt=no-new-privileges".into(),
        "--read-only".into(),
        format!("--memory={EGRESS_PROXY_MEMORY}"),
        format!("--pids-limit={EGRESS_PROXY_PIDS_LIMIT}"),
        image.to_string(),
        crate::egress_proxy::SUBCOMMAND.into(),
        "--listen".into(),
        format!("0.0.0.0:{}", crate::egress_proxy::LISTEN_PORT),
    ];
    for host in hosts {
        proxy_args.push("--allow".into());
        proxy_args.push(host.trim().to_string());
    }
    Ok(EgressLaunch {
        network,
        proxy_name,
        proxy_args,
    })
}

/// `docker`（か`docker_command`）の`Command`。コンソールのウィンドウを開かない
fn docker_command(program: &str, env: &[(String, String)]) -> Command {
    let mut command = Command::new(program);
    command.env_clear().envs(env.iter().cloned());
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    command
}

/// 内部のネットワークを作り、中継のプロキシを起動して、待ち受けを始めるまで
/// 待つ（#615）。返すプロキシの子プロセスは、プラグインと一緒に止める
async fn start_egress_proxy(
    plugin: &str,
    program: &str,
    env: &[(String, String)],
    egress: &EgressLaunch,
    timeout: Duration,
) -> Result<Child, String> {
    let created = docker_command(program, env)
        .args([
            "network".to_string(),
            "create".to_string(),
            "--internal".to_string(),
            format!("--label={CONTAINER_LABEL}={plugin}"),
            egress.network.clone(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("could not run `{program} network create`: {e}"))?;
    if !created.status.success() {
        return Err(format!(
            "could not create the internal network '{}' for allowed_hosts: {}",
            egress.network,
            truncate(String::from_utf8_lossy(&created.stderr).trim(), 300)
        ));
    }
    let mut proxy = docker_command(program, env)
        .args(&egress.proxy_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start the egress proxy with '{program}': {e}"))?;
    if let Some(stderr) = proxy.stderr.take() {
        spawn_stderr_logger(format!("{plugin}/egress"), stderr);
    }
    let stdout = proxy
        .stdout
        .take()
        .ok_or("could not read the egress proxy's output")?;
    let ready = tokio::time::timeout(timeout, async {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim() == crate::egress_proxy::READY_LINE {
                return true;
            }
        }
        false
    })
    .await;
    match ready {
        Ok(true) => Ok(proxy),
        Ok(false) => Err(format!(
            "the egress proxy exited before it was ready (its messages are in the log lines starting with \"[plugin {plugin}/egress]\". The image must be pulled beforehand with `docker pull`)"
        )),
        Err(_) => Err(format!(
            "the egress proxy did not start within {} seconds",
            timeout.as_secs()
        )),
    }
}

/// プラグインを止めたときに、コンテナ・Jobを消すコマンド。dockerや
/// kubectlのプロセスを止めても、コンテナ・Jobが残ることがないように、
/// 終了時に実行する（`docker rm -f`・`kubectl delete job`）。
struct Cleanup {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    /// 失敗の文面が「既に消えていた」なら、成功とみなす。`docker rm -f`・
    /// `podman rm -f`だけ。`kubectl delete`は`--ignore-not-found`で、既に無ければ
    /// 成功で終わるので、失敗はいつも知らせる（認証のコマンドの失敗の文面に
    /// `No such container`が入ることもあるため。PR #685のレビュー）
    ignore_already_removed: bool,
    /// 続けて実行する後片付け（`network = "allowlist"`の内部のネットワークを
    /// 消す等、#615）。このコマンドが失敗しても実行する
    then: Option<Box<Cleanup>>,
}

impl Cleanup {
    /// コマンドを実行する（終了を待たない）。既に消えていれば何もしない。
    fn run(self) {
        self.spawn(None);
    }

    /// コマンドを実行し、`delay`の後にもう1度実行する（終了を待たない）。
    /// 作る要求（`kubectl create`）の途中で打ち切ったとき、APIサーバーが
    /// 受け取っていた要求は、消した後に作り終えることがある。少し後で
    /// もう1度消して、残さない（PR #698のレビュー）。`--ignore-not-found`
    /// なので、2回目に消すものが無くても成功する
    fn run_twice(self, delay: std::time::Duration) {
        self.spawn(Some(delay));
    }

    fn spawn(self, again_after: Option<std::time::Duration>) {
        // 終了を待って回収するため、別スレッドで実行する。mcp-rsの終了前に
        // `wait_for_cleanups`で待てるよう、スレッドを覚えておく（PR #672の
        // レビュー）。失敗は、`wait_for_cleanups`が返す（#684）。2回実行する
        // ときは、最後の結果を返す
        let handle = std::thread::spawn(move || {
            let mut result = self.execute();
            if let Some(delay) = again_after {
                std::thread::sleep(delay);
                result = self.execute();
            }
            if let Err(error) = result {
                CLEANUP_FAILURES
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(error);
            }
        });
        let mut pending = PENDING_CLEANUPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.retain(|h| !h.is_finished());
        pending.push(handle);
    }

    /// コマンドを実行し、終了を待つ。起動できない・失敗したら、その理由を
    /// 返す（#684）。既に消えていた場合は成功
    fn execute(&self) -> Result<(), String> {
        let result = self.execute_one();
        match &self.then {
            Some(then) => {
                let next = then.execute();
                result.and(next)
            }
            None => result,
        }
    }

    fn execute_one(&self) -> Result<(), String> {
        let mut command = std::process::Command::new(&self.program);
        command
            .args(&self.args)
            .env_clear()
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let output = command
            .output()
            .map_err(|e| format!("could not run `{}`: {}", self.command_line(), e))?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if self.ignore_already_removed && is_already_removed(&stderr) {
            return Ok(());
        }
        let stderr = stderr.trim();
        let mut message = format!("`{}` failed ({})", self.command_line(), output.status);
        if !stderr.is_empty() {
            message.push_str(": ");
            message.push_str(&truncate(stderr, 300));
        }
        Err(message)
    }

    /// 表示用のコマンド。環境変数（資格情報を含むことがある）は含めない
    fn command_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// 落とされたら`Cleanup`を実行する。作ったもの（KubernetesのJob・Secret）を
/// 準備の途中で外から打ち切られても（`tokio::time::timeout`等でFutureが
/// 落とされても）、残さないため（#697）。準備ができたら`disarm`で取り出す
struct CleanupOnDrop(Option<Cleanup>);

impl CleanupOnDrop {
    fn new(cleanup: Cleanup) -> Self {
        Self(Some(cleanup))
    }

    /// 後片付けを取り出す。落としても、もう実行しない
    fn disarm(mut self) -> Cleanup {
        self.0
            .take()
            .expect("CleanupOnDrop holds the cleanup until it is disarmed")
    }
}

impl Drop for CleanupOnDrop {
    fn drop(&mut self) {
        if let Some(cleanup) = self.0.take() {
            // 作る要求の途中で落とされたときに、後から作り終えたものも消す
            cleanup.run_twice(CLEANUP_RETRY_DELAY);
        }
    }
}

/// 準備の途中で落とされたときに、もう1度消すまでの時間。`check`等が後片付けを
/// 待つ時間（10秒）に収まるようにする
const CLEANUP_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(3);

/// テストで、後片付けの結果（`PENDING_CLEANUPS`・`CLEANUP_FAILURES`）を
/// 読み出す（`wait_for_cleanups`）テストを、同時に動かさないためのロック。
/// 並べて動かすと、ほかのテストが登録した失敗を先に読み出してしまう
/// （PR #698のレビュー）
#[cfg(test)]
pub(crate) static CLEANUP_REGISTRY_TEST_LOCK: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

/// 後片付けのコマンドが、消すものが既に無くて失敗したか。`docker run --rm`の
/// コンテナは止まると自分で消えるので、`docker rm -f`と重なることがある
fn is_already_removed(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "no such container",
        "already in progress",
        "no container with name or id",
        // `docker network rm`（#615）
        "no such network",
    ]
    .iter()
    .any(|text| stderr.contains(text))
}

/// 実行中の後片付けのコマンド（`Cleanup::run`）
static PENDING_CLEANUPS: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>> =
    std::sync::Mutex::new(Vec::new());

/// 失敗した後片付けのコマンドと、その理由（#684）
static CLEANUP_FAILURES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// 後片付けのコマンドを待った結果（`wait_for_cleanups`）
#[derive(Debug, Default)]
pub struct CleanupReport {
    /// 待ち終えずに残ったコマンドがある
    pub timed_out: bool,
    /// 失敗した・起動できなかったコマンドと、その理由（#684）
    pub failures: Vec<String>,
}

/// 後片付けのコマンド（`docker rm -f`・`kubectl delete`）の終了を、最長
/// `timeout`待つ。mcp-rsが終わる前に呼ぶ。待たないと、プロセスの終了で
/// コマンドのスレッドも終わり、コンテナ・Job・Secretが残ることがある
/// （PR #672のレビュー）。待ち終えなかったか、失敗したコマンドを返す（#684）
pub fn wait_for_cleanups(timeout: std::time::Duration) -> CleanupReport {
    let deadline = std::time::Instant::now() + timeout;
    let handles = std::mem::take(
        &mut *PENDING_CLEANUPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let mut report = CleanupReport::default();
    'handles: for handle in handles {
        // JoinHandleには時間を区切ったjoinが無いので、終わったかを見て待つ
        while !handle.is_finished() {
            if std::time::Instant::now() >= deadline {
                report.timed_out = true;
                break 'handles;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = handle.join();
    }
    report.failures = std::mem::take(
        &mut *CLEANUP_FAILURES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    report
}

/// dockerコマンド自体に渡す、本体の環境変数
fn docker_cli_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(key, _)| is_docker_cli_env_var(key))
        .collect()
}

/// プラグイン名を検証する。ツール名の前に付けるため、英小文字・数字・
/// `_`・`-`のみ。区切りと紛らわしい`__`は使えない。`_`で始まる・終わる
/// 名前も使えない（#543）: 例えば`fs_`のツール`fs___read`は、区切りの
/// `__`と合わせて`_`が3つ続き、別のプラグイン`fs`のツールの指定
/// （`tools = ["fs__*"]`）にも当たってしまう
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
    if name.starts_with('_') || name.ends_with('_') {
        return Err(format!(
            "plugin name '{}' must not start or end with '_'",
            name
        ));
    }
    Ok(())
}

/// MCPのツールの名前の長さの上限（2025-11-25の仕様の`SHOULD`）
const MAX_TOOL_NAME_LENGTH: usize = 128;

/// `tools/list`のツール（`raw`）が、tasks（2025-11-25）でしか呼べないか
/// （`execution.taskSupport`が`"required"`、#741）
fn requires_task(raw: &Value) -> bool {
    raw.pointer("/execution/taskSupport")
        .and_then(Value::as_str)
        == Some("required")
}

/// MCPのツールの名前の形か（#598）。英数字・`_`・`-`・`.`で、1〜128文字
/// （2025-11-25の仕様の`SHOULD`）。プラグインの名前を前に付けた名前で確かめる
fn is_valid_tool_name(name: &str) -> bool {
    (1..=MAX_TOOL_NAME_LENGTH).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// プラグインの設定を、起動せずに確かめる（`mcp-rs check`、#589）。
/// 起動するときと同じ確かめ方で、起動に使うコマンド（プロセスなら
/// `command`、コンテナなら`docker`等、Kubernetesなら`kubectl`等）を返す
pub fn launcher_program(config: &PluginConfig) -> Result<String, String> {
    validate_plugin_name(&config.name)?;
    match (&config.container, &config.kubernetes) {
        (Some(_), Some(_)) => Err("container and kubernetes cannot both be set".to_string()),
        (Some(container), None) => {
            // 起動するときと同じく、`env`の名前も確かめる（値は要らない。#697）
            let mut env_keys: Vec<&str> = config.env.keys().map(String::as_str).collect();
            env_keys.sort_unstable();
            container_launch(config, container, &env_keys, "check").map(|launch| launch.program)
        }
        (None, Some(kubernetes)) => {
            kubernetes::validate(config, kubernetes)?;
            Ok(kubernetes
                .kubectl_command
                .as_deref()
                .unwrap_or(kubernetes::DEFAULT_KUBECTL_COMMAND)
                .trim()
                .to_string())
        }
        (None, None) if config.command.trim().is_empty() => Err("command is empty".to_string()),
        (None, None) => Ok(config.command.clone()),
    }
}

/// 設定されたプラグインを起動する。起動できなかったプラグインは警告を
/// 出して飛ばし、本体の起動は続ける（そのプラグインのツールが使えない
/// だけにする）。ツール呼び出しは`audit_logger`に記録する（本体の他の
/// ハンドラーと同じものを渡す、#416）。
pub async fn start_plugins(
    configs: &[PluginConfig],
    audit_logger: Arc<AuditLogger>,
) -> Vec<(String, Arc<PluginHandler>)> {
    start_plugins_reporting_failures(configs, audit_logger)
        .await
        .0
}

/// `start_plugins`と同じ。起動できなかったプラグイン（名前、理由）も返す
/// （`mcp_rs_status`に出すため、#434）。
pub async fn start_plugins_reporting_failures(
    configs: &[PluginConfig],
    audit_logger: Arc<AuditLogger>,
) -> (Vec<(String, Arc<PluginHandler>)>, Vec<(String, String)>) {
    start_plugins_until(configs, audit_logger, std::future::pending()).await
}

/// `start_plugins_reporting_failures`と同じ。`stop`が終わったら（mcp-rsの
/// 終了の合図、#775）、起動の途中のプラグインをやめ、残りのプラグインは
/// 起動しない。やめたプラグインの子プロセスは止め、コンテナ・Kubernetesの
/// Job・Secretの後片付けを始める（起動の途中のfutureを落とすと、
/// `Connection`・`CleanupOnDrop`の破棄で始まる。終わるのは
/// `wait_for_cleanups`で待つ）。起動し終えたプラグインは返すので、呼ぶ側が
/// 止める（`McpServer::shutdown`）。
///
/// プラグインの起動（`start_timeout`の既定は30秒）を待ってから合図を見ると、
/// `docker stop`の猶予（既定は10秒）の内に後片付けを終えられないため
pub async fn start_plugins_until(
    configs: &[PluginConfig],
    audit_logger: Arc<AuditLogger>,
    stop: impl std::future::Future<Output = ()>,
) -> (Vec<(String, Arc<PluginHandler>)>, Vec<(String, String)>) {
    let mut stop = std::pin::pin!(stop);
    let mut started = Vec::new();
    let mut failed = Vec::new();
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
            failed.push((
                config.name.clone(),
                "another plugin has the same name".to_string(),
            ));
            continue;
        }
        let start = PluginHandler::start_with_audit_logger(config, audit_logger.clone());
        // 既に合図を受けていれば、起動しない
        let result = tokio::select! {
            biased;
            () = &mut stop => None,
            result = start => Some(result),
        };
        let Some(result) = result else {
            warn!(
                "Plugin '{}' was not started: mcp-rs is shutting down",
                config.name
            );
            failed.push((config.name.clone(), "mcp-rs is shutting down".to_string()));
            break;
        };
        match result {
            Ok(handler) => {
                info!(
                    "Plugin '{}' started with {} tools",
                    config.name,
                    handler.tools.len()
                );
                started.push((config.name.clone(), Arc::new(handler)));
            }
            Err(e) => {
                warn!("Plugin '{}' is not available: {}", config.name, e);
                failed.push((config.name.clone(), e));
            }
        }
    }
    (started, failed)
}

enum ReadLine {
    Line(Vec<u8>),
    /// 上限を超えた行（行末まで読み捨てた）。先頭の
    /// `OVERSIZED_LINE_PREFIX_BYTES`バイトだけを持つ
    Oversized(Vec<u8>),
}

/// 行の長さを、末尾のLFと、その直前のCRを除いて数える（#750）。行末を
/// まだ読んでいなければ（`complete`が`false`）、最後のCRは次のLFの直前
/// かもしれないので数えない
fn line_content_len(line: &[u8], complete: bool) -> usize {
    let mut content = line;
    if complete {
        content = content.strip_suffix(b"\n").unwrap_or(content);
    }
    content.strip_suffix(b"\r").unwrap_or(content).len()
}

/// 読み捨てる行の先頭から、最上位の`"id"`の値を読む（#750）。先頭の範囲に
/// `"id"`が無いとき（前の項目が長い等）・JSONとして読めないときは`None`
fn leading_id(prefix: &[u8]) -> Option<Value> {
    let mut i = skip_json_whitespace(prefix, 0);
    if prefix.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    loop {
        i = skip_json_whitespace(prefix, i);
        if prefix.get(i) != Some(&b'"') {
            return None;
        }
        let key_end = json_value_end(prefix, i)?;
        let key: String = serde_json::from_slice(&prefix[i..key_end]).ok()?;
        i = skip_json_whitespace(prefix, key_end);
        if prefix.get(i) != Some(&b':') {
            return None;
        }
        i = skip_json_whitespace(prefix, i + 1);
        let value_end = json_value_end(prefix, i)?;
        if key == "id" {
            return serde_json::from_slice(&prefix[i..value_end]).ok();
        }
        i = skip_json_whitespace(prefix, value_end);
        if prefix.get(i) != Some(&b',') {
            return None;
        }
        i += 1;
    }
}

fn skip_json_whitespace(bytes: &[u8], mut i: usize) -> usize {
    while matches!(bytes.get(i), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        i += 1;
    }
    i
}

/// `start`から始まるJSONの値の終わり（の次の位置）。値が途中で切れて
/// いれば`None`
fn json_value_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &b) in bytes.get(start..)?.iter().enumerate() {
        let at = start + offset;
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                if depth == 0 {
                    // 数等の値が、外側の`}`で終わった
                    return Some(at);
                }
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            b',' | b' ' | b'\t' | b'\r' | b'\n' if depth == 0 => return Some(at),
            _ => {}
        }
    }
    None
}

/// STDIOの`notifications/cancelled`で取り消したプラグインの呼び出しの
/// エラー（#766）。監査ログには`CANCELLED`と記録する
const PLUGIN_CALL_CANCELLED: &str = "the tool call was cancelled by the client";

fn plugin_call_cancelled() -> McpError {
    McpError::Other(PLUGIN_CALL_CANCELLED.to_string())
}

/// 送信中の`write_message`が取り消された（期限切れ等）ら、接続を止める
/// （#750）。送りかけのフレームがパイプに残り、次の要求がその続きになって、
/// 子プロセスが独立したJSON-RPCのフレームとして読めなくなるため
struct WriteInProgress<'a> {
    connection: &'a mut Connection,
    finished: bool,
}

impl Drop for WriteInProgress<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.connection.stop(
                "sending a message to the plugin was interrupted (the call timed out while writing)"
                    .to_string(),
            );
        }
    }
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
    /// `initialize`・`tools/list`を終えた（起動し終えた）
    started: bool,
    /// `network = "allowlist"`の中継のプロキシ（#615）。プラグインと一緒に止める
    egress_proxy: Option<Child>,
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
        if let Some(proxy) = self.egress_proxy.as_mut() {
            let _ = proxy.start_kill();
        }
        if let Some(cleanup) = self.cleanup.take() {
            if self.started {
                cleanup.run();
            } else {
                // 起動の途中で止めた（期限切れ・mcp-rsの終了の合図、#775）。
                // `docker run`を止めたとき、デーモンがまだコンテナを作って
                // いる途中だと、先の`docker rm -f`は「無い」と返し、その後に
                // 作り終えたコンテナが残る。少し後でもう1度消す（PR #827の
                // レビュー。Kubernetesの`CleanupOnDrop`と同じ）
                cleanup.run_twice(CLEANUP_RETRY_DELAY);
            }
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
        // 書き終える前に取り消されたら、`WriteInProgress`が接続を止める
        let mut writing = WriteInProgress {
            connection: self,
            finished: false,
        };
        let result = async {
            writing.connection.stdin.write_all(&bytes).await?;
            writing.connection.stdin.flush().await
        }
        .await;
        writing.finished = true;
        drop(writing);
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
            if self.discarding {
                // 読み捨てる行も、`id`を読むために先頭だけは残す
                let room = OVERSIZED_LINE_PREFIX_BYTES.saturating_sub(self.line.len());
                self.line.extend_from_slice(&available[..len.min(room)]);
            } else {
                self.line.extend_from_slice(&available[..len]);
            }
            self.stdout.consume(len);

            // 行末のLFと、その直前のCRは上限に数えない
            if !self.discarding && line_content_len(&self.line, newline) > max_bytes {
                self.discarding = true;
                self.line.truncate(OVERSIZED_LINE_PREFIX_BYTES);
                self.line.shrink_to_fit();
            }
            if newline {
                if self.discarding {
                    self.discarding = false;
                    return Ok(ReadLine::Oversized(std::mem::take(&mut self.line)));
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
        self.request_cancellable(method, params, max_bytes, None)
            .await
    }

    /// `request`と同じ。`cancel`で取り消されたら（STDIOの
    /// `notifications/cancelled`、#766）:
    /// - まだ送っていなければ、送らずにやめる
    /// - 送り終えていれば、このリクエストの`id`（プロキシが割り当てたもの）で
    ///   プラグインに`notifications/cancelled`を送り、応答を待たずにやめる。
    ///   後から届く応答は、次のリクエストで`id`を見て読み飛ばす
    ///
    /// 書き込みの途中では止めない（取り消しを見るのは、応答を待つ間だけ）。
    /// 途中で止めると、送りかけのフレームが残り、接続ごと止まるため（#750）
    async fn request_cancellable(
        &mut self,
        method: &str,
        params: Value,
        max_bytes: usize,
        mut cancel: Option<&mut crate::mcp::CallCancellation>,
    ) -> Result<Value, McpError> {
        if let Some(reason) = self.stopped.clone() {
            return Err(self.not_running_error(&reason));
        }
        if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
            return Err(plugin_call_cancelled());
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

        // 取り消された後に読んだ行の数。プラグインが関係の無い行（通知等）を
        // 続けて書いても、取り消しを遅らせないよう、1行までにする（PR #826の
        // レビュー。`biased`の`select!`は、読める行がある間は取り消しを見ない）。
        // 取り消しより前に届いた応答を返すのは、それが次の1行のときだけ。応答の
        // 前に通知等が溜まっていれば、応答を読まずに取り消す（監査ログは
        // `CANCELLED`。プラグインは処理を終えているが、取り消しの側に倒す）
        let mut lines_after_cancel = 0usize;
        loop {
            let read = match cancel.as_deref_mut() {
                Some(cancel) => {
                    if cancel.is_cancelled() {
                        lines_after_cancel += 1;
                    }
                    if lines_after_cancel > 1 {
                        None
                    } else {
                        // 応答が先に届いていれば、それを返す
                        tokio::select! {
                            biased;
                            read = self.read_line(max_bytes) => Some(read),
                            _ = cancel.cancelled() => None,
                        }
                    }
                }
                None => Some(self.read_line(max_bytes).await),
            };
            let Some(read) = read else {
                // `read_line`は途中で止めても、読んだ分を`self.line`に残す
                self.write_message(&json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": { "requestId": id, "reason": "cancelled by the client" }
                }))
                .await?;
                return Err(plugin_call_cancelled());
            };
            let bytes = match read? {
                ReadLine::Line(bytes) => bytes,
                ReadLine::Oversized(prefix) => {
                    // 期限の切れた前の要求への応答なら、読み飛ばす（#750）。
                    // 待っている要求への応答か、`id`が読めなければエラー
                    if leading_id(&prefix).is_some_and(|other| other != json!(id)) {
                        debug!(
                            "Plugin '{}' sent an oversized response for another request (ignored)",
                            self.plugin
                        );
                        continue;
                    }
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

/// プラグインのツールの結果（CallToolResult）から、仕様（2025-06-18・2025-11-25）で
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
    /// 起動の仕方（`process`・`container`・`kubernetes`、#434の状態表示用）
    runtime: &'static str,
    /// ツールの「読み取りだけ」の印を、`@read_only`に使うか（#595）
    trust_annotations: bool,
    /// mcp-rsの終了で`true`にする（#673）。処理中の呼び出しをやめさせ、
    /// 接続のロックを空けて、子プロセス・コンテナを止められるようにする
    shutting_down: tokio::sync::watch::Sender<bool>,
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
        // `network = "allowlist"`の中継のプロキシ（#615）
        let mut egress_proxy = None;
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
                if let Some(egress) = &launch.egress {
                    // 内部のネットワーク・プロキシ・プラグインのコンテナを、まとめて
                    // 消す。用意の途中で失敗・中断しても残さない（#615）
                    let guard = CleanupOnDrop::new(Cleanup {
                        program: launch.program.clone(),
                        args: vec![
                            "rm".into(),
                            "-f".into(),
                            launch.container_name.clone(),
                            egress.proxy_name.clone(),
                        ],
                        env: docker_env.clone(),
                        ignore_already_removed: true,
                        then: Some(Box::new(Cleanup {
                            program: launch.program.clone(),
                            args: vec!["network".into(), "rm".into(), egress.network.clone()],
                            env: docker_env.clone(),
                            ignore_already_removed: true,
                            then: None,
                        })),
                    });
                    info!(
                        "Plugin '{}' can reach only {} (network '{}', through the egress proxy '{}')",
                        config.name,
                        container.allowed_hosts.as_deref().unwrap_or_default().join(", "),
                        egress.network,
                        egress.proxy_name
                    );
                    egress_proxy = Some(
                        start_egress_proxy(
                            &config.name,
                            &launch.program,
                            &docker_env,
                            egress,
                            start_timeout,
                        )
                        .await?,
                    );
                    cleanup = Some(guard.disarm());
                }
                let mut command = Command::new(&launch.program);
                command
                    .args(&launch.args)
                    .env_clear()
                    .envs(docker_env.clone())
                    // `--env=NAME`の値は、dockerコマンドの環境変数から渡る
                    .envs(env);
                if cleanup.is_none() {
                    cleanup = Some(Cleanup {
                        program: launch.program,
                        args: vec!["rm".into(), "-f".into(), launch.container_name],
                        env: docker_env,
                        ignore_already_removed: true,
                        then: None,
                    });
                }
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
            started: false,
            egress_proxy,
        };

        let raw_tools = match tokio::time::timeout(
            start_timeout,
            Self::handshake(&mut connection, max_response_bytes),
        )
        .await
        {
            Ok(Ok(tools)) => {
                connection.started = true;
                tools
            }
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
            // MCPのツールの名前の形（#598）。空白・制御文字・`*`等の入った
            // 名前は、ログ・監査ログ・ツールの制限の名前の指定を乱すので使わない
            if !is_valid_tool_name(&name) {
                warn!(
                    "Plugin '{}' listed tool {:?}, whose name is not a valid MCP tool name (letters, digits, '_', '-' and '.', at most {} characters with the plugin name) (ignored)",
                    config.name,
                    original.chars().take(80).collect::<String>(),
                    MAX_TOOL_NAME_LENGTH
                );
                continue;
            }
            if original_names.contains_key(&name) {
                warn!(
                    "Plugin '{}' listed tool '{}' twice (ignored the second)",
                    config.name, original
                );
                continue;
            }
            // tasks（2025-11-25）でしか呼べないツールは、使えない（#741）。
            // mcp-rsはtasksを実装しておらず、`task`無しの`tools/call`は断られる
            if requires_task(&raw) {
                warn!(
                    "Plugin '{}' listed tool '{}', which can only be called as a task (execution.taskSupport = \"required\"); mcp-rs does not support tasks (ignored)",
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
            trust_annotations: config.trust_annotations.unwrap_or(false),
            shutting_down: tokio::sync::watch::channel(false).0,
            runtime: if config.container.is_some() {
                "container"
            } else if config.kubernetes.is_some() {
                "kubernetes"
            } else {
                "process"
            },
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
        // 知らない版なら、`notifications/initialized`を送る前にやめる（#750）
        if !KNOWN_PLUGIN_PROTOCOL_VERSIONS.contains(&version) {
            return Err(McpError::ExternalApi(format!(
                "plugin '{}' answered with MCP protocol version '{}', which mcp-rs does not support (supported: {})",
                connection.plugin,
                truncate(version, 64),
                KNOWN_PLUGIN_PROTOCOL_VERSIONS.join(", ")
            )));
        }
        debug!(
            "Plugin '{}' uses MCP protocol version {}",
            connection.plugin, version
        );
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
        // 子プロセスが`isError: true`の結果を返したときも、失敗として記録する
        // （#750）。中継する結果は変えない
        let tool_error = result
            .as_ref()
            .is_ok_and(|value| value.get("isError") == Some(&Value::Bool(true)));
        let success = result.is_ok() && !tool_error;
        // STDIOで取り消した呼び出し（#766）。プラグインは処理を続けているか
        // もしれないので、失敗ではなく取り消しとして残す
        let cancelled =
            matches!(result, Err(McpError::Other(message)) if message == PLUGIN_CALL_CANCELLED);
        let status = if success {
            "SUCCESS"
        } else if cancelled {
            "CANCELLED"
        } else {
            "FAILURE"
        };
        let mut entry = AuditLogEntry::new(
            if success {
                AuditLevel::Info
            } else if cancelled {
                AuditLevel::Warning
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
        } else if tool_error {
            entry = entry.add_metadata(
                "error".to_string(),
                "the tool returned a result with isError: true".to_string(),
            );
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
        let mut shutting_down = self.shutting_down.subscribe();
        // STDIOの取り消し（#766）。HTTP等では`None`
        let mut cancel = crate::mcp::call_cancellation();
        let call = tokio::time::timeout(self.call_timeout, async {
            // 前の呼び出しを待つ間に取り消されたら、送らずにやめる
            let mut connection = match cancel.as_mut() {
                Some(cancel) => tokio::select! {
                    biased;
                    connection = self.connection.lock() => connection,
                    _ = cancel.cancelled() => return Err(plugin_call_cancelled()),
                },
                None => self.connection.lock().await,
            };
            connection
                .request_cancellable(
                    "tools/call",
                    params,
                    self.max_response_bytes,
                    cancel.as_mut(),
                )
                .await
        });
        // mcp-rsの終了では、応答を待たずにやめる（#673）。待っている間は
        // 接続がロックされ、`shutdown`がプラグインを止められないため
        let result = tokio::select! {
            biased;
            _ = shutting_down.wait_for(|stopping| *stopping) => {
                Err(self.shutting_down_error())
            }
            result = call => result.unwrap_or_else(|_| {
                Err(McpError::ExternalApi(format!(
                    "plugin '{}' did not respond within {} seconds",
                    self.name,
                    self.call_timeout.as_secs()
                )))
            }),
        }?;

        call_tool_result_for_client(&self.name, result)
    }

    fn shutting_down_error(&self) -> McpError {
        McpError::ExternalApi(format!(
            "plugin '{}' was stopped because mcp-rs is shutting down",
            self.name
        ))
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

    fn trusts_read_only_hints(&self) -> bool {
        self.trust_annotations
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<Value, McpError> {
        // STDIOの取り消し（`notifications/cancelled`）で、途中で止めさせない
        // （PR #765のレビュー）。送信の途中で止めると、`WriteInProgress`が
        // 接続ごと止め、監査ログも残らない。取り消しは、止めずに知らせを
        // 受ける（#766）: まだ送っていなければ送らず、送り終えていれば、
        // プラグインに`notifications/cancelled`を伝えて、応答を待たずに
        // 終える。どちらも監査ログに残す
        if !crate::mcp::keep_call_from_being_aborted() {
            return Err(McpError::Other(
                "the tool call was cancelled before it started".to_string(),
            ));
        }
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

    async fn status(&self) -> Option<String> {
        // 呼び出しの最中は接続がロックされている。待たずに「動作中」とする
        let state = match self.connection.try_lock() {
            Ok(mut connection) => match &connection.stopped {
                Some(reason) => format!("stopped ({})", reason),
                None => match connection.child.try_wait() {
                    Ok(Some(exit)) => format!("exited ({})", exit),
                    _ => "running".to_string(),
                },
            },
            Err(_) => "running (a call is in progress)".to_string(),
        };
        Some(format!(
            "plugin '{}' ({}): {}, {} tools",
            self.name,
            self.runtime,
            state,
            self.tools.len()
        ))
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        // 段階1はツールのみ中継する
        Ok(Vec::new())
    }

    async fn read_resource(&self, params: ResourceReadParams) -> Result<Value, McpError> {
        Err(McpError::ResourceNotFound(params.uri))
    }

    async fn shutdown(&self) {
        // 処理中の呼び出しをやめさせてから、接続のロックを取る（#673）
        self.shutting_down.send_replace(true);
        let mut connection = self.connection.lock().await;
        if connection.stopped.is_none() {
            // 後片付け（`docker rm -f`等）は`Cleanup::run`が始め、終了は
            // `wait_for_cleanups`で待つ
            connection.kill();
            connection.stopped = Some("mcp-rs is shutting down".to_string());
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::{PluginKubernetesConfig, PluginMountConfig};

    /// 後片付けのコマンドを覚えておき、終了を待てる（PR #672のレビュー）。
    /// 起動できなかったコマンドは、失敗として返す（#684）
    #[test]
    fn test_wait_for_cleanups() {
        let _registry = CLEANUP_REGISTRY_TEST_LOCK.blocking_lock();
        Cleanup {
            program: "mcp-rs-test-no-such-command".to_string(),
            args: vec!["job/test-684".to_string()],
            env: Vec::new(),
            ignore_already_removed: false,
            then: None,
        }
        .run();
        let report = wait_for_cleanups(std::time::Duration::from_secs(30));
        assert!(!report.timed_out);
        assert!(
            report.failures.iter().any(|failure| failure
                .starts_with("could not run `mcp-rs-test-no-such-command job/test-684`")),
            "{:?}",
            report.failures
        );
    }

    /// `path`にファイルを作るコマンド
    /// （このテストのバイナリを`test_touch_main`だけ動かす。シェルを通さない
    /// ので、パスに空白等があってもよい。PR #698のレビュー）
    fn touch_cleanup(path: &std::path::Path) -> Cleanup {
        Cleanup {
            program: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            args: vec![
                "--exact".to_string(),
                "plugin_proxy::tests::test_touch_main".to_string(),
                "--ignored".to_string(),
                "--test-threads=1".to_string(),
            ],
            env: vec![(
                TEST_TOUCH_ENV.to_string(),
                path.to_string_lossy().into_owned(),
            )],
            ignore_already_removed: false,
            then: None,
        }
    }

    const TEST_TOUCH_ENV: &str = "MCP_RS_TEST_TOUCH";

    /// `touch_cleanup`から動かすとき（`MCP_RS_TEST_TOUCH`があるとき）だけ、
    /// そのパスのファイルに1行足す（何回動いたかを数えられる）
    #[test]
    #[ignore]
    fn test_touch_main() {
        use std::io::Write as _;
        if let Some(path) = std::env::var_os(TEST_TOUCH_ENV) {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            file.write_all(b"ran\n").unwrap();
        }
    }

    /// ファイルの行数（無ければ0）
    fn count_lines(path: &std::path::Path) -> usize {
        std::fs::read_to_string(path)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    /// ファイルが`lines`行になるまで、最長10秒待つ
    fn wait_for_lines(path: &std::path::Path, lines: usize) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if count_lines(path) >= lines {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        count_lines(path) >= lines
    }

    /// 準備の途中で落とされたら、後片付けを実行し、少し後でもう1度実行する
    /// （遅れて作り終えたものも消す。PR #698のレビュー）。取り出した後は
    /// 実行しない（#697）
    #[test]
    fn test_cleanup_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        // パスに空白があってもよい（PR #698のレビュー）
        let dropped = dir.path().join("dropped marker");
        drop(CleanupOnDrop::new(touch_cleanup(&dropped)));
        assert!(wait_for_lines(&dropped, 2), "{}", count_lines(&dropped));

        let disarmed = dir.path().join("disarmed");
        let cleanup = CleanupOnDrop::new(touch_cleanup(&disarmed)).disarm();
        // 取り出した後片付けは、呼べば1回だけ実行できる
        assert_eq!(count_lines(&disarmed), 0);
        cleanup.execute().unwrap();
        assert_eq!(count_lines(&disarmed), 1);
    }

    /// `tokio::time::timeout`で打ち切られたFutureの中のガードも、後片付けを
    /// 実行する（`mcp-rs check`がプラグインの起動を10秒で打ち切るとき。#697）
    #[tokio::test]
    async fn test_cleanup_runs_when_the_future_is_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("cancelled");
        let preparing = {
            let marker = marker.clone();
            async move {
                let _guard = CleanupOnDrop::new(touch_cleanup(&marker));
                // Podを待ち続ける代わり
                std::future::pending::<()>().await;
            }
        };
        let result = tokio::time::timeout(std::time::Duration::from_millis(50), preparing).await;
        assert!(result.is_err());
        assert!(wait_for_lines(&marker, 1));
    }

    /// 終了コードを返すコマンド（Windowsでは`cmd /c exit N`）
    fn exit_cleanup(code: u8) -> Cleanup {
        #[cfg(windows)]
        let (program, args) = ("cmd", vec!["/c".to_string(), format!("exit {}", code)]);
        #[cfg(not(windows))]
        let (program, args) = ("sh", vec!["-c".to_string(), format!("exit {}", code)]);
        Cleanup {
            program: program.to_string(),
            args,
            env: Vec::new(),
            ignore_already_removed: false,
            then: None,
        }
    }

    /// 後片付けのコマンドが失敗したら、コマンドと終了コードを返す（#684）
    #[test]
    fn test_cleanup_execute_reports_failure() {
        let error = exit_cleanup(1).execute().unwrap_err();
        assert!(error.contains("exit 1`"), "{}", error);
        assert!(error.contains("failed"), "{}", error);
    }

    /// 「既に消えていた」の文面で成功とみなすのは、コンテナの後片付けだけ。
    /// kubectlの失敗は、同じ文面でも知らせる（PR #685のレビュー）
    #[test]
    fn test_already_removed_only_for_container_cleanups() {
        #[cfg(windows)]
        let (program, args) = (
            "cmd",
            vec![
                "/c".to_string(),
                "echo No such container: x 1>&2 & exit 1".to_string(),
            ],
        );
        #[cfg(not(windows))]
        let (program, args) = (
            "sh",
            vec![
                "-c".to_string(),
                "echo 'No such container: x' >&2; exit 1".to_string(),
            ],
        );
        let cleanup = |ignore_already_removed| Cleanup {
            program: program.to_string(),
            args: args.clone(),
            env: Vec::new(),
            ignore_already_removed,
            then: None,
        };
        assert_eq!(cleanup(true).execute(), Ok(()));
        let error = cleanup(false).execute().unwrap_err();
        assert!(error.contains("No such container"), "{}", error);
    }

    /// 後片付けのコマンドが成功したら、失敗にしない（#684）
    #[test]
    fn test_cleanup_execute_success() {
        assert_eq!(exit_cleanup(0).execute(), Ok(()));
    }

    /// 消すものが既に無いだけの失敗は、成功とみなす（#684）
    #[test]
    fn test_is_already_removed() {
        assert!(is_already_removed(
            "Error response from daemon: No such container: mcp-rs-fs-1"
        ));
        assert!(is_already_removed(
            "Error response from daemon: removal of container mcp-rs-fs-1 is already in progress"
        ));
        assert!(is_already_removed(
            "Error: no container with name or ID \"mcp-rs-fs-1\" found: no such container"
        ));
        assert!(!is_already_removed(
            "error: You must be logged in to the server (Unauthorized)"
        ));
        assert!(!is_already_removed(""));
    }

    const TEST_PLUGIN_ENV: &str = "MCP_RS_TEST_PLUGIN";
    /// テスト用のプラグインが`initialize`で答える版（省略時は`PROTOCOL_VERSION`）
    const TEST_PLUGIN_VERSION_ENV: &str = "MCP_RS_TEST_PLUGIN_VERSION";
    /// テスト用のプラグインが`initialize`に答えるまで待つミリ秒（#775）
    const TEST_PLUGIN_START_DELAY_ENV: &str = "MCP_RS_TEST_PLUGIN_START_DELAY_MS";

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

        // 受け取った`notifications/cancelled`の`requestId`（#766）
        let mut cancels: Vec<Value> = Vec::new();
        while let Some(Ok(line)) = lines.next() {
            let request: Value = serde_json::from_str(&line).unwrap();
            let Some(id) = request.get("id").cloned() else {
                if request["method"] == json!("notifications/cancelled") {
                    cancels.push(request["params"]["requestId"].clone());
                }
                continue; // 通知
            };
            let method = request["method"].as_str().unwrap_or("");
            let result = match method {
                "initialize" => {
                    if let Some(ms) = std::env::var(TEST_PLUGIN_START_DELAY_ENV)
                        .ok()
                        .and_then(|ms| ms.parse().ok())
                    {
                        std::thread::sleep(Duration::from_millis(ms));
                    }
                    json!({
                    // 知らない版で答えるプラグインのテスト（#750）
                    "protocolVersion": std::env::var(TEST_PLUGIN_VERSION_ENV)
                        .unwrap_or_else(|_| PROTOCOL_VERSION.to_string()),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "test-plugin", "version": "1.0.0" }
                    })
                }
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
                        json!({ "tools": [
                            tool("big"),
                            tool("exit"),
                            tool("noisy"),
                            tool("fail"),
                            tool("toolerror"),
                            tool("slowbig"),
                            tool("raw"),
                            tool("cancels")
                        ] })
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
                        "cancels" => text_result(Value::Array(cancels.clone()).to_string()),
                        "toolerror" => json!({
                            "content": [{ "type": "text", "text": "tool failed" }],
                            "isError": true
                        }),
                        "slowbig" => {
                            // 期限の切れた後に届く、上限を超える応答（#750）
                            let ms = args["ms"].as_u64().unwrap_or(0);
                            std::thread::sleep(Duration::from_millis(ms));
                            text_result("x".repeat(args["bytes"].as_u64().unwrap_or(0) as usize))
                        }
                        "raw" => {
                            // 改行を除いてちょうど`bytes`バイトの応答を、LFか
                            // CRLFで終えて送る（#750）
                            let bytes = args["bytes"].as_u64().unwrap_or(0) as usize;
                            let line = |text: String| {
                                json!({ "jsonrpc": "2.0", "id": id, "result": text_result(text) })
                                    .to_string()
                            };
                            let padding = bytes.saturating_sub(line(String::new()).len());
                            let mut text = line("x".repeat(padding));
                            assert_eq!(text.len(), bytes);
                            text.push_str(if args["crlf"] == json!(true) {
                                "\r\n"
                            } else {
                                "\n"
                            });
                            let mut raw = std::io::stdout();
                            raw.write_all(text.as_bytes()).unwrap();
                            raw.flush().unwrap();
                            continue;
                        }
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

    /// テスト用のプラグイン（このテストのバイナリ）の設定。`mcp-rs check`の
    /// テスト（#589）でも使う
    pub(crate) fn test_plugin_config(name: &str) -> PluginConfig {
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
            trust_annotations: None,
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
        for ok in ["example", "my-plugin", "tool_1", "a", "a-", "-a"] {
            assert!(validate_plugin_name(ok).is_ok(), "{}", ok);
        }
        for bad in [
            "",
            "Example",
            "my plugin",
            "a__b",
            // #543: `fs_`のツール`fs___read`は、`fs__*`にも当たる
            "fs_",
            "_fs",
            "_",
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
                "demo__fail",
                "demo__toolerror",
                "demo__slowbig",
                "demo__raw",
                "demo__cancels"
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

    /// STDIOで取り消した呼び出しは、プラグインに、プロキシの`id`で
    /// `notifications/cancelled`を伝え、応答を待たずに終える（#766）。監査
    /// ログには`CANCELLED`と残る。後から届く応答は、次の呼び出しで読み飛ばす
    #[tokio::test]
    async fn test_cancelled_call_is_relayed_to_the_plugin() {
        let handler = PluginHandler::start(&test_plugin_config("cancel"))
            .await
            .unwrap();
        let started = std::time::Instant::now();
        let (cancel, running) =
            crate::mcp::run_as_stdio_call(call(&handler, "cancel__sleep", json!({ "ms": 1500 })));
        let (result, ()) = tokio::join!(running, async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(cancel());
        });
        let error = result
            .expect("a plugin call is not aborted midway")
            .unwrap_err();
        assert!(error.to_string().contains(PLUGIN_CALL_CANCELLED), "{error}");
        // 応答（1.5秒後）を待たずに終わる
        assert!(started.elapsed() < Duration::from_millis(1200));

        // 次の呼び出しは、取り消した呼び出しの遅れた応答を読み飛ばす
        let result = call(&handler, "cancel__echo", json!({ "text": "after" }))
            .await
            .unwrap();
        assert_eq!(text(&result), "after");
        let result = call(&handler, "cancel__cancels", json!({})).await.unwrap();
        let cancels: Vec<Value> = serde_json::from_str(text(&result)).unwrap();
        // プロキシが割り当てた`id`（`initialize`・2ページの`tools/list`の後の
        // 4番目）。クライアントの`id`（`1`）ではない（PR #826のレビュー）
        assert_eq!(cancels, vec![json!(4)]);

        let entries = handler.audit_logger().get_all_entries().await;
        assert!(
            entries
                .iter()
                .any(|e| e.message == "Plugin tool call: cancel__sleep - CANCELLED"),
            "{:?}",
            entries.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    /// 前の呼び出しを待っている間に取り消した呼び出しは、プラグインに送らない
    /// （#766）
    #[tokio::test]
    async fn test_call_cancelled_before_sending_is_not_sent() {
        let handler = PluginHandler::start(&test_plugin_config("queued"))
            .await
            .unwrap();
        let (cancel, waiting) =
            crate::mcp::run_as_stdio_call(call(&handler, "queued__echo", json!({ "text": "x" })));
        let (first, second, ()) = tokio::join!(
            call(&handler, "queued__sleep", json!({ "ms": 800 })),
            async {
                // 先の呼び出しが接続を使ってから、待ち始める
                tokio::time::sleep(Duration::from_millis(100)).await;
                waiting.await
            },
            async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                assert!(cancel());
            }
        );
        assert_eq!(text(&first.unwrap()), "slept");
        let error = second.unwrap().unwrap_err();
        assert!(error.to_string().contains(PLUGIN_CALL_CANCELLED), "{error}");
        let result = call(&handler, "queued__cancels", json!({})).await.unwrap();
        assert_eq!(text(&result), "[]");
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
    async fn test_shutdown_stops_plugin_shared_with_dispatcher() {
        // HTTPモードのように、`dispatcher`がハンドラーを共有していても、
        // `McpServer::shutdown`でプラグインが止まる（#673）
        let handler = Arc::new(
            PluginHandler::start(&test_plugin_config("down"))
                .await
                .unwrap(),
        );
        let mut server = crate::mcp::McpServer::new();
        server.add_handler("down".to_string(), handler.clone());
        let _dispatcher = server.dispatcher();

        // 処理中の呼び出しは、応答を待たずにエラーで終わる
        let in_flight = {
            let handler = handler.clone();
            tokio::spawn(async move { call(&handler, "down__sleep", json!({ "ms": 20000 })).await })
        };
        tokio::time::sleep(Duration::from_millis(300)).await;
        tokio::time::timeout(Duration::from_secs(5), server.shutdown())
            .await
            .expect("shutdown should not wait for the call in progress");
        let error = tokio::time::timeout(Duration::from_secs(5), in_flight)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("shutting down"), "{}", error);

        let status = handler.status().await.unwrap();
        assert!(
            status.contains("stopped (mcp-rs is shutting down)"),
            "{}",
            status
        );
        let error = call(&handler, "down__echo", json!({ "text": "late" }))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("shutting down"), "{}", error);

        // プラグインのプロセスが実際に終わる（`dispatcher`が残っていても）
        // （PR #675のレビュー）
        let exited = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(Some(_)) = handler.connection.lock().await.child.try_wait() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(exited.is_ok(), "the plugin process is still running");

        // 2回目は何もしない
        server.shutdown().await;
        drop(_dispatcher);
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
        assert_eq!(handler.list_tools().await.unwrap().len(), 11);
    }

    /// 子プロセスが`isError: true`の結果を返したら、結果はそのまま中継し、
    /// 監査ログには`FAILURE`と記録する（#750）
    #[tokio::test]
    async fn test_tool_error_result_is_audited_as_failure() {
        let handler = PluginHandler::start(&test_plugin_config("toolerr"))
            .await
            .unwrap();
        let result = call(&handler, "toolerr__toolerror", json!({}))
            .await
            .unwrap();
        assert_eq!(result["isError"], json!(true));
        assert_eq!(text(&result), "tool failed");
        call(&handler, "toolerr__echo", json!({ "text": "ok" }))
            .await
            .unwrap();

        let entries = handler.audit_logger().get_all_entries().await;
        let results: Vec<Option<&str>> = entries.iter().map(|e| e.result.as_deref()).collect();
        assert_eq!(results, vec![Some("FAILURE"), Some("SUCCESS")]);
        assert_eq!(entries[0].level, AuditLevel::Error);
    }

    /// 起動の途中で止める合図を受けたら、起動を待たずにやめ、残りの
    /// プラグインも起動しない（#775）。合図の前に起動し終えたものは返す
    #[tokio::test]
    async fn test_start_plugins_until_stops_a_slow_start() {
        let first = test_plugin_config("first");
        let mut slow = test_plugin_config("slow");
        slow.env
            .insert(TEST_PLUGIN_START_DELAY_ENV.to_string(), "20000".to_string());
        slow.call_timeout_seconds = Some(60);
        let rest = test_plugin_config("rest");
        let audit_logger = Arc::new(AuditLogger::with_defaults());

        let started_at = std::time::Instant::now();
        let (started, failed) = start_plugins_until(
            &[first, slow, rest],
            audit_logger,
            // 1つ目が起動し、2つ目の`initialize`を待っている間に合図を受ける
            tokio::time::sleep(Duration::from_secs(3)),
        )
        .await;
        // 2つ目の起動（20秒）を待たない
        assert!(
            started_at.elapsed() < Duration::from_secs(10),
            "{:?}",
            started_at.elapsed()
        );
        let names: Vec<&str> = started.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, vec!["first"]);
        assert_eq!(
            failed,
            vec![("slow".to_string(), "mcp-rs is shutting down".to_string())]
        );
        for (_, handler) in started {
            handler.shutdown().await;
        }

        // 既に合図を受けていれば、1つも起動しない
        let (started, failed) = start_plugins_until(
            &[test_plugin_config("never")],
            Arc::new(AuditLogger::with_defaults()),
            std::future::ready(()),
        )
        .await;
        assert!(started.is_empty());
        assert_eq!(failed.len(), 1);
    }

    /// 知らない版で答えるプラグインは、起動のエラーにする（#750）
    #[tokio::test]
    async fn test_plugin_with_unknown_protocol_version_fails_to_start() {
        let mut config = test_plugin_config("unknownver");
        config.env.insert(
            TEST_PLUGIN_VERSION_ENV.to_string(),
            "2099-01-01".to_string(),
        );
        let error = PluginHandler::start(&config).await.err().unwrap();
        assert!(error.contains("2099-01-01"), "{}", error);
        assert!(error.contains("does not support"), "{}", error);

        // 知っている古い版なら起動する
        let mut config = test_plugin_config("oldver");
        config.env.insert(
            TEST_PLUGIN_VERSION_ENV.to_string(),
            "2024-11-05".to_string(),
        );
        assert!(PluginHandler::start(&config).await.is_ok());
    }

    /// 送信の途中で期限が切れたら、送りかけのフレームが残った接続を止める。
    /// 応答を待つ間の期限切れでは、接続を使い続ける（#750）
    #[tokio::test]
    async fn test_timeout_while_writing_stops_the_connection() {
        let mut config = test_plugin_config("partial");
        config.call_timeout_seconds = Some(1);
        let handler = PluginHandler::start(&config).await.unwrap();

        // 子プロセスが3秒間、標準入力を読まないようにする（応答を待つ間の
        // 期限切れなので、接続は止めない）
        let error = call(&handler, "partial__sleep", json!({ "ms": 3000 }))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("did not respond"), "{}", error);
        assert!(handler.status().await.unwrap().contains(": running"));

        // パイプに入りきらない要求は、書き終える前に期限が切れる
        let error = call(
            &handler,
            "partial__echo",
            json!({ "text": "x".repeat(8 * 1024 * 1024) }),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("did not respond"), "{}", error);
        let status = handler.status().await.unwrap();
        assert!(status.contains("interrupted"), "{}", status);

        let error = call(&handler, "partial__echo", json!({ "text": "after" }))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("not running"), "{}", error);
    }

    /// 期限の切れた前の要求への、上限を超える応答は読み飛ばし、待っている
    /// 要求の応答を返す（#750）
    #[tokio::test]
    async fn test_oversized_late_response_for_another_request_is_skipped() {
        let mut config = test_plugin_config("latebig");
        config.call_timeout_seconds = Some(1);
        let handler = PluginHandler::start(&config).await.unwrap();

        let error = call(
            &handler,
            "latebig__slowbig",
            json!({ "ms": 1500, "bytes": 10000 }),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("did not respond"), "{}", error);

        let result = call(&handler, "latebig__echo", json!({ "text": "after" }))
            .await
            .unwrap();
        assert_eq!(text(&result), "after");
    }

    /// 行の上限は、末尾のLFと、その直前のCRを除いた長さで比べる（#750）
    #[tokio::test]
    async fn test_line_limit_excludes_only_the_line_ending() {
        let handler = PluginHandler::start(&test_plugin_config("limit"))
            .await
            .unwrap();
        // `test_plugin_config`の`max_response_bytes`は4096
        for crlf in [false, true] {
            let result = call(
                &handler,
                "limit__raw",
                json!({ "bytes": 4096, "crlf": crlf }),
            )
            .await
            .unwrap();
            assert!(text(&result).starts_with('x'), "crlf: {}", crlf);

            let error = call(
                &handler,
                "limit__raw",
                json!({ "bytes": 4097, "crlf": crlf }),
            )
            .await
            .unwrap_err();
            assert!(
                error.to_string().contains("max_response_bytes"),
                "crlf: {}: {}",
                crlf,
                error
            );
        }
    }

    #[test]
    fn test_line_content_len() {
        assert_eq!(line_content_len(b"{}\n", true), 2);
        assert_eq!(line_content_len(b"{}\r\n", true), 2);
        // CRは、LFの直前の1つだけを除く
        assert_eq!(line_content_len(b"{}\r\r\n", true), 3);
        // 行末を読む前は、最後のCRを数えない（次がLFかもしれない）
        assert_eq!(line_content_len(b"{}\r", false), 2);
        assert_eq!(line_content_len(b"{}\rx", false), 4);
        assert_eq!(line_content_len(b"{}", false), 2);
    }

    #[test]
    fn test_leading_id() {
        let big = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{{\"text\":\"{}",
            "x".repeat(100)
        );
        assert_eq!(leading_id(big.as_bytes()), Some(json!(7)));
        assert_eq!(
            leading_id(br#" { "id" : "a\"b", "result": "#),
            Some(json!("a\"b"))
        );
        // 前の項目（入れ子・文字列の中の`"id"`）は飛ばす
        assert_eq!(
            leading_id(br#"{"meta":{"id":1,"x":[1,"}"]},"note":"\"id\":2","id":3,"#),
            Some(json!(3))
        );
        assert_eq!(leading_id(br#"{"id":null}"#), Some(Value::Null));
        // 先頭に`"id"`が無い・途中で切れている・JSONでない
        assert_eq!(leading_id(br#"{"result":{"text":"xxxx"#), None);
        assert_eq!(leading_id(br#"{"jsonrpc":"2.0","id":12"#), None);
        assert_eq!(leading_id(b"not json"), None);
        assert_eq!(leading_id(b""), None);
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
        // プロセスが終わるのと、`initialize`を書き込むのの、どちらが先かで
        // 文が変わる（終わった後に書くと、パイプが閉じている）
        assert!(
            error.contains("exited") || error.contains("not running"),
            "{}",
            error
        );
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
            allowed_hosts: None,
            egress_proxy_image: None,
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

    /// `network = "allowlist"`（#615）: プラグインは内部のネットワークに入れ、
    /// プロキシを環境変数で教える。プロキシは内部のネットワークと`bridge`に
    /// つなぎ、許可した行き先だけを渡す
    #[test]
    fn test_container_launch_with_allowlist() {
        let mut container = container_config("example/search:1.0");
        container.network = Some("allowlist".to_string());
        container.allowed_hosts = Some(vec![
            "api.example.com:443".to_string(),
            "*.cdn.example.com:443".to_string(),
        ]);
        let config = container_plugin_config("search", container.clone());
        let launch = container_launch(&config, &container, &["API_KEY"], "abc123").unwrap();
        assert!(launch
            .args
            .contains(&"--network=mcp-rs-search-abc123-net".to_string()));
        assert!(!launch.args.iter().any(|a| a == "--network=allowlist"));
        for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
            assert!(
                launch.args.contains(&format!(
                    "--env={key}=http://mcp-rs-search-abc123-egress:3128"
                )),
                "{key}: {:?}",
                launch.args
            );
        }
        let egress = launch.egress.unwrap();
        assert_eq!(egress.network, "mcp-rs-search-abc123-net");
        assert_eq!(egress.proxy_name, "mcp-rs-search-abc123-egress");
        let expected: Vec<String> = [
            "run",
            "--rm",
            "--init",
            "--pull=never",
            "--name=mcp-rs-search-abc123-egress",
            "--label=mcp-rs.plugin=search",
            "--network=mcp-rs-search-abc123-net",
            "--network=bridge",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--read-only",
            "--memory=64m",
            "--pids-limit=64",
            DEFAULT_EGRESS_PROXY_IMAGE,
            "egress-proxy",
            "--listen",
            "0.0.0.0:3128",
            "--allow",
            "api.example.com:443",
            "--allow",
            "*.cdn.example.com:443",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(egress.proxy_args, expected);

        // プロキシのイメージを変えられる
        container.egress_proxy_image = Some("registry.example.com/mcp-rs:0.32.0".to_string());
        let launch = container_launch(&config, &container, &[], "x").unwrap();
        assert!(launch
            .egress
            .unwrap()
            .proxy_args
            .contains(&"registry.example.com/mcp-rs:0.32.0".to_string()));
    }

    #[test]
    fn test_container_launch_rejects_invalid_allowlist() {
        let check = |container: PluginContainerConfig, env: &[&str], expected: &str| {
            let config = container_plugin_config("bad", container.clone());
            let error = container_launch(&config, &container, env, "x").unwrap_err();
            assert!(error.contains(expected), "{error}");
        };
        let mut allowlist = container_config("example/search:1.0");
        allowlist.network = Some("allowlist".to_string());
        // 行き先が無い・形が違う
        check(allowlist.clone(), &[], "allowed_hosts is empty");
        let mut bad = allowlist.clone();
        bad.allowed_hosts = Some(vec!["1.2.3.4:443".to_string()]);
        check(bad, &[], "not an IP address");
        let mut bad = allowlist.clone();
        bad.allowed_hosts = Some(vec!["api.example.com".to_string()]);
        check(bad, &[], "needs a port");
        // プロキシの環境変数は`env`で渡せない
        let mut ok = allowlist.clone();
        ok.allowed_hosts = Some(vec!["api.example.com:443".to_string()]);
        check(ok, &["HTTPS_PROXY"], "sets it to its proxy");
        // 名前が長いと、プロキシのホスト名がDNSのラベルの上限を超える
        let mut long = allowlist.clone();
        long.allowed_hosts = Some(vec!["api.example.com:443".to_string()]);
        let config = container_plugin_config(&"a".repeat(37), long.clone());
        let error = container_launch(&config, &long, &[], "x").unwrap_err();
        assert!(error.contains("at most 36 characters"), "{error}");
        let config = container_plugin_config(&"a".repeat(36), long.clone());
        let launch = container_launch(&config, &long, &[], "abcdef123456").unwrap();
        assert_eq!(launch.egress.unwrap().proxy_name.len(), 63);
        // `allowlist`でないのに`allowed_hosts`・`egress_proxy_image`
        let mut other = container_config("example/search:1.0");
        other.network = Some("bridge".to_string());
        other.allowed_hosts = Some(vec!["api.example.com:443".to_string()]);
        check(other, &[], "needs network = \"allowlist\"");
        let mut other = container_config("example/search:1.0");
        other.egress_proxy_image = Some("x/y:1".to_string());
        check(other, &[], "needs network = \"allowlist\"");
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

    /// `mcp-rs check`（`launcher_program`）も、起動するときと同じく`env`の
    /// 名前を確かめる（#697）
    #[test]
    fn test_launcher_program_checks_container_env_names() {
        let mut config = container_plugin_config("tool", container_config("example/image"));
        config
            .env
            .insert("API_KEY".to_string(), "${MCP_RS_TEST_API_KEY}".to_string());
        assert!(launcher_program(&config).is_ok());
        config
            .env
            .insert("DOCKER_HOST".to_string(), "tcp://example:2375".to_string());
        let error = launcher_program(&config).unwrap_err();
        assert!(error.contains("DOCKER_HOST"), "{error}");
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

        // Secretは、作る時点から同じプラグインのJobが所有者（#700）
        for plugin in ["k8snone", "k8sallow"] {
            let get = |kind: &str, jsonpath: &str| {
                let output = std::process::Command::new("kubectl")
                    .args([
                        "get",
                        kind,
                        "-o",
                        &format!("jsonpath={}", jsonpath),
                        &format!("--selector={}={}", CONTAINER_LABEL, plugin),
                    ])
                    .output()
                    .unwrap();
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            };
            let owner = get(
                "secrets",
                "{.items[0].metadata.ownerReferences[0].kind}/{.items[0].metadata.ownerReferences[0].name}/{.items[0].metadata.ownerReferences[0].uid}",
            );
            let job = get(
                "jobs",
                "Job/{.items[0].metadata.name}/{.items[0].metadata.uid}",
            );
            assert!(job.len() > "Job//".len(), "no job for {}", plugin);
            assert_eq!(owner, job, "owner of the secret for {}", plugin);
        }

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

    /// tasksでしか呼べないツールだけを除く（#741）
    #[test]
    fn test_requires_task() {
        assert!(requires_task(
            &json!({ "name": "t", "execution": { "taskSupport": "required" } })
        ));
        for tool in [
            json!({ "name": "t" }),
            json!({ "name": "t", "execution": {} }),
            json!({ "name": "t", "execution": { "taskSupport": "optional" } }),
            json!({ "name": "t", "execution": { "taskSupport": "forbidden" } }),
        ] {
            assert!(!requires_task(&tool), "{tool}");
        }
    }

    #[test]
    fn test_is_valid_tool_name() {
        for ok in [
            "fs__read_file",
            "a",
            "x.y-z_1",
            &"a".repeat(MAX_TOOL_NAME_LENGTH),
        ] {
            assert!(is_valid_tool_name(ok), "{ok}");
        }
        for bad in [
            "",
            "fs__read file",
            "fs__read\nfile",
            "fs__*",
            "fs__読む",
            "fs__a/b",
            &"a".repeat(MAX_TOOL_NAME_LENGTH + 1),
        ] {
            assert!(!is_valid_tool_name(bad), "{bad:?}");
        }
    }
}
