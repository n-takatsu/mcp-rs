//! プラグインをKubernetesのJobとして起動する（#413、#392の段階3）
//!
//! `kubectl`で、Job（とSecret・NetworkPolicy）を作り、`kubectl attach -i`で
//! Podの標準入出力につなぐ。認証は`kubectl`のもの（kubeconfig、または
//! クラスタ内のServiceAccount）をそのまま使う。
//!
//! - `env`の値はSecretに入れ、マニフェストとして標準入力で渡す（コマンド
//!   ラインに載せない）。SecretはJobを所有者にして、Jobと一緒に消えるように
//!   する
//! - Jobは`backoffLimit: 0`（再起動しない）・`ttlSecondsAfterFinished`付き。
//!   mcp-rsが強制終了しても、Podは標準入力が閉じて終わり、Jobは消える
//! - 停止時・終了時は、`kubectl delete`でJobとSecretを消す

use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::info;

use super::{truncate, validate_option_value, Cleanup, CONTAINER_LABEL};
use crate::config::{PluginConfig, PluginKubernetesConfig};

const DEFAULT_KUBECTL_COMMAND: &str = "kubectl";
const DEFAULT_MEMORY: &str = "512Mi";
const DEFAULT_CPU: &str = "500m";
const DEFAULT_RUN_AS_USER: u32 = 65532;
const DEFAULT_START_TIMEOUT_SECS: u64 = 120;
/// 終わったJob（とPod・Secret）を、Kubernetesが消すまでの時間
const JOB_TTL_SECONDS: u64 = 60;
/// Podの起動を待つ以外の、`kubectl`の1回の実行の待ち時間
const KUBECTL_TIMEOUT: Duration = Duration::from_secs(30);
const POD_POLL_INTERVAL: Duration = Duration::from_secs(1);
/// プラグインのコンテナ名
const CONTAINER_NAME: &str = "plugin";
/// 通信を拒否するNetworkPolicyの名前と、その対象にするラベル
pub(super) const NETWORK_POLICY_NAME: &str = "mcp-rs-plugin-deny-all";
pub(super) const NETWORK_LABEL: &str = "mcp-rs.network";
/// Job・Pod・Secretを1回の起動ごとに見分けるラベル
const INSTANCE_LABEL: &str = "mcp-rs.instance";
/// これらの理由で待っているPodは、待っても起動しない
const FATAL_WAITING_REASONS: &[&str] = &[
    "ErrImagePull",
    "ImagePullBackOff",
    "InvalidImageName",
    "CreateContainerError",
];

/// 起動の準備ができた`kubectl attach`と、後片付け
pub(super) struct Prepared {
    pub command: Command,
    pub cleanup: Cleanup,
    pub start_timeout: Duration,
}

/// `kubectl`のコマンドと、すべての実行に付ける`--context`・`--namespace`
struct Kubectl {
    program: String,
    target_args: Vec<String>,
    env: Vec<(String, String)>,
}

impl Kubectl {
    fn command(&self, args: &[String]) -> Command {
        let mut command = Command::new(&self.program);
        // kubectlは本体と同じ環境変数で動かす（kubeconfigの場所や、
        // 認証プラグインが使う変数のため）。プラグイン（Pod）には渡らない
        command
            .args(args)
            .args(&self.target_args)
            .env_clear()
            .envs(self.env.iter().cloned());
        #[cfg(windows)]
        {
            command.creation_flags(0x0800_0000);
        }
        command
    }

    /// 実行して、標準出力を返す。失敗したらkubectlのエラーメッセージを返す。
    async fn run(&self, args: &[String], input: Option<&str>) -> Result<String, String> {
        let mut command = self.command(args);
        command
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|e| format!("could not start '{}': {}", self.program, e))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin
                .write_all(input.as_bytes())
                .await
                .map_err(|e| format!("could not write to kubectl: {}", e))?;
        }
        let output = tokio::time::timeout(KUBECTL_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| {
                format!(
                    "kubectl {} did not finish within {} seconds",
                    args.first().map(String::as_str).unwrap_or(""),
                    KUBECTL_TIMEOUT.as_secs()
                )
            })?
            .map_err(|e| format!("could not run kubectl: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "kubectl {} failed: {}",
                args.first().map(String::as_str).unwrap_or(""),
                truncate(String::from_utf8_lossy(&output.stderr).trim(), 500)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// DNSラベル（名前空間名）として正しいか
fn is_dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

/// 設定を検証する。
fn validate(config: &PluginConfig, kubernetes: &PluginKubernetesConfig) -> Result<(), String> {
    let field = |name: &str| format!("kubernetes.{}", name);
    validate_option_value(&field("image"), kubernetes.image.trim())?;
    if let Some(namespace) = &kubernetes.namespace {
        if !is_dns_label(namespace) {
            return Err(format!(
                "kubernetes.namespace '{}' is not a valid namespace name",
                namespace
            ));
        }
    }
    if let Some(context) = &kubernetes.context {
        validate_option_value(&field("context"), context)?;
    }
    match kubernetes.network.as_deref() {
        None | Some("none") | Some("allow") => {}
        Some(other) => {
            return Err(format!(
                "kubernetes.network '{}' must be \"none\" or \"allow\"",
                other
            ))
        }
    }
    if let Some(memory) = &kubernetes.memory {
        validate_option_value(&field("memory"), memory)?;
    }
    if let Some(cpu) = &kubernetes.cpu {
        validate_option_value(&field("cpu"), cpu)?;
    }
    if kubernetes.run_as_user == Some(0) {
        return Err("kubernetes.run_as_user must not be 0 (root)".to_string());
    }
    if let Some(policy) = &kubernetes.image_pull_policy {
        if !["Always", "IfNotPresent", "Never"].contains(&policy.as_str()) {
            return Err(format!(
                "kubernetes.image_pull_policy '{}' must be Always, IfNotPresent or Never",
                policy
            ));
        }
    }
    if kubernetes
        .kubectl_command
        .as_deref()
        .is_some_and(|c| c.trim().is_empty())
    {
        return Err("kubernetes.kubectl_command is empty".to_string());
    }
    if let Some(dir) = &config.working_dir {
        if !dir.starts_with('/') {
            return Err(format!(
                "working_dir '{}' must be a path in the container (starting with '/')",
                dir
            ));
        }
    }
    for key in config.env.keys() {
        // Secretのキーとして使える文字だけ
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        {
            return Err(format!(
                "environment variable name '{}' cannot be used with kubernetes (use letters, digits, '-', '.' and '_')",
                key
            ));
        }
    }
    Ok(())
}

/// ラベルの値（63文字以内、英数字で始まり英数字で終わる）にする
fn label_value(plugin: &str) -> String {
    let value: String = plugin.chars().take(63).collect();
    value
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_string()
}

/// Job・Secretの名前（DNSラベル、63文字以内）
fn job_name(plugin: &str, instance: &str) -> String {
    let plugin: String = plugin.replace('_', "-").chars().take(40).collect();
    let plugin = plugin.trim_matches('-');
    format!("mcp-rs-{}-{}", plugin, instance)
}

fn labels(plugin: &str, instance: &str) -> Map<String, Value> {
    let mut labels = Map::new();
    labels.insert(CONTAINER_LABEL.into(), json!(label_value(plugin)));
    labels.insert(INSTANCE_LABEL.into(), json!(instance));
    labels
}

fn network_policy_manifest() -> Value {
    json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "NetworkPolicy",
        "metadata": {
            "name": NETWORK_POLICY_NAME,
            "labels": { "app.kubernetes.io/managed-by": "mcp-rs" }
        },
        "spec": {
            // このラベルのPodは、受信・送信ともにすべて拒否する（許可の
            // ルールを1つも持たない）
            "podSelector": { "matchLabels": { NETWORK_LABEL: "none" } },
            "policyTypes": ["Ingress", "Egress"]
        }
    })
}

fn secret_manifest(name: &str, labels: &Map<String, Value>, env: &[(String, String)]) -> Value {
    let data: Map<String, Value> = env.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
    json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "metadata": { "name": name, "labels": labels },
        "type": "Opaque",
        "stringData": data
    })
}

fn job_manifest(
    config: &PluginConfig,
    kubernetes: &PluginKubernetesConfig,
    name: &str,
    labels: &Map<String, Value>,
    with_secret: bool,
) -> Value {
    let user = kubernetes.run_as_user.unwrap_or(DEFAULT_RUN_AS_USER);
    let mut pod_labels = labels.clone();
    if kubernetes.network.as_deref().unwrap_or("none") == "none" {
        pod_labels.insert(NETWORK_LABEL.into(), json!("none"));
    }

    let mut container = json!({
        "name": CONTAINER_NAME,
        "image": kubernetes.image.trim(),
        "stdin": true,
        "stdinOnce": true,
        "tty": false,
        "securityContext": {
            "allowPrivilegeEscalation": false,
            "capabilities": { "drop": ["ALL"] },
            "readOnlyRootFilesystem": kubernetes.read_only.unwrap_or(true)
        },
        "resources": {
            "limits": {
                "memory": kubernetes.memory.as_deref().unwrap_or(DEFAULT_MEMORY),
                "cpu": kubernetes.cpu.as_deref().unwrap_or(DEFAULT_CPU)
            }
        },
        "volumeMounts": [{ "name": "tmp", "mountPath": "/tmp" }]
    });
    // Dockerと同じく、`command`と`args`でイメージのCMDを置き換える
    // （ENTRYPOINTはそのまま）
    let mut args: Vec<String> = Vec::new();
    if !config.command.trim().is_empty() {
        args.push(config.command.clone());
    }
    args.extend(config.args.iter().cloned());
    if !args.is_empty() {
        container["args"] = json!(args);
    }
    if let Some(dir) = &config.working_dir {
        container["workingDir"] = json!(dir);
    }
    if let Some(policy) = &kubernetes.image_pull_policy {
        container["imagePullPolicy"] = json!(policy);
    }
    if with_secret {
        container["envFrom"] = json!([{ "secretRef": { "name": name } }]);
    }

    json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": { "name": name, "labels": labels },
        "spec": {
            "backoffLimit": 0,
            "ttlSecondsAfterFinished": JOB_TTL_SECONDS,
            "template": {
                "metadata": { "labels": pod_labels },
                "spec": {
                    "restartPolicy": "Never",
                    "automountServiceAccountToken": false,
                    "enableServiceLinks": false,
                    "terminationGracePeriodSeconds": 5,
                    "securityContext": {
                        "runAsNonRoot": true,
                        "runAsUser": user,
                        "runAsGroup": user,
                        "seccompProfile": { "type": "RuntimeDefault" }
                    },
                    "containers": [container],
                    "volumes": [{
                        "name": "tmp",
                        "emptyDir": { "medium": "Memory", "sizeLimit": "64Mi" }
                    }]
                }
            }
        }
    })
}

/// Jobを作り、Podが動き始めるまで待って、`kubectl attach`のコマンドを返す。
pub(super) async fn prepare(
    config: &PluginConfig,
    kubernetes: &PluginKubernetesConfig,
    env: &[(String, String)],
) -> Result<Prepared, String> {
    validate(config, kubernetes)?;
    let start_timeout = Duration::from_secs(
        kubernetes
            .start_timeout_seconds
            .unwrap_or(DEFAULT_START_TIMEOUT_SECS),
    );
    let mut target_args = Vec::new();
    if let Some(context) = &kubernetes.context {
        target_args.push(format!("--context={}", context));
    }
    if let Some(namespace) = &kubernetes.namespace {
        target_args.push(format!("--namespace={}", namespace));
    }
    let kubectl = Kubectl {
        program: kubernetes
            .kubectl_command
            .as_deref()
            .unwrap_or(DEFAULT_KUBECTL_COMMAND)
            .trim()
            .to_string(),
        target_args,
        env: std::env::vars().collect(),
    };

    let instance = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let name = job_name(&config.name, &instance);
    let labels = labels(&config.name, &instance);
    let with_secret = !env.is_empty();
    info!(
        "Plugin '{}' runs as Kubernetes Job '{}' (image '{}', namespace '{}', network '{}')",
        config.name,
        name,
        kubernetes.image.trim(),
        kubernetes.namespace.as_deref().unwrap_or("(current)"),
        kubernetes.network.as_deref().unwrap_or("none")
    );

    if kubernetes.network.as_deref().unwrap_or("none") == "none" {
        kubectl
            .run(
                &strings(&["apply", "-f", "-"]),
                Some(&network_policy_manifest().to_string()),
            )
            .await?;
    }

    let mut delete_args = vec!["delete".to_string(), format!("job/{}", name)];
    if with_secret {
        delete_args.push(format!("secret/{}", name));
    }
    delete_args.extend(strings(&["--ignore-not-found", "--wait=false"]));
    delete_args.extend(kubectl.target_args.iter().cloned());
    let cleanup = Cleanup {
        program: kubectl.program.clone(),
        args: delete_args,
        env: kubectl.env.clone(),
    };

    // Secret → Jobの順に作る（PodがSecretを待たないように）。ここから
    // 後で失敗したら、作ったものを消す
    let result = async {
        if with_secret {
            kubectl
                .run(
                    &strings(&["create", "-f", "-"]),
                    Some(&secret_manifest(&name, &labels, env).to_string()),
                )
                .await?;
        }
        let uid = kubectl
            .run(
                &strings(&["create", "-f", "-", "-o", "jsonpath={.metadata.uid}"]),
                Some(&job_manifest(config, kubernetes, &name, &labels, with_secret).to_string()),
            )
            .await?;
        if with_secret {
            // Jobを所有者にして、Jobが消えたらSecretも消えるようにする
            let patch = json!({
                "metadata": { "ownerReferences": [{
                    "apiVersion": "batch/v1",
                    "kind": "Job",
                    "name": name,
                    "uid": uid.trim()
                }] }
            });
            kubectl
                .run(
                    &[
                        "patch".to_string(),
                        format!("secret/{}", name),
                        "--type=merge".to_string(),
                        format!("--patch={}", patch),
                    ],
                    None,
                )
                .await?;
        }
        wait_for_running_pod(&kubectl, &instance, start_timeout).await
    }
    .await;
    let pod = match result {
        Ok(pod) => pod,
        Err(e) => {
            cleanup.run();
            return Err(e);
        }
    };

    let command = kubectl.command(&[
        "attach".to_string(),
        "-i".to_string(),
        "-q".to_string(),
        format!("pod/{}", pod),
        format!("--container={}", CONTAINER_NAME),
    ]);
    Ok(Prepared {
        command,
        cleanup,
        start_timeout,
    })
}

/// JobのPodが`Running`になるまで待ち、Pod名を返す。
async fn wait_for_running_pod(
    kubectl: &Kubectl,
    instance: &str,
    timeout: Duration,
) -> Result<String, String> {
    let started = Instant::now();
    let args = vec![
        "get".to_string(),
        "pods".to_string(),
        format!("--selector={}={}", INSTANCE_LABEL, instance),
        "--output=jsonpath={.items[0].metadata.name}|{.items[0].status.phase}|{.items[0].status.containerStatuses[0].state.waiting.reason}|{.items[0].status.containerStatuses[0].state.waiting.message}".to_string(),
    ];
    let mut last_state = String::from("no pod yet");
    loop {
        // Podがまだ無いと、jsonpathがエラーになることがある
        if let Ok(output) = kubectl.run(&args, None).await {
            let mut parts = output.trim().splitn(4, '|');
            let pod = parts.next().unwrap_or("").to_string();
            let phase = parts.next().unwrap_or("");
            let reason = parts.next().unwrap_or("");
            let message = parts.next().unwrap_or("");
            if !pod.is_empty() {
                match phase {
                    "Running" => return Ok(pod),
                    "Succeeded" | "Failed" => {
                        return Err(format!(
                            "the plugin pod '{}' ended before mcp-rs could connect (phase {})",
                            pod, phase
                        ))
                    }
                    _ => {}
                }
                if FATAL_WAITING_REASONS.contains(&reason) {
                    return Err(format!(
                        "the plugin pod '{}' cannot start: {} {}",
                        pod,
                        reason,
                        truncate(message, 300)
                    ));
                }
                last_state = if reason.is_empty() {
                    format!("pod '{}' is {}", pod, phase)
                } else {
                    format!("pod '{}' is {} ({})", pod, phase, reason)
                };
            }
        }
        if started.elapsed() >= timeout {
            return Err(format!(
                "the plugin pod did not start within {} seconds ({})",
                timeout.as_secs(),
                last_state
            ));
        }
        tokio::time::sleep(POD_POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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

    fn plugin_config(name: &str, kubernetes: PluginKubernetesConfig) -> PluginConfig {
        PluginConfig {
            name: name.to_string(),
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
            container: None,
            kubernetes: Some(kubernetes),
            enabled: None,
            call_timeout_seconds: None,
            max_response_bytes: None,
            rate_limit: None,
        }
    }

    #[test]
    fn test_job_manifest_isolates_by_default() {
        let kubernetes = kubernetes_config("example/server:1.0");
        let mut config = plugin_config("my_plugin", kubernetes.clone());
        config.command = "python".to_string();
        config.args = vec!["-m".to_string(), "server".to_string()];
        let name = job_name(&config.name, "abc123");
        assert_eq!(name, "mcp-rs-my-plugin-abc123");
        let labels = labels(&config.name, "abc123");
        let job = job_manifest(&config, &kubernetes, &name, &labels, true);

        assert_eq!(job["spec"]["backoffLimit"], json!(0));
        assert_eq!(
            job["spec"]["ttlSecondsAfterFinished"],
            json!(JOB_TTL_SECONDS)
        );
        let template = &job["spec"]["template"];
        assert_eq!(
            template["metadata"]["labels"],
            json!({
                "mcp-rs.plugin": "my_plugin",
                "mcp-rs.instance": "abc123",
                "mcp-rs.network": "none"
            })
        );
        let pod = &template["spec"];
        assert_eq!(pod["restartPolicy"], json!("Never"));
        assert_eq!(pod["automountServiceAccountToken"], json!(false));
        assert_eq!(pod["enableServiceLinks"], json!(false));
        assert_eq!(
            pod["securityContext"],
            json!({
                "runAsNonRoot": true,
                "runAsUser": 65532,
                "runAsGroup": 65532,
                "seccompProfile": { "type": "RuntimeDefault" }
            })
        );
        let container = &pod["containers"][0];
        assert_eq!(container["image"], json!("example/server:1.0"));
        assert_eq!(container["args"], json!(["python", "-m", "server"]));
        assert!(container.get("command").is_none());
        assert_eq!(container["stdin"], json!(true));
        assert_eq!(container["stdinOnce"], json!(true));
        assert_eq!(
            container["securityContext"],
            json!({
                "allowPrivilegeEscalation": false,
                "capabilities": { "drop": ["ALL"] },
                "readOnlyRootFilesystem": true
            })
        );
        assert_eq!(
            container["resources"]["limits"],
            json!({ "memory": "512Mi", "cpu": "500m" })
        );
        assert_eq!(
            container["envFrom"],
            json!([{ "secretRef": { "name": "mcp-rs-my-plugin-abc123" } }])
        );
        assert_eq!(
            pod["volumes"][0]["emptyDir"],
            json!({ "medium": "Memory", "sizeLimit": "64Mi" })
        );
    }

    #[test]
    fn test_job_manifest_applies_options() {
        let mut kubernetes = kubernetes_config("example/server");
        kubernetes.network = Some("allow".to_string());
        kubernetes.memory = Some("1Gi".to_string());
        kubernetes.cpu = Some("2".to_string());
        kubernetes.run_as_user = Some(1000);
        kubernetes.read_only = Some(false);
        kubernetes.image_pull_policy = Some("Always".to_string());
        let mut config = plugin_config("p", kubernetes.clone());
        config.working_dir = Some("/work".to_string());
        let labels = labels("p", "x");
        let job = job_manifest(&config, &kubernetes, "n", &labels, false);
        let pod = &job["spec"]["template"]["spec"];
        let container = &pod["containers"][0];

        assert!(job["spec"]["template"]["metadata"]["labels"]
            .get(NETWORK_LABEL)
            .is_none());
        assert_eq!(pod["securityContext"]["runAsUser"], json!(1000));
        assert_eq!(
            container["securityContext"]["readOnlyRootFilesystem"],
            json!(false)
        );
        assert_eq!(
            container["resources"]["limits"],
            json!({ "memory": "1Gi", "cpu": "2" })
        );
        assert_eq!(container["imagePullPolicy"], json!("Always"));
        assert_eq!(container["workingDir"], json!("/work"));
        // commandもargsも無ければ、イメージのものをそのまま使う
        assert!(container.get("args").is_none());
        assert!(container.get("envFrom").is_none());
    }

    #[test]
    fn test_secret_and_network_policy_manifests() {
        let labels = labels("p", "x");
        let env = vec![("API_KEY".to_string(), "secret-value".to_string())];
        let secret = secret_manifest("n", &labels, &env);
        assert_eq!(secret["kind"], json!("Secret"));
        assert_eq!(secret["stringData"], json!({ "API_KEY": "secret-value" }));
        assert_eq!(secret["metadata"]["labels"]["mcp-rs.instance"], json!("x"));

        let policy = network_policy_manifest();
        assert_eq!(policy["metadata"]["name"], json!(NETWORK_POLICY_NAME));
        assert_eq!(
            policy["spec"],
            json!({
                "podSelector": { "matchLabels": { "mcp-rs.network": "none" } },
                "policyTypes": ["Ingress", "Egress"]
            })
        );
    }

    #[test]
    fn test_names_and_labels_stay_within_kubernetes_limits() {
        let long = "a".repeat(62) + "_b";
        let name = job_name(&long, "0123456789ab");
        assert!(name.len() <= 63, "{}", name);
        assert!(is_dns_label(&name), "{}", name);
        let value = label_value(&long);
        assert!(value.len() <= 63);
        assert!(value.ends_with(|c: char| c.is_ascii_alphanumeric()));
        assert_eq!(label_value("my_plugin"), "my_plugin");
    }

    #[test]
    fn test_validate_rejects_invalid_settings() {
        let check = |modify: &dyn Fn(&mut PluginConfig, &mut PluginKubernetesConfig),
                     expected: &str| {
            let mut kubernetes = kubernetes_config("example/image");
            let mut config = plugin_config("bad", kubernetes.clone());
            modify(&mut config, &mut kubernetes);
            let error = validate(&config, &kubernetes).unwrap_err();
            assert!(error.contains(expected), "{}: {}", expected, error);
        };
        check(&|_, k| k.image = String::new(), "image");
        check(&|_, k| k.image = "--x".to_string(), "image");
        check(
            &|_, k| k.namespace = Some("Bad_NS".to_string()),
            "namespace",
        );
        check(&|_, k| k.context = Some("-x".to_string()), "context");
        check(&|_, k| k.network = Some("host".to_string()), "network");
        check(&|_, k| k.memory = Some("1 Gi".to_string()), "memory");
        check(&|_, k| k.run_as_user = Some(0), "root");
        check(
            &|_, k| k.image_pull_policy = Some("always".to_string()),
            "image_pull_policy",
        );
        check(
            &|_, k| k.kubectl_command = Some(" ".to_string()),
            "kubectl_command",
        );
        check(
            &|c, _| c.working_dir = Some("work".to_string()),
            "working_dir",
        );
        check(
            &|c, _| {
                c.env.insert("BAD KEY".to_string(), "v".to_string());
            },
            "BAD KEY",
        );

        let kubernetes = kubernetes_config("example/image");
        assert!(validate(&plugin_config("ok", kubernetes.clone()), &kubernetes).is_ok());
    }
}
