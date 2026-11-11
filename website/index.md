---
layout: home
title: MCP-RS Documentation
permalink: /
---

Welcome to the MCP-RS documentation! This site contains comprehensive guides, API references, and architecture documentation for the Rust implementation of the Model Context Protocol.

## Quick Access

- **[Getting Started](./docs/guides/)** - Implementation guides and tutorials
- **[API Reference](./docs/api/)** - Complete API documentation
- **[Architecture](./docs/architecture/)** - System design and technical architecture

## Documentation Hub

- **[Full Documentation](./docs/)** - Complete documentation index
- **[GitHub Repository](https://github.com/n-takatsu/mcp-rs)** - Source code and issues

## What is MCP-RS?

MCP-RS is a Model Context Protocol (MCP) server written in Rust. MCP clients such as Claude Desktop and Claude Code connect to it over STDIO, and other clients over HTTP, to use tools for WordPress, a PostgreSQL database, and external MCP servers added as plugins.

## Key Features

### 🚀 Core Capabilities

- **MCP over JSON-RPC 2.0**: Protocol versions up to 2025-11-25
- **Request Dispatch**: `McpServer` lists tools from every handler and routes each call to the handler that provides it, in both STDIO and HTTP modes
- **Transports**: STDIO, and HTTP (with optional WebSocket on `/ws`)
- **Plugins**: Add the tools of any MCP server that speaks STDIO, run as a child process, in a Docker container, or as a Kubernetes Job. See [Plugins](docs/plugins/)
- **Configuration**: One TOML file, with `${VAR}` for values from environment variables
- **Status Tool**: `mcp_rs_status` shows the version, config file, log files, and what failed to start

## 🔒 Security Features

- **Encrypted Credentials**: The WordPress Application Password can be stored encrypted with AES-GCM-256 (PBKDF2 key derivation), or read from an environment variable
- **HTTP Mode Authentication**: API keys, or OAuth 2.1 access tokens issued by an external authorization server
- **Per-Caller Tool Limits**: Allow each API key or token only the tools it needs (for example `@read_only`)
- **TLS/mTLS**: HTTPS with a minimum TLS version, and optional client certificates
- **Rate Limiting** of WordPress and plugin tool calls
- **Audit Logging**: Who called which tool, and the result

See [Security](docs/security/) for the settings.

## 🎯 WordPress MCP Tools (32 tools available)

**📝 Content Management:**

- Create, read (full editable source), update, and delete posts and pages
- Advanced post creation with SEO and scheduling
- YouTube and social media embeds
- Category and tag management

**🖼️ Media Management:**

- Upload media files (base64, with type, extension, and content validation)
- Featured image management
- Media library operations
- Accessibility support (alt text, captions)

**🔗 Content Integration:**

- Create posts with taxonomy
- Comment retrieval

**⚙️ Site Management:**

- Read and update site settings
- Front page configuration
- Health check of connectivity, authentication, and permissions

See [WordPress Integration](docs/wordpress/) for the full list of tools.

## 🗄️ Database MCP Tools (PostgreSQL)

- **`execute_query`**: One `SELECT`, run in a read-only transaction that is rolled back
- **`get_schema`**: Tables, columns, keys, and indexes
- **`execute_command`**: One `INSERT`, `UPDATE`, or `DELETE`, only with `allow_write = true`
- Values are bound as parameters; results are limited in rows and time

See [Database Integration](docs/database/) for the settings.

## Current Status

## ✅ Recently Completed (v0.22.0)

- **🆕 MCP Protocol 2025-11-25**: Clients that ask for the newest protocol version, such as Claude Desktop, now get it. Clients using `2025-06-18` or `2024-11-05` keep working as before
- **🔐 Requests from Web Pages**: In HTTP mode, mcp-rs checks the `Origin` header and answers requests from web pages on other origins with 403, so a page you visit cannot call its tools through your browser. CORS follows the same rule, and `allowed_origins` adds origins. See [Security](docs/security/)
- **📝 Audit Log Location**: The audit log is now written next to the logs, to `%USERPROFILE%\.mcp-rs\audit\audit.log` on Windows, and on Linux and macOS it is readable by your user only. The old `audit.log` next to the config file is left in place

## ✅ v0.21.0

- **📂 Log Files by Local Date**: Log files now switch at local midnight, so the date in the file name matches the times in the log. When a running mcp-rs switches to a new file, it writes a `continued` line with its `pid` and `instance` at the top
- **🔎 Which Process Did It**: Each audit log record has `pid` and `instance` in `metadata`, matching the log's header lines and the `Instance` line of `mcp_rs_status`, so you can tell the two processes Claude Desktop starts apart
- **🔒 Dependencies**: jsonwebtoken 11.1 with the `aws_lc_rs` backend, the same crypto library rustls already uses

## ✅ v0.20.0

- **🧹 Cleanup**: Removed modules, features, and dependencies that the server never used (725 → 591 crates; OpenSSL is no longer needed), and corrected the documentation to match what mcp-rs actually does
- **🔒 Environment Variables in the Config File**: Values expanded from `${VAR}` are no longer written to the debug log. With WordPress enabled, mcp-rs does not start when a referenced variable is missing, and names the setting and the variable. See [Security](docs/security/)
- **📂 Log Location**: When installed in `C:\Program Files\MCP-rs`, the log in `%USERPROFILE%\.mcp-rs\logs` is no longer reported as a fallback

## ✅ v0.19.2

- **🩺 Status Tool**: Ask your AI client to call `mcp_rs_status` to see the version, the config file in use, where the log and audit log are written, and the state of each plugin. See [API](docs/api/#mcp_rs_status)
- **🛑 Unknown Options**: mcp-rs no longer starts with an unknown option or `--config` without a path, so a typo does not silently use another config file

## ✅ v0.19.1

- **🏷️ Version and Help**: `mcp-rs --version` prints the installed version, and `mcp-rs --help` lists the options

## ✅ v0.19.0

- **🔐 OAuth 2.1 for HTTP Mode**: Accept access tokens from an external authorization server such as Keycloak, Okta, or Microsoft Entra ID, as described in the MCP specification. See [Security](docs/security/#httpwebsocket-oauth-21-authorization)
- **🧰 Tools per API Key or Token**: Limit which tools each API key or OAuth scope may use, for example read-only tools only. See [Security](docs/security/#limiting-tools-per-api-key-or-token)
- **📝 Audit Log**: Choose where the audit log is written (by default, next to the config file), with rotation. Every tool call is recorded with the caller, result, and duration. See [Security](docs/security/#audit-log)
- **🆕 MCP Protocol 2025-06-18**: Tool annotations (read-only, destructive), `outputSchema`, and `structuredContent` are passed on to clients that use the new version
- **📦 Plugins in Containers and Kubernetes**: Run a plugin in a Docker container or as a Kubernetes Job, with no network access by default. See [Plugins](docs/plugins/)

## ✅ v0.18.4

- **🌐 HTTP Mode Works**: Requests over HTTP (`/mcp`) and WebSocket (`/ws`) are now answered, with the same tools as STDIO mode, including plugins. Previously every request timed out
- **🔀 Concurrent Clients**: Each HTTP request gets its own response, even when several clients use the same JSON-RPC `id`

## ✅ v0.18.3

- **🔌 Plugins**: Add tools from other MCP servers, run as separate processes. mcp-rs relays them as `<plugin>__<tool>` with rate limiting, audit logging, and timeouts (STDIO mode). See [Plugins](docs/plugins/)
- **🔗 Post Slugs**: Set the URL slug when creating or updating a post or page, for example an English slug for a Japanese title
- **🧩 Multiple Handlers in STDIO Mode**: Tools from all handlers are listed and each call goes to the handler that provides the tool

## ✅ v0.18.2

- **🔁 Safer Retries**: Create and delete requests to WordPress are no longer resent after a timeout or 5xx error, so media is not uploaded twice
- **🧹 Cleanup**: Removed unused modules that were never connected to the server (interactive setup, `src/k8s`, and the earlier plugin implementations)

## ✅ v0.18.1

- **🖼️ Upload Local Files**: `upload_media` can read a file from folders allowed in `upload_allowed_dirs`, instead of base64 data
- **📌 Featured Image in `get_post`**: Shows the featured image's media ID and URL
- **📄 Full Post Content in `get_post`**: Returns the whole content, including the editable source with Gutenberg block markup
- **🔐 Credential Encryption Command**: `mcp-rs encrypt-wordpress-credentials` generates the `encrypted_credentials` section
- **🔌 MCP Protocol Fixes**: STDIO and HTTP no longer answer JSON-RPC notifications
- **🗑️ Media Deletion Fix**: `delete_media` with `force: true` no longer reports an error after deleting

## ✅ v0.18.0

- **🔑 API Key Authentication**: Optional API key check on every HTTP/WebSocket request
- **🚦 Rate Limiting**: WordPress tool calls are rate limited per site
- **📝 Audit Logging**: Every WordPress tool call that changes the site is recorded, including rejected ones
- **🔐 Encrypted Credentials**: WordPress Application Passwords can be stored encrypted in the config file
- **🖼️ Media Upload Validation**: MIME allowlist, extension and content checks, and size limits for `upload_media`
- **🛡️ Dependency Audits**: Known vulnerabilities fixed, and `cargo audit` now runs daily

See the [Security documentation](docs/security/) for details.

For older versions, see the [CHANGELOG](https://github.com/n-takatsu/mcp-rs/blob/main/CHANGELOG.md) and the [releases](https://github.com/n-takatsu/mcp-rs/releases).
