---
layout: page
title: Plugins
permalink: /docs/plugins/
---

# Plugins

**[Home](../) > [Documentation](./) > Plugins**

A plugin is a separate MCP server that mcp-rs starts as a child process, or inside a Docker container. mcp-rs connects to it as an MCP client and offers its tools alongside the built-in ones. Plugins can be written in any language, as long as they speak MCP over standard input/output.

> **Isolation.** A plugin started as a child process runs with your user's permissions. mcp-rs does not restrict which files or network addresses it can reach, so use child processes only for plugins you trust. To limit what a plugin can reach, [run it in a container](#running-a-plugin-in-a-container) (versions after v0.18.4).

Plugins work in both STDIO mode (for example, Claude Desktop and Claude Code) and HTTP mode (v0.18.4 and later; in v0.18.3, `[[plugins]]` was ignored in HTTP mode). In HTTP mode, everyone who can reach the HTTP listener can call the plugin's tools, so turn on API key authentication when the listener is not limited to your own machine.

## Before you add a plugin

- **Edit the config file your MCP client actually uses.** That is the file passed with `--config` in your client's MCP settings (for Claude Desktop, the `args` of the mcp-rs entry in `claude_desktop_config.json`). If you keep more than one mcp-rs config file, changes to the others have no effect. When a plugin is configured, the mcp-rs log always has either `Plugin '<name>' started with N tools` or `Plugin '<name>' is not available: ...`. If neither appears, mcp-rs did not read your `[[plugins]]`
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
# working_dir = "C:/tools"       # default: the folder mcp-rs runs in
# call_timeout_seconds = 30      # startup and each tool call
# max_response_bytes = 1048576   # largest message accepted from the plugin
# [plugins.rate_limit]           # same settings as [handlers.wordpress.rate_limit]
```

- `name`: 1 to 64 characters: lowercase letters, digits, `_` and `-` (not `__`). Each name must be unique
- `command`: The executable to start. For a script, give the interpreter as `command` and the script in `args` (for example, `command = "node"`, `args = ["server.js"]`)
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

When mcp-rs stops the plugin (or exits), it also runs `docker rm -f` on the container. Containers are named `mcp-rs-<plugin name>-<random>` and labeled `mcp-rs.plugin=<plugin name>`, so you can list them with `docker ps --filter label=mcp-rs.plugin`.

If the plugin cannot start (for example, Docker is not running or the image has not been pulled), the warning says so, and Docker's own message is in the log lines that start with `[plugin <name>]`.

## How tools appear

Each tool is offered as `<plugin name>__<tool name>`, and its description starts with `[plugin: <name>]`. For example, the plugin `example` with a tool `search` becomes `example__search`. If a plugin tool has the same name as a built-in tool or another plugin's tool, the one registered first is used and a warning is logged. Built-in handlers are registered before plugins.

Only tools are relayed. A plugin's resources, prompts, and notifications (such as `notifications/tools/list_changed`) are not. mcp-rs reads the tool list once, when the plugin starts.

Tool results are passed on with `content` and `isError`. mcp-rs uses MCP protocol version 2024-11-05, so newer fields such as `structuredContent` (and the tool's `outputSchema`) are not passed on. The same information is in `content`.

## What mcp-rs enforces

- **Isolation (container only)**: See [Running a plugin in a container](#running-a-plugin-in-a-container)
- **Environment**: mcp-rs does not pass its own environment variables to the plugin. WordPress credentials and `WORDPRESS_MASTER_PASSWORD` stay in mcp-rs. Only the variables in `env` are passed (on Windows, `SystemRoot` too, because some programs cannot start without it)
- **Rate limiting**: Tool calls to each plugin are rate limited, with the same defaults as WordPress
- **Audit log**: Every plugin tool call is recorded, whether it succeeds or fails
- **Timeout**: If the plugin does not answer within `call_timeout_seconds`, the call fails. A late answer is discarded
- **Response size**: A message larger than `max_response_bytes` is discarded and the call fails
- **Output separation**: The plugin's standard error is written to the mcp-rs log. Anything on its standard output that is not JSON-RPC is logged and ignored, so it never reaches your MCP client

## When something goes wrong

- **The plugin cannot start** (the command is not found, or it does not answer `initialize` and `tools/list` within `call_timeout_seconds`): mcp-rs logs a warning and starts without that plugin
- **The plugin exits or stops responding later**: mcp-rs keeps running. Calls to that plugin's tools return an error that says the plugin is not running. Restart mcp-rs to start the plugin again. mcp-rs does not restart plugins automatically
- **Requests from the plugin** (such as sampling): mcp-rs answers that they are not supported

## Notes

- Claude Desktop may start mcp-rs twice. Each mcp-rs starts its own plugin processes (or containers), so you may see two of each. Each mcp-rs stops its plugins when it exits
- For a plugin that runs on Node.js, set `command` to `node.exe` (or `node`) and put the server's script in `args`. `npx` does not work as `command` without `PATH` and other variables, which mcp-rs does not pass
