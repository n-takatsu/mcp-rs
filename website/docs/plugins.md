---
layout: page
title: Plugins
permalink: /docs/plugins/
---

# Plugins

**[Home](../) > [Documentation](./) > Plugins**

A plugin is a separate MCP server that mcp-rs starts as a child process. mcp-rs connects to it as an MCP client and offers its tools alongside the built-in ones. Plugins can be written in any language, as long as they speak MCP over standard input/output.

> **Trusted plugins only.** In this first stage, a plugin runs as an ordinary process with your user's permissions. mcp-rs does not restrict which files or network addresses it can reach. Only add plugins you trust. Running plugins in a container is planned ([#392](https://github.com/n-takatsu/mcp-rs/issues/392)).

Plugins currently work in STDIO mode only (for example, Claude Desktop and Claude Code). In HTTP mode, `[[plugins]]` is ignored with a warning until [#400](https://github.com/n-takatsu/mcp-rs/issues/400) is fixed.

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

## How tools appear

Each tool is offered as `<plugin name>__<tool name>`, and its description starts with `[plugin: <name>]`. For example, the plugin `example` with a tool `search` becomes `example__search`. If a plugin tool has the same name as a built-in tool or another plugin's tool, the one registered first is used and a warning is logged. Built-in handlers are registered before plugins.

Only tools are relayed. A plugin's resources, prompts, and notifications (such as `notifications/tools/list_changed`) are not. mcp-rs reads the tool list once, when the plugin starts.

## What mcp-rs enforces

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
