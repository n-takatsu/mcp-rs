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

## 🎯 WordPress MCP Tools (41 tools available)

**📝 Content Management:**

- Create, read (full editable source), update, and delete posts and pages
- List revisions and restore a post or page to one of them
- Custom post types (products, events, ...) allowed in the config, and their own taxonomies
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
- Comment moderation: approve, mark as spam, reply, and delete

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

## ✅ Recently Completed (v0.30.0)

A stronger core for WordPress, MCP and setup. Some changes affect existing settings (for example, `ssl_mode` is now required for remote databases); see the [release notes](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.30.0) before updating.

- **🌐 Several WordPress sites**: One mcp-rs serves several sites, each under `[handlers.wordpress.sites.<name>]`, with a `site` argument on each tool (the default site when left out). The single-site form still works, and `mcp-rs init` can add more sites. See [WordPress](docs/wordpress/#sites)
- **📝 Comments, revisions and custom post types**: Create, moderate and delete comments, view revisions and restore one, and use the custom post types you allow in `post_types`, with their taxonomies (`terms`). `create_post_with_embeds` also embeds YouTube Shorts. See [Comments](docs/wordpress/#comments), [Revisions](docs/wordpress/#revisions) and [Custom post types](docs/wordpress/#custom-post-types)
- **💬 MCP prompts and resource templates**: Ready-made prompts in the client's menu (`draft_post`, `review_post_seo`, `moderate_comments`, `summarize_table`), and `wordpress://post/{id}`, `wordpress://page/{id}` and `wordpress://media/{id}` to read one item. See [Prompts](docs/wordpress/#prompts) and [API](docs/api/)
- **🧩 Claude Desktop extension**: Each release has `mcp-rs-<version>.mcpb`, with mcp-rs for Windows, macOS (Apple Silicon) and Linux. The site URL, username and Application Password are entered in Claude Desktop. See [Getting Started](docs/guides/#claude-desktop-extension)
- **🧭 Setup**: `mcp-rs init` can set up a database without WordPress. Without a config file, mcp-rs starts in STDIO mode with only `mcp_rs_status`, which says how to set it up. Tool descriptions were rewritten so AI clients pick the right tool. See [Getting Started](docs/guides/#mcp-rs-init)
- **🐳 Docker**: The image is a single statically linked (musl) executable on `gcr.io/distroless/static-debian12:nonroot`, without the Debian C library and OpenSSL packages. See the [Docker Guide](https://github.com/n-takatsu/mcp-rs/blob/main/docs/DOCKER_GUIDE.md)
- **🔒 Hardening**: Remote databases need `ssl_mode` (see [Database Integration](docs/database/#tls)), fewer false positives in the IDS, plugins are stopped on SIGTERM, and many fixes from reviews of earlier versions. On Windows, `mcp-rs init` writes config files with owner-only ACLs

## ✅ v0.29.0

Easier to set up and to use. Some changes affect existing settings; see the [release notes](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.29.0) before updating.

- **📦 Scoop and Homebrew**: `scoop install n-takatsu/mcp-rs` on Windows and `brew install n-takatsu/tap/mcp-rs` on macOS (Apple Silicon) and Linux (x64), from this project's bucket and tap, which follow new releases automatically. See [Getting Started](docs/guides/#package-managers)
- **🧭 `mcp-rs init` with databases**: `init` also sets up `[database]` (PostgreSQL, MySQL, MariaDB), with the password in the OS credential store (`keyring` in `[database]`). `mcp-rs database-keyring` stores it without `init`. See [Database Integration](docs/database/#keyring)
- **🧹 `mcp-rs uninstall`**: Removes the registrations in Claude Desktop, Claude Code and Copilot, and the credentials, asking before each step. See [Getting Started](docs/guides/#uninstall)
- **🔔 New versions**: `mcp-rs check` and `mcp_rs_status` say when a new version is available, with the update command for the way you installed mcp-rs
- **🔎 Searching posts**: `get_posts` and `get_pages` take a keyword, categories, tags, dates and a page, and show the total. `update_post` can schedule an existing post (`date`). See [WordPress](docs/wordpress/)
- **🔒 Hardening**: Limits on the size of WordPress responses, checks on plugin tool names, and plugin paths relative to the config file. Unused database modules (about 17,000 lines) were removed

## ✅ v0.28.0

- **🆕 MCP 2026-07-28**: mcp-rs accepts the new version of MCP, which has no `initialize` and no sessions (`server/discover`, and the version in each request's `_meta`), over STDIO and HTTP. Clients that use `initialize`, such as Claude Desktop, work as before. In HTTP mode, an `initialize` from one client no longer changes the protocol version for other clients. See [API](docs/api/)
- **🩺 `mcp-rs check`**: Also checks `[database]` (the settings, weak TLS, and connecting and signing in) and `[[plugins]]` (the command can be found, and each plugin starts and lists its tools). `--offline` checks only the settings. See [Getting Started](docs/guides/#check)
- **🖼️ Icon**: mcp-rs sends its logo in the server information, for clients that show it

## ✅ v0.27.0

A security release. Some fixes stop settings that used to work; see the [release notes](https://github.com/n-takatsu/mcp-rs/releases/tag/v0.27.0) before updating.

- **🔒 HTTP mode**: Connections that send no requests are closed after `request_timeout_seconds`, so idle HTTP/2 connections cannot fill `max_connections`. Behind a reverse proxy (`trusted_proxies`), mcp-rs needs an API key or OAuth to start. Addresses that fail authentication 20 times in a minute get 429, and `/ws` messages are checked by the IDS. See [Security](docs/security/)
- **🔒 `@read_only` and plugins**: `@read_only` keys no longer include plugin tools by default, because plugins set their own `readOnlyHint`. Use `trust_annotations = true` for plugins you have checked. See [Plugins](docs/plugins/)
- **🔒 Databases**: `ssl_mode = "verify-ca"` is refused on PostgreSQL and needs `ssl_root_cert` on MySQL and MariaDB. `execute_query` refuses row locks, and both tools refuse functions that touch the server's files or other connections (`LOAD_FILE()`, `pg_read_file()`, `dblink*` and others). See [Database Integration](docs/database/)
- **🧭 `mcp-rs init`**: Carries over settings that only the previous config file has (`[[plugins]]`, `[database]` and others), offers to delete backups with plaintext passwords, and writes files atomically with owner-only permissions on Unix
- **📦 Release provenance**: Release files now have GitHub build provenance attestations: `gh attestation verify <file> -R n-takatsu/mcp-rs`

## ✅ v0.26.0

- **⚡ Install Script**: One command installs mcp-rs on Windows, macOS (Apple Silicon), and Linux (x64): it downloads the latest release, checks its SHA256, and puts mcp-rs in `~/.mcp-rs/bin`, without administrator rights. See [Getting Started](docs/guides/#install-script)
- **🧭 `mcp-rs init`**: Sets up mcp-rs by asking a few questions: the WordPress credentials (kept in the OS credential store), the config file, a check of the connection and sign-in, and registering mcp-rs with Claude Desktop, Claude Code, and GitHub Copilot (VS Code). See [Getting Started](docs/guides/#mcp-rs-init)
- **🔒 MySQL and MariaDB `sql_mode`**: mcp-rs now removes `NO_BACKSLASH_ESCAPES` and `ANSI_QUOTES` from its sessions, so a server with these modes cannot make the SQL check read strings differently from the database. See [Database Integration](docs/database/)
- **📘 Getting Started**: The README now has a three-step quick start, and the [Getting Started](docs/guides/) guide covers installing by hand, setting up each client, and the log files

## ✅ v0.25.0

- **🐬 MySQL and MariaDB**: `[database]` now also connects to MySQL (`engine = "mysql"`, 8.0 or later) and MariaDB (`engine = "mariadb"`, 10.6 or later), with the same tools and limits as PostgreSQL: read-only by default, `?` parameters, time, row and size limits, and TLS. See [Database Integration](docs/database/)
- **📦 Ready-Built Files**: Each release now has mcp-rs for Windows (x64), macOS (Apple Silicon), and Linux (x64), with SHA256 checksums, so Rust is not needed. See [Getting Started](docs/guides/#install)
- **🩺 `mcp-rs check`**: Checks a config file without an MCP client: whether it loads, whether the WordPress credentials can be read, whether mcp-rs can reach the site and sign in, whether it can connect to the database, and whether each plugin starts
- **🧩 VS Code (GitHub Copilot)**: How to use mcp-rs from the Copilot chat in VS Code

## ✅ v0.24.0

- **🔑 OS Credential Store**: WordPress credentials can come from the Windows Credential Manager, the macOS Keychain, or the Linux Secret Service (`keyring = "<name>"`, registered with `mcp-rs wordpress-keyring set`), so Claude Desktop, Claude Code, and a terminal all read the same place. The entry remembers its site and is used only for that `url`. See [Security](docs/security/)
- **🔐 Security Review**: A review of the whole server. HTTP mode no longer starts on a network address without authentication, limits connections, request sizes and times, and closes idle and stalled WebSocket connections. Without `--config`, mcp-rs no longer reads a config file in the current folder. Logs are private to your user, database results are limited in size, and headers from a reverse proxy are used only from `trusted_proxies`. See [Security](docs/security/)
- **🧹 Config Examples**: Every example under `configs/` now loads in the current format, and a test keeps them that way

## ✅ v0.23.0

- **🗄️ Database Tools**: Add a `[database]` section to connect one PostgreSQL database. `execute_query` (SELECT) and `get_schema` are always available, and `execute_command` (INSERT, UPDATE, DELETE) only with `allow_write = true`. Queries run in a read-only transaction that is rolled back, values are bound as parameters, and results are limited in rows and time. Included in the default build. See [Database Integration](docs/database/)
- **🐳 Docker Image**: The image now starts with its default arguments. Mount a config file at `/app/config/mcp-config.toml`; see the [Docker guide](https://github.com/n-takatsu/mcp-rs/blob/main/docs/DOCKER_GUIDE.md)
- **🧹 Honest Code and Docs**: Removed settings and arguments that were accepted but never used (such as `PasswordHasher::new(salt_rounds)`), a module whose health checks always reported success, and documentation that described features mcp-rs does not have

## ✅ v0.22.0

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
