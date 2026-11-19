# <img src="website/assets/favicon.svg" alt="" width="36" height="36"> mcp-rs

A Rust implementation of a Model Context Protocol (MCP) server that gives AI clients tools for WordPress, databases (PostgreSQL, MySQL, MariaDB), and external MCP plugins.

Ask Claude to draft, schedule, and publish WordPress posts, upload media, or moderate comments. Install it with Scoop or Homebrew, answer a few questions in `mcp-rs init`, and it is registered with Claude Desktop, Claude Code, and GitHub Copilot.

> **[English](README.md)** | **[日本語](README.ja.md)**

[![Version](https://img.shields.io/badge/Version-v0.29.0-blue)](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.29.0)
![Implementation](https://img.shields.io/badge/WordPress_Tools-41_Available-green)
![License](https://img.shields.io/badge/License-MIT%2FApache--2.0-green)

## Overview

`mcp-rs` is an MCP server written in Rust. MCP clients such as Claude Desktop and Claude Code connect to it over STDIO, and other clients over HTTP. It offers:

- 41 WordPress tools (posts, pages, custom post types, media, categories, tags and other taxonomies, comments, site settings) through the WordPress REST API
- Read-only SQL tools for one PostgreSQL, MySQL, or MariaDB database (writes only when allowed)
- The tools of external MCP servers, started as plugins (child process, Docker container, or Kubernetes Job)
- For HTTP mode: API key or OAuth 2.1 authentication, per-caller tool limits, TLS/mTLS, rate limiting, and an audit log

## 🚀 Key Features

## **Core Capabilities**

- **MCP over JSON-RPC 2.0**: Protocol versions up to 2025-11-25 (with `initialize`), and 2026-07-28 (`server/discover` and per-request `_meta`, over STDIO and HTTP `/mcp`)
- **Transports**: STDIO, and HTTP (with optional WebSocket on `/ws`)
- **Plugins**: Add the tools of any MCP server that speaks STDIO, run as a child process, in a Docker container, or as a Kubernetes Job. See [Plugins](https://n-takatsu.github.io/mcp-rs/docs/plugins/)
- **Configuration**: One TOML file, with `${VAR}` for values from environment variables
- **Status Tool**: `mcp_rs_status` shows the version, config file, log files, and what failed to start

## **WordPress Integration (41 Tools)**

- **Posts & Pages Management**: Create, read (full editable source), update, and delete posts and pages, with SEO metadata and scheduling; list revisions and restore one
- **Media Management**: Upload with base64 (type, extension, and content validation), list, update metadata, delete, and set featured images
- **Categories & Tags Management**: Create, update, and delete, with hierarchical categories
- **Comments**: List by status (awaiting moderation, spam and others), approve, mark as spam, reply, and delete
- **YouTube & Social Embeds**: Rich media integration with security validation
- **Site Settings**: Read and update site settings and the front page configuration
- **Health Check**: Verify connectivity, authentication, permissions, and media upload capability
- **Custom Post Types**: Products, events and other post types that the site exposes in the REST API, when allowed in `post_types`, with their own taxonomies ([docs](https://n-takatsu.github.io/mcp-rs/docs/wordpress/#custom-post-types))
- **Prompts**: Ready-made requests in the client's menu: draft a post, review a post's SEO, moderate comments (and summarize a database table)
- **Claude Desktop Extension**: Install `mcp-rs-<version>.mcpb` from a release and enter the site in Claude Desktop's settings ([docs](https://n-takatsu.github.io/mcp-rs/docs/guides/#claude-desktop-extension))
- **Multiple Sites**: Use several WordPress sites from one mcp-rs; each tool takes a `site` argument ([docs](https://n-takatsu.github.io/mcp-rs/docs/wordpress/#sites))

### Live Demo Site

The `examples/wordpress_site_builder_demo.rs` example builds and updates the public WordPress demo site at [redring.jp](https://redring.jp/). It is a real-world showcase of how mcp-rs can bootstrap a site and organize content in production.

## **Security**

- **Credentials Out of Files**: `mcp-rs init` keeps the WordPress Application Password and the database password in the OS credential store. They can also be stored encrypted with AES-GCM-256 (PBKDF2 key derivation), or read from an environment variable
- **HTTP Mode Authentication**: API keys, or OAuth 2.1 access tokens (JWT) issued by an external authorization server
- **Per-Caller Tool Limits**: Allow each API key or token only the tools it needs (for example `@read_only`)
- **TLS/mTLS**: HTTPS with a minimum TLS version, and optional client certificates
- **Network Policy, Anti-Replay and Intrusion Detection** for the HTTP/WebSocket listener
- **Rate Limiting** of tool calls
- **Audit Logging**: Who called which tool, and the result

## **Database Integration (PostgreSQL, MySQL, MariaDB)**

- **One Section to Connect**: Add `[database]` to the configuration file (`engine = "postgresql"`, or `"mysql"` or `"mariadb"` (v0.25.0 and later), host, database, user, and `password = "${VAR}"`)
- **Read-Only by Default**: `execute_query` (one SELECT, run in a read-only transaction that is rolled back) and `get_schema`
- **Writes Only When Allowed**: `execute_command` (one INSERT, UPDATE, or DELETE) is offered only with `allow_write = true`
- **Bound Parameters**: `$1`, `$2`, … (PostgreSQL) or `?` (MySQL, MariaDB) are bound, never inserted into the SQL text, and strings are converted to the column's type
- **Limits**: Rows per result (1,000), time per statement (30 s), and connections (5) by default
- **Audit Logging**: Each call is recorded with its SQL; parameter values are not

Connect with a read-only database role. See the [Database Integration Guide](https://n-takatsu.github.io/mcp-rs/docs/database/).

## 🚀 Quick Start

You need a WordPress site with Application Passwords (Users > Profile > Application Passwords) that mcp-rs can reach over HTTPS. Rust is not needed: each release from v0.25.0 has ready-built mcp-rs for Windows (x64), macOS (Apple Silicon), and Linux (x64).

**1. Install** with a package manager (from v0.29.0). It installs the release's ready-built mcp-rs, and updates it with your other packages (`scoop update`, `brew upgrade`):

```powershell
# Windows (Scoop)
scoop bucket add n-takatsu https://github.com/n-takatsu/scoop-bucket
scoop install n-takatsu/mcp-rs
```

```bash
# macOS (Apple Silicon) and Linux (x64) (Homebrew)
brew install n-takatsu/tap/mcp-rs
```

Or with the install script (from v0.26.0). It downloads the latest release, checks its SHA256, and puts mcp-rs in `~/.mcp-rs/bin` (`%USERPROFILE%\.mcp-rs\bin` on Windows), without administrator rights or changing anything else. Run it again to update:

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 | iex
```

```bash
# macOS (Apple Silicon) and Linux (x64)
curl -fsSL https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh | sh
```

**2. Set up.** Run `mcp-rs init` (the install script offers to run it). It asks for the site URL, the username and the Application Password (kept in the OS credential store, not in a file), writes the config file (`~/.config/mcp-rs/config.toml`), checks the connection and sign-in, and registers mcp-rs with Claude Desktop, Claude Code and GitHub Copilot (VS Code), asking before each change:

```bash
mcp-rs init
# Installed with the script: ~/.mcp-rs/bin/mcp-rs init
# (Windows: & "$env:USERPROFILE\.mcp-rs\bin\mcp-rs.exe" init)
```

**3. Check.** Restart the client as `init` shows, then ask it to call the `mcp_rs_status` tool. It shows the version, the config file in use, where the logs are written, anything that failed to start, and whether a newer version is out. `mcp-rs check --config <path>` checks a config file without a client.

To remove what `init` set up (the client registrations, the credential store entries, and optionally the config file), run `mcp-rs uninstall`. It asks before each change, and shows how to remove mcp-rs itself for the way you installed it.

The [Getting Started guide](https://n-takatsu.github.io/mcp-rs/docs/guides/) covers the rest: reading the install script before running it, installing by hand or from source, setting up each client by hand, the config file, where the logs are, and what to check when a client cannot find the tools.

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

- [Getting Started](https://n-takatsu.github.io/mcp-rs/docs/guides/) (installing, setting up each client, log files)
- [WordPress](https://n-takatsu.github.io/mcp-rs/docs/wordpress/)
- [Database (PostgreSQL, MySQL, MariaDB)](https://n-takatsu.github.io/mcp-rs/docs/database/)
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
