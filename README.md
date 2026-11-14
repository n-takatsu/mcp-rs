# mcp-rs

A Rust implementation of a Model Context Protocol (MCP) server that gives AI clients tools for WordPress, PostgreSQL, and external MCP plugins.

> **[English](README.md)** | **[日本語](README.ja.md)**

[![Version](https://img.shields.io/badge/Version-v0.23.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.23.0)
![Implementation](https://img.shields.io/badge/WordPress_Tools-32_Available-green)
![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)

## Overview

`mcp-rs` is an MCP server written in Rust. MCP clients such as Claude Desktop and Claude Code connect to it over STDIO, and other clients over HTTP. It offers:

- 32 WordPress tools (posts, pages, media, categories, tags, comments, site settings) through the WordPress REST API
- Read-only SQL tools for one PostgreSQL database (writes only when allowed)
- The tools of external MCP servers, started as plugins (child process, Docker container, or Kubernetes Job)
- For HTTP mode: API key or OAuth 2.1 authentication, per-caller tool limits, TLS/mTLS, rate limiting, and an audit log

## 🚀 Key Features

## **Core Capabilities**

- **MCP over JSON-RPC 2.0**: Protocol versions up to 2025-11-25
- **Transports**: STDIO, and HTTP (with optional WebSocket on `/ws`)
- **Plugins**: Add the tools of any MCP server that speaks STDIO, run as a child process, in a Docker container, or as a Kubernetes Job. See [Plugins](https://n-takatsu.github.io/mcp-rs/docs/plugins/)
- **Configuration**: One TOML file, with `${VAR}` for values from environment variables
- **Status Tool**: `mcp_rs_status` shows the version, config file, log files, and what failed to start

## **WordPress Integration (32 Tools)**

- **Posts & Pages Management**: Create, read (full editable source), update, and delete posts and pages, with SEO metadata and scheduling
- **Media Management**: Upload with base64 (type, extension, and content validation), list, update metadata, delete, and set featured images
- **Categories & Tags Management**: Create, update, and delete, with hierarchical categories
- **Comments**: Retrieve comments
- **YouTube & Social Embeds**: Rich media integration with security validation
- **Site Settings**: Read and update site settings and the front page configuration
- **Health Check**: Verify connectivity, authentication, permissions, and media upload capability

### Live Demo Site

The `examples/wordpress_site_builder_demo.rs` example builds and updates the public WordPress demo site at [redring.jp](https://redring.jp/). It is a real-world showcase of how mcp-rs can bootstrap a site and organize content in production.

## **Security**

- **Encrypted Credentials**: The WordPress Application Password can be stored encrypted with AES-GCM-256 (PBKDF2 key derivation), or read from an environment variable
- **HTTP Mode Authentication**: API keys, or OAuth 2.1 access tokens (JWT) issued by an external authorization server
- **Per-Caller Tool Limits**: Allow each API key or token only the tools it needs (for example `@read_only`)
- **TLS/mTLS**: HTTPS with a minimum TLS version, and optional client certificates
- **Network Policy, Anti-Replay and Intrusion Detection** for the HTTP/WebSocket listener
- **Rate Limiting** of tool calls
- **Audit Logging**: Who called which tool, and the result

## **Database Integration (PostgreSQL)**

- **One Section to Connect**: Add `[database]` to the configuration file (`engine = "postgresql"`, host, database, user, and `password = "${VAR}"`)
- **Read-Only by Default**: `execute_query` (one SELECT, run in a read-only transaction that is rolled back) and `get_schema`
- **Writes Only When Allowed**: `execute_command` (one INSERT, UPDATE, or DELETE) is offered only with `allow_write = true`
- **Typed Parameters**: `$1`, `$2`, … are bound, never inserted into the SQL text, and strings are converted to the column's type
- **Limits**: Rows per result (1,000), time per statement (30 s), and connections (5) by default
- **Audit Logging**: Each call is recorded with its SQL; parameter values are not

Connect with a read-only database role. See the [Database Integration Guide](https://n-takatsu.github.io/mcp-rs/docs/database/).

## 🚀 Quick Start

## Prerequisites

- Rust 1.98 or later (`rust-version` in `Cargo.toml`; the latest stable is recommended)
- WordPress site with Application Passwords enabled
- Network access to WordPress REST API

## Installation

### WordPress Credentials

Keep the WordPress username and Application Password in the OS credential store (Windows Credential Manager, macOS Keychain, Secret Service on Linux; v0.24.0 and later). mcp-rs then reads them from one place, whether Claude Desktop, Claude Code, or a terminal starts it:

```bash
mcp-rs wordpress-keyring set example.com   # asks for the site URL, the username and the Application Password
```

```toml
[handlers.wordpress]
enabled = true            # required: without it, the WordPress tools are not registered (no error)
url = "https://example.com"
keyring = "example.com"   # the name used with `wordpress-keyring set`
```

The credentials are used only when `url` is the site you entered with `set` (same scheme, host, and port), so a config file that names the entry with another `url` cannot send them elsewhere.

Why not environment variables: Claude Desktop passes MCP servers only a fixed set such as `PATH` plus the `env` of `claude_desktop_config.json`, never your user environment variables, while Claude Code and terminals pass all of them. With `${WORDPRESS_PASSWORD}` you would write the same values twice. Environment variables and encrypted credentials still work; see [Security](https://n-takatsu.github.io/mcp-rs/docs/security/#which-environment-variables-reach-mcp-rs).

### Claude Desktop

Claude Desktop starts mcp-rs and talks to it over STDIO. In STDIO mode, standard output carries only MCP messages; logs go to a file (and standard error). Add mcp-rs to `claude_desktop_config.json` with absolute paths. On Windows, write paths with `/`, or write each `\` as `\\` in JSON. With the credentials in the OS credential store, no `env` is needed:

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:/Program Files/MCP-rs/mcp-rs.exe",
      "args": ["--config", "C:/Users/you/AppData/Roaming/Claude/mcp-rs/mcp-config.toml"]
    }
  }
}
```

- If the config file uses `${...}` instead, list every variable it reads in `env`. `env` only passes values; the `${...}` in the config file decides which variables are read. For example, `configs/production/claude-desktop.toml` reads `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD`, so all three are needed
- After changing the config file, the credentials, or `env`, quit Claude Desktop from the system tray (menu bar) and start it again. Closing the window leaves it running
- If mcp-rs does not start ("Server disconnected"), errors in the config file are not in the mcp-rs log file, because they happen before logging starts. Open the mcp-rs log from Claude Desktop's settings (Developer). The Microsoft Store version does not write to `%APPDATA%\Claude\logs`

> 📖 See [Claude Desktop Integration Guide](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md) (Japanese) for the settings and troubleshooting.

### Claude Code

Claude Code also talks to mcp-rs over STDIO and uses the same configuration file. Register mcp-rs with absolute paths. Everything after `--` is the mcp-rs command line. With the credentials in the OS credential store, no `-e` is needed:

```bash
claude mcp add mcp-rs --scope user -- "C:/Program Files/MCP-rs/mcp-rs.exe" --config C:/Users/you/AppData/Roaming/Claude/mcp-rs/mcp-config.toml
```

Claude Code passes its own environment to mcp-rs, so `${...}` can also read user environment variables (restart VS Code after changing them), or values passed with `-e`. Values passed with `-e` are stored in Claude Code's settings, so do not use `--scope project` (which writes them to the shared `.mcp.json`) with real credentials. Check the connection with `claude mcp list`, or `/mcp` inside Claude Code.

> 📖 See [Claude Code Integration Guide](./project-docs/CLAUDE_CODE_INTEGRATION.md) (Japanese) for `.mcp.json`, encrypted credentials, and troubleshooting.

### Setup

```bash
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo build --release
```

Then start mcp-rs with a config file. Ready-made config files are in `configs/production/`:

- `claude-desktop.toml`: STDIO, for Claude Desktop and Claude Code. WordPress credentials are read from the `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD` environment variables. To use the OS credential store instead, copy it and change `[handlers.wordpress]` as shown above
- `web-ui.toml`: HTTP, for web clients

```bash
./target/release/mcp-rs --config configs/production/claude-desktop.toml
```

`mcp-rs --version` prints the installed version, and `mcp-rs --help` lists the options (v0.19.1 and later). mcp-rs does not start when it gets an unknown option or `--config` without a path (v0.19.2 and later; v0.19.1 only logged a warning), so check the `args` of the mcp-rs entry in your MCP client's settings if it stops starting.

From an MCP client (for example, ask Claude), call the `mcp_rs_status` tool to see the version, the config file in use, where the log and audit log are written, and the state of each plugin (v0.19.2 and later).

To write your own config file instead, start from the minimal example below.

## Basic Configuration

Create `mcp-config.toml`:

```toml
[transport]
type = "stdio"          # "http" for HTTP mode (see [transport.http])

[server]
log_level = "warn"      # Logs go to a file, never to standard output in STDIO mode

[handlers.wordpress]
enabled = true
url = "https://your-wordpress-site.com"
username = "your-username"
password = "your-application-password"   # WordPress Application Password
```

Then run `./target/release/mcp-rs --config mcp-config.toml`. To keep the password out of the file, use `keyring = "<name>"` ([OS credential store](https://n-takatsu.github.io/mcp-rs/docs/security/#os-credential-store)), `${WORDPRESS_PASSWORD}`, or [encrypted credentials](https://n-takatsu.github.io/mcp-rs/docs/security/#encrypted-wordpress-credentials). mcp-rs does not start if a `${...}` variable is not set (in `[database]`, it starts without the database tools instead).

- `enabled = true` is required. Without it, mcp-rs does not register the WordPress tools and does not expand `${...}`, with no error
- Always pass the config file with `--config`. Without it, mcp-rs reads only `~/.config/mcp-rs/config.toml` (v0.24.0 and later). Config files in the current folder (`mcp-config.toml`, `config.toml`, `config/mcp.toml`) are not read without `--config`: clients such as Claude Code start mcp-rs in the folder you open, and a config file there could otherwise run commands (`[[plugins]]`) or send your credentials to another site. Up to v0.23.0, mcp-rs read them
- With `--config`, the credentials come only from the config file: `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD` are read only through `${...}`. Without `--config` (`~/.config/mcp-rs/config.toml`), mcp-rs also reads them directly when `WORDPRESS_URL` is set, and they override the file (except with `keyring`)
- In Docker and HTTP mode on a server, there is no OS credential store; use `${...}` or encrypted credentials as before

### HTTP TLS/mTLS Configuration (Optional)

When using HTTP transport with TLS and client certificate authentication (mTLS), set:

```toml
[transport]
transport_type = "http"

[transport.http]
addr = "127.0.0.1"
port = 8080
tls_enabled = true
tls_cert_path = "./certs/server.crt"
tls_key_path = "./certs/server.key"
mtls_enabled = true
mtls_ca_cert_path = "./certs/ca.crt"
enforce_https = true
min_tls_version = "1.3"
hsts_enabled = true
```

Notes:

- `mtls_enabled = true` requires `tls_enabled = true`
- `mtls_enabled = true` requires `mtls_ca_cert_path`
- Clients without a valid certificate signed by the configured CA are rejected at TLS handshake

### WebSocket

WebSocket is not a separate transport. In HTTP mode, set `enable_websocket_upgrade = true` to also accept MCP requests on `/ws`:

```toml
[transport]
transport_type = "http"

[transport.http]
enable_websocket_upgrade = true
```

### Log Files

mcp-rs writes its log to the first of these folders that it can write to:

1. `logs` in the folder of the `mcp-rs` executable (for example, where you extracted the zip)
2. The user folder: `%USERPROFILE%\.mcp-rs\logs` on Windows, `$XDG_STATE_HOME/mcp-rs/logs` or `~/.local/state/mcp-rs/logs` elsewhere. When mcp-rs is installed in `C:\Program Files\MCP-rs`, the log is always here
3. `logs` in the current folder, then `mcp-rs/logs` in the temporary folder

The `mcp_rs_status` tool shows the log file in use. It says `fallback` only when mcp-rs could not write to 1 or 2.

The file name ends with the date, such as `mcp-rs.log.2026-11-09`, and a new file starts at midnight local time. The date in the file name and the times inside the log are both in local time (older versions used the UTC date in the file name). When a running mcp-rs moves to the next day's file, the new file starts with a line such as `=== mcp-rs … continued at … / pid … / instance … / log: … ===`, so each file shows which process wrote it. When two processes are running (see below), the new file gets one such line from each, with different `instance` values.

Claude Desktop may start two mcp-rs processes for one entry in `claude_desktop_config.json`. This is how Claude Desktop works, not a problem with your mcp-rs settings. Both processes write to the same log file, so the log can look doubled:

- Each line has the process ID (`[pid …]`), so you can tell the two processes apart
- Each process writes its own startup line (`=== mcp-rs … started at … / pid … / instance … / log: … ===`). Windows reuses process IDs, so the same `pid` can belong to a different process after a restart. Read `[pid …]` together with the latest startup line above it. The startup line also has `instance` (the process ID and start time), which the audit log records with every tool call, so you can match the two logs
- Startup messages, such as the WordPress connection check, appear once per process. Seeing them twice is normal

The audit log (who called which tool) is written next to the logs, to `%USERPROFILE%\.mcp-rs\audit\audit.log` on Windows (`$XDG_STATE_HOME/mcp-rs/audit` or `~/.local/state/mcp-rs/audit` elsewhere). `[audit] file_path` changes it. Up to version 0.21.0, it was `audit.log` in the config file's folder; that file is left where it is. See [Security](https://n-takatsu.github.io/mcp-rs/docs/security/#audit-log).

## 🏗️ Architecture

```text
┌─────────────────────┐    ┌─────────────────────┐    ┌─────────────────────┐
│   AI Agent/Client   │───▶│    MCP Server       │───▶│   WordPress Site    │
│  (Claude Desktop,   │    │   (mcp-rs)          │    │   (REST API)        │
│   Custom AI, etc)   │    │                     │    │                     │
└─────────────────────┘    └─────────────────────┘    └─────────────────────┘
                                      │
                                      ▼
                           ┌─────────────────────┐
                           │   Security Layer    │
                           │ • Authentication    │
                           │ • Tool Limits       │
                           │ • Rate Limiting     │
                           │ • Audit Log         │
                           └─────────────────────┘
```

## 📚 Documentation

The documentation site is at [n-takatsu.github.io/mcp-rs](https://n-takatsu.github.io/mcp-rs/docs/):

- [WordPress](https://n-takatsu.github.io/mcp-rs/docs/wordpress/)
- [Database (PostgreSQL)](https://n-takatsu.github.io/mcp-rs/docs/database/)
- [Plugins](https://n-takatsu.github.io/mcp-rs/docs/plugins/)
- [Security](https://n-takatsu.github.io/mcp-rs/docs/security/) (HTTP authentication, tool limits, TLS, audit log)
- [API Reference](https://n-takatsu.github.io/mcp-rs/docs/api/) (`mcp_rs_status` and the MCP methods)
- [Architecture](https://n-takatsu.github.io/mcp-rs/docs/architecture/)
- [Claude Desktop Integration](./project-docs/CLAUDE_DESKTOP_INTEGRATION.md) (Japanese)
- [Claude Code Integration](./project-docs/CLAUDE_CODE_INTEGRATION.md) (Japanese)
- [Docker Guide](./docs/DOCKER_GUIDE.md)

## 🛠️ Development

## Building from Source

```bash

## Development build

cargo build

## Production build with optimizations

cargo build --release

## Run tests

cargo test

## Run with specific configuration

cargo run -- --config configs/development/http-transport.toml
```

## Code Examples

Examples are in the `examples/` directory. For example, `cargo run --example wordpress_api_test` checks the connection to the WordPress site given by `WORDPRESS_URL`. When `WORDPRESS_USERNAME` and `WORDPRESS_PASSWORD` are also set, it checks the login and creates a draft post.

## Testing

```bash

## Run all tests

cargo test

## Run with output

cargo test -- --nocapture  # Show test output (no-capture flag) # cSpell:ignore nocapture

## Run specific test module

cargo test wordpress_api
```

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is dual-licensed under:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

You may choose either license at your option.

## 🙏 Acknowledgments

- [Model Context Protocol](https://modelcontextprotocol.io/) by Anthropic
- [WordPress REST API](https://developer.wordpress.org/rest-api/)
- The Rust community for excellent tooling and libraries

## 📞 Support

- 📖 [Documentation](./docs/)
- 🐛 [Issue Tracker](https://github.com/n-takatsu/mcp-rs/issues)

---

Built with ❤️ in Rust 🦀
