---
layout: page
title: Plugins
permalink: /docs/plugins/
---

# Plugins

**[Home]({{ site.baseurl }}/) > [Documentation]({{ site.baseurl }}/docs/) > Plugins**

A plugin is a separate MCP server that mcp-rs starts as a child process, inside a Docker container, or as a Kubernetes Job. mcp-rs connects to it as an MCP client and offers its tools alongside the built-in ones. Plugins can be written in any language, as long as they speak MCP over standard input/output.

> **Isolation.** A plugin started as a child process runs with your user's permissions. mcp-rs does not restrict which files or network addresses it can reach, so use child processes only for plugins you trust. To limit what a plugin can reach, [run it in a container](#running-a-plugin-in-a-container) or [on Kubernetes](#running-a-plugin-on-kubernetes) (v0.19.0 and later).

Plugins work in both STDIO mode (for example, Claude Desktop and Claude Code) and HTTP mode (v0.18.4 and later; in v0.18.3, `[[plugins]]` was ignored in HTTP mode). In HTTP mode, everyone who can reach the HTTP listener can call the plugin's tools, so turn on API key authentication when the listener is not limited to your own machine.

## Before you add a plugin

- **Edit the config file your MCP client actually uses.** That is the file passed with `--config` in your client's MCP settings (for Claude Desktop, the `args` of the mcp-rs entry in `claude_desktop_config.json`). If you keep more than one mcp-rs config file, changes to the others have no effect. When a plugin is configured and `[server] log_level` is `info` (the default) or more detailed, the mcp-rs log has one line for each `[[plugins]]` entry: `Plugin '<name>' started with N tools`, `Plugin '<name>' is not available: ...` (it failed to start), `Plugin '<name>' is disabled` (`enabled = false`), or `Plugin '<name>' is not started: another plugin has the same name`. With `warn`, only the second and the last appear, and with `error`, none do, so check `log_level` first. If none of them appears at `info`, mcp-rs did not read your `[[plugins]]`
- **Delete the old `[plugins]` section first.** Config files made from the templates of v0.18.1 and earlier may contain a `[plugins]` section (with keys such as `search_paths`, `auto_load`, and `max_plugins`) and `[plugins.*]` subsections. mcp-rs never used them, and ignores them on their own. But a file cannot contain both `[plugins]` and `[[plugins]]`: mcp-rs fails to start with a `duplicate key` error. Delete the old section, including its subsections, before adding `[[plugins]]`

## Adding a plugin

Add one `[[plugins]]` entry per plugin to the config file:

```toml
[[plugins]]
name = "example"                       # tool names become example__<tool>
command = "C:/tools/example-mcp.exe"   # the program to start
args = ["--stdio"]
env = { EXAMPLE_API_KEY = "${EXAMPLE_API_KEY}" }

# Optional:
# enabled = true                 # false: do not start this plugin
# working_dir = "C:/tools"       # default: the folder mcp-rs runs in; a relative path is relative to the config file's folder
# call_timeout_seconds = 30      # startup and each tool call
# max_response_bytes = 1048576   # largest message accepted from the plugin
# trust_annotations = false      # true: @read_only API keys may use the tools the plugin marks read-only
# [plugins.rate_limit]           # same settings as [handlers.wordpress.rate_limit]
```

- `name`: 1 to 64 characters: lowercase letters, digits, `_` and `-` (not `__`, and not starting or ending with `_`, v0.24.0 and later). Each name must be unique
- `command`: The executable to start. For a script, give the interpreter as `command` and the script in `args` (for example, `command = "node"`, `args = ["server.js"]`). A name without a folder (such as `node`) is looked up in `PATH`. A relative path with a folder (such as `./server` or `bin/server.exe`) is relative to the folder of the config file (v0.29.0 and later; up to v0.28.0, to the folder mcp-rs was started in, which in Claude Code is the folder you opened). `working_dir` works the same way. In a container or on Kubernetes, both are paths inside the container and are not changed
- `env`: The only environment variables the plugin receives. `${VAR}` is replaced with mcp-rs's own environment variable. If `VAR` is not set, that entry is not passed at all

## Running a plugin in a container

Add a `[plugins.container]` section after the `[[plugins]]` entry. mcp-rs then starts the plugin with `docker run -i` and talks to it over standard input/output, the same way as a child process.

```toml
[[plugins]]
name = "fs"
args = ["/projects"]                   # passed to the image's ENTRYPOINT

[plugins.container]
image = "mcp/filesystem"
mounts = [
  { source = "C:/Users/me/Documents", target = "/projects" },   # read-only by default
]

# Optional:
# network = "none"          # default. Use "bridge" only for plugins that must reach the network
# memory = "512m"           # default
# pids_limit = 256          # default
# cpus = "1.0"              # default: no limit
# read_only = true          # default: the container's root filesystem is read-only
# user = "1000:1000"        # default: the image's user
# docker_command = "docker" # for example, "podman"
```

- `image`: mcp-rs does not download images (`--pull=never`), so that the first start does not run into `call_timeout_seconds` and the image does not change without notice. Run `docker pull <image>` before starting mcp-rs, and again when you want to update it
- `command` and `args`: In container mode, `command` is the program to run inside the container. Leave it out to use the image's own command; `args` are then passed to the image's `ENTRYPOINT`
- `working_dir`: The folder inside the container (must start with `/`)
- `env`: Passed into the container, the same as for a child process. The values are not put on the `docker` command line, so other users cannot see them in the process list. Names such as `PATH`, `HOME` and `DOCKER_*` cannot be used, because mcp-rs passes those to the `docker` command itself
- `mounts`: The only host folders the plugin can see. `source` must be an absolute path that exists; `target` is the path inside the container. Mounts are read-only unless you set `read_only = false`

By default, mcp-rs starts the container with:

| Option | Effect |
|---|---|
| `--network=none` | No network access, unless you set `network` |
| `--cap-drop=ALL`, `--security-opt=no-new-privileges` | No Linux capabilities, and no way to gain them |
| `--read-only`, `--tmpfs=/tmp:rw,noexec,nosuid,size=64m` | Only `/tmp` (64 MB, in memory) is writable |
| `--memory=512m`, `--pids-limit=256` | Limits on memory and number of processes |
| `--rm`, `--init` | The container is removed when the plugin stops |

When mcp-rs stops the plugin (or exits, in STDIO or HTTP mode, including on Ctrl+C, and on SIGTERM from `docker stop` or `systemctl stop` on Linux and macOS, even while plugins are still starting), it also runs `docker rm -f` on the container and waits up to 10 seconds for it to finish. From v0.32.0, a signal received while a plugin is still starting stops that start at once (the rest of the plugins are not started), instead of waiting for it to finish starting (up to `start_timeout`, 30 seconds by default), and starts the cleanup at once. This makes it much more likely that the cleanup ends within the grace period of `docker stop` (10 seconds by default), but does not guarantee it: the 10 seconds that mcp-rs waits for the cleanup are counted from when it starts stopping the plugins, and a container that was still being created is removed again 3 seconds later. When mcp-rs itself runs in a container, give it more time to stop, for example `docker stop --timeout 30`, or `stop_grace_period: 30s` in Docker Compose; if it takes longer, the log warns you to remove the container by hand. Containers are named `mcp-rs-<plugin name>-<random>` and labeled `mcp-rs.plugin=<plugin name>`, so you can list them with `docker ps --filter label=mcp-rs.plugin`.

If the plugin cannot start (for example, Docker is not running or the image has not been pulled), the warning says so, and Docker's own message is in the log lines that start with `[plugin <name>]`.

## Running a plugin on Kubernetes

Add a `[plugins.kubernetes]` section after the `[[plugins]]` entry. mcp-rs then uses `kubectl` to start the plugin as a Kubernetes Job, and connects to it with `kubectl attach -i`. This works both when mcp-rs runs on your own computer and when mcp-rs itself runs in the cluster (in HTTP mode, with a ServiceAccount). `kubectl` must be installed where mcp-rs runs.

```toml
[[plugins]]
name = "search"
command = "python"
args = ["-m", "my_mcp_server"]
env = { API_KEY = "${SEARCH_API_KEY}" }

[plugins.kubernetes]
image = "registry.example.com/my-mcp-server:1.0"

# Optional:
# namespace = "mcp-plugins"          # default: the current namespace of your kubeconfig
# context = "my-cluster"             # default: the current context of your kubeconfig
# network = "none"                   # default. "allow" lets the plugin use the network
# memory = "512Mi"                   # default
# cpu = "500m"                       # default
# run_as_user = 65532                # default. 0 (root) is not allowed
# read_only = true                   # default: the root filesystem is read-only
# image_pull_policy = "IfNotPresent" # Always, IfNotPresent or Never
# start_timeout_seconds = 120        # default. Waiting for the pod (including the image pull) and for the plugin to start
# kubectl_command = "kubectl"
```

- `command` and `args`: The same as in a container. They replace the image's command (`CMD`); the image's `ENTRYPOINT` is kept. Leave both out to use the image's own command
- `env`: The values are stored in a Secret that belongs to the Job, and deleted with it. They are sent to `kubectl` on its standard input, never on its command line. Names may contain only letters, digits, `-`, `.` and `_`
- `kubectl` runs with mcp-rs's own environment variables (for `KUBECONFIG` and the credential plugins of your cluster). They do not reach the plugin
- `[plugins.container]` and `[plugins.kubernetes]` cannot be used together. Host folders cannot be mounted

What mcp-rs creates:

| Resource | Details |
|---|---|
| NetworkPolicy `mcp-rs-plugin-deny-all` | Created once per namespace when `network = "none"`. Blocks all traffic to and from pods labeled `mcp-rs.network=none` |
| Job `mcp-rs-<plugin name>-<random>` | `backoffLimit: 0` (never restarted), deleted by Kubernetes 60 seconds after it finishes |
| Secret (same name as the Job) | Only when `env` is set. Owned by the Job from the moment it is created: mcp-rs creates the Job suspended, then the Secret, then resumes the Job. If the Job is gone, Kubernetes deletes the Secret |

The plugin's pod runs with:

| Setting | Effect |
|---|---|
| `runAsNonRoot`, `runAsUser`/`runAsGroup: 65532` | Not root, even if the image's user is root |
| `capabilities.drop: [ALL]`, `allowPrivilegeEscalation: false`, `seccompProfile: RuntimeDefault` | No Linux capabilities, no way to gain them, and the runtime's system call filter |
| `readOnlyRootFilesystem`, `/tmp` as a 64 Mi in-memory `emptyDir` | Only `/tmp` is writable |
| `resources.limits` | Memory 512Mi and CPU 500m |
| `automountServiceAccountToken: false`, `enableServiceLinks: false` | No access token for the Kubernetes API, and no environment variables describing the cluster's services (Kubernetes still sets the API server's address, such as `KUBERNETES_SERVICE_HOST`) |
| Label `mcp-rs.network=none` | Covered by the NetworkPolicy above (unless `network = "allow"`) |

When mcp-rs stops the plugin (or exits, in STDIO or HTTP mode), it runs `kubectl delete` on the Job and the Secret and waits up to 10 seconds for it to finish. A tool call still in progress when mcp-rs exits ends with an error. If mcp-rs is killed, the plugin's standard input closes, the plugin exits, and Kubernetes deletes the Job 60 seconds later.

> **NetworkPolicy needs a network plugin that enforces it.** Most clusters do (for example, Calico and Cilium; also kind's default network). On a cluster whose network plugin ignores NetworkPolicy, `network = "none"` has no effect. Check with your cluster administrator.
>
> Even where it is enforced, traffic between a pod and the node it runs on may still be allowed: Kubernetes leaves this to the network plugin. For example, on a single-node kind cluster, the plugin can still open a connection to the API server, which runs on the same node. The plugin has no service account token, so the API server treats it as an anonymous user.

The user or ServiceAccount that `kubectl` runs as needs these permissions in the namespace:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: mcp-rs-plugins
  namespace: mcp-plugins
rules:
  - apiGroups: ["batch"]
    resources: ["jobs"]
    verbs: ["get", "create", "patch", "delete"]
  - apiGroups: [""]
    resources: ["secrets"]
    verbs: ["create", "delete"]
  - apiGroups: ["networking.k8s.io"]
    resources: ["networkpolicies"]
    verbs: ["get", "create", "patch"]
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get", "list", "watch"]
  - apiGroups: [""]
    resources: ["pods/attach"]
    verbs: ["create"]
```

Until v0.29.0, mcp-rs needed `patch` on `secrets` instead of `get` and `patch` on `jobs`. `get` and `patch` on `jobs` are used only for plugins with `env` (`kubectl patch` reads the Job before changing it).

Plugins with `env` need Kubernetes 1.22 or later (the Job's `spec.suspend`, on by default since 1.22). Plugins without `env` do not use it.

If the plugin cannot start (for example, the image cannot be pulled, or the pod does not start within `start_timeout_seconds`), the warning says why, and `kubectl`'s own message is in the log lines that start with `[plugin <name>]`.

## How tools appear

Each tool is offered as `<plugin name>__<tool name>`, and its description starts with `[plugin: <name>]`. For example, the plugin `example` with a tool `search` becomes `example__search`. If a plugin tool has the same name as a built-in tool or another plugin's tool, the one registered first is used and a warning is logged. Built-in handlers are registered before plugins.

A tool whose name is not a valid MCP tool name is left out with a warning (v0.29.0 and later): `<plugin name>__<tool name>` may have only letters, digits, `_`, `-` and `.`, and at most 128 characters. Names with spaces, control characters or `*` would otherwise reach the log, the audit log, and the tool names allowed for API keys.

**Tool descriptions are written by the plugin.** The AI client reads them to decide when to use a tool, so a plugin can put instructions in them, for example to call another tool or to send data somewhere (tool poisoning). mcp-rs passes descriptions on as they are and does not check them. Add only plugins you trust, read their tool descriptions (the client shows them, or ask it to list the tools), and limit API keys to the tools they need.

Only tools are relayed. A plugin's resources, prompts, and notifications (such as `notifications/tools/list_changed`) are not. mcp-rs reads the tool list once, when the plugin starts.

mcp-rs asks plugins for MCP protocol version 2025-11-25, and also works with plugins that answer with an older version (2025-06-18, 2025-03-26, or 2024-11-05). A plugin that answers with any other version fails to start. Tool definitions are passed on with `title`, `outputSchema`, and `annotations`, and tool results with `content`, `structuredContent`, and `isError`. Other fields of a plugin's tool definitions (such as `icons` and `execution`, added in 2025-11-25) are not passed on. The newer fields are passed on only to clients that use version 2025-06-18 or later. For clients that use version 2024-11-05, mcp-rs removes them, and replaces content types that version does not have (resource links and audio) with text. The same information is in `content`. For those clients, if a result has an empty `content` and only `structuredContent`, mcp-rs puts `structuredContent` into `content` as JSON text. A plugin result without `content` is refused as invalid, as before (`content` is required by the protocol). Up to version 0.18.4, mcp-rs used version 2024-11-05 with every client, and did not pass on these fields.

## What mcp-rs enforces

- **Isolation (container and Kubernetes only)**: See [Running a plugin in a container](#running-a-plugin-in-a-container) and [Running a plugin on Kubernetes](#running-a-plugin-on-kubernetes)
- **Environment**: mcp-rs does not pass its own environment variables to the plugin. WordPress credentials and `WORDPRESS_MASTER_PASSWORD` stay in mcp-rs. Only the variables in `env` are passed (on Windows, `SystemRoot` too, because some programs cannot start without it)
- **Rate limiting**: Tool calls to each plugin are rate limited, with the same defaults as WordPress
- **Read-only marks**: A plugin marks its own tools as read-only (`readOnlyHint`). In HTTP mode, an API key or token limited to `@read_only` does not get the plugin's tools, because mcp-rs cannot check the marks (v0.27.0 and later). Set `trust_annotations = true` after checking the plugin's tools, or allow them by name. MCP clients still see the marks as the plugin sent them
- **Audit log**: Every plugin tool call is recorded, whether it succeeds or fails, in the same audit log file as WordPress tool calls (see [Audit Log](../security/#audit-log))
- **Timeout**: If the plugin does not answer within `call_timeout_seconds`, the call fails. A late answer is discarded
- **Cancellation**: When an MCP client cancels a plugin tool call in STDIO mode (`notifications/cancelled`), mcp-rs does not send the answer to the client. From v0.32.0, it also tells the plugin: when the request was already sent, mcp-rs sends the plugin `notifications/cancelled` with the id that mcp-rs gave the request, and stops waiting for the answer, so the next request does not wait for it; an answer that comes later is skipped. When the request was not sent yet (it was waiting for an earlier call to the same plugin), mcp-rs does not send it. The call is recorded in the audit log with the result `CANCELLED`. mcp-rs never stops in the middle of sending a request, which would cut the connection to the plugin. A plugin that does not handle `notifications/cancelled` keeps working on the call. Other tools that are not marked read-only (`readOnlyHint: true`, from a handler whose marks mcp-rs trusts), such as WordPress tools that change content, run to the end, so that a change is never left out of the audit log; read-only tools are stopped right away. While such a call runs, `ping` is still answered at once (as long as the client keeps reading mcp-rs's output); other requests wait until the call ends (v0.30.0 and later)
- **Response size**: A message larger than `max_response_bytes` is discarded and the call fails
- **Output separation**: The plugin's standard error is written to the mcp-rs log. Anything on its standard output that is not JSON-RPC is logged and ignored, so it never reaches your MCP client

## When something goes wrong

- **The plugin cannot start** (the command is not found, or it does not answer `initialize` and `tools/list` within `call_timeout_seconds`): mcp-rs logs a warning and starts without that plugin
- **The plugin exits or stops responding later**: mcp-rs keeps running. Calls to that plugin's tools return an error that says the plugin is not running. Restart mcp-rs to start the plugin again. mcp-rs does not restart plugins automatically
- **Requests from the plugin** (such as sampling): mcp-rs answers that they are not supported

## Notes

- Claude Desktop may start mcp-rs twice. Each mcp-rs starts its own plugin processes (or containers), so you may see two of each. Each mcp-rs stops its plugins when it exits
- For a plugin that runs on Node.js, set `command` to `node.exe` (or `node`) and put the server's script in `args`. `npx` does not work as `command` without `PATH` and other variables, which mcp-rs does not pass
