---
layout: page
title: Architecture
permalink: /docs/architecture/
---

# Architecture

**[Home]({{ site.baseurl }}/) > [Documentation]({{ site.baseurl }}/docs/) > Architecture**

How a request travels through mcp-rs, and where each part lives in the source.

## Overview

```text
MCP client ──STDIO or HTTP──▶ transport ──▶ McpServer ──▶ handlers
(Claude Desktop,              (src/transport)  (src/mcp)    ├─ WordPress   ──▶ WordPress REST API
 Claude Code, …)                                            ├─ Database    ──▶ PostgreSQL
                                                            └─ plugin:<name> ──▶ external MCP server
                                                                               (child process,
                                                                                Docker, Kubernetes)
```

1. **Startup** (`src/main.rs`): reads the configuration file (`src/config.rs`), sets up logging and the audit log, then builds the server. Each configured handler is registered: WordPress, the database, and one handler per plugin. A database or plugin that cannot start is skipped and reported by `mcp_rs_status`; mcp-rs still starts
2. **Transport** (`src/transport`): STDIO for clients that start mcp-rs themselves, or HTTP (with optional WebSocket on `/ws`) for clients over the network
3. **Server** (`src/mcp`): answers `initialize`, `tools/list`, `tools/call`, and `resources/*`. It negotiates the protocol version (up to 2025-11-25), collects the tools of all handlers, and sends each tool call to the handler that owns it
4. **Handlers** (`src/handlers`, `src/plugin_proxy`): implement the `McpHandler` trait. The WordPress and database handlers call their services directly; a plugin handler forwards the call to the plugin's MCP server

## HTTP mode

Requests over HTTP pass these checks before they reach the server (`src/transport/http.rs`, `src/security`):

| Check | Settings |
|-------|----------|
| Origin header (DNS rebinding protection) | `allowed_origins` |
| Network policy (allowed client addresses) | `network_policy_*` |
| TLS, minimum TLS version, mTLS | `tls_enabled`, `min_tls_version`, `mtls_enabled` |
| Anti-replay and intrusion detection | `anti_replay_enabled`, `ids_enabled` |
| Authentication: API key or OAuth 2.1 access token | `auth_enabled`, `api_keys`, `[transport.http.oauth]` |
| Tool limits per API key or token | `tools` (for example `@read_only`) |

STDIO mode has none of these, because the client that starts mcp-rs is the only one that can talk to it. See [Security](../security.md) for the settings.

In both modes, the WordPress handler and each plugin limit the rate of their tool calls (`[handlers.wordpress.rate_limit]`, and `rate_limit` of a plugin).

## Audit log

Every tool call, from every handler and in both modes, is written to the audit log (`src/security/audit_log.rs`): who called which tool, and whether it succeeded. See [Audit log](../security.md#audit-log).

## Source layout

| Path | Contents |
|------|----------|
| `src/main.rs`, `src/cli_args.rs` | Command line, startup, handler registration |
| `src/config.rs` | Configuration file, `${VAR}` expansion |
| `src/logging.rs` | Log files and their location |
| `src/mcp/` | MCP server, protocol versions, `mcp_rs_status`, tool limits |
| `src/transport/` | STDIO, HTTP, WebSocket, OAuth |
| `src/handlers/wordpress.rs` | WordPress tools |
| `src/handlers/database/` | Database tools (PostgreSQL) |
| `src/plugin_proxy/` | Plugins: starting them and forwarding calls |
| `src/security/` | Encrypted credentials, authentication, rate limiting, anti-replay, intrusion detection, audit log |
