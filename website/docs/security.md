---
layout: page
title: Security
permalink: /docs/security/
---

# Security

**[Home](../) > [Documentation](./) > Security**

## Overview

What mcp-rs does to protect your WordPress site, your database, and the HTTP listener:

| Area | What mcp-rs does | Section |
|------|------------------|---------|
| Credentials | Reads secrets from environment variables (`${VAR}`), or stores the WordPress Application Password encrypted with AES-GCM-256 (PBKDF2 key derivation). Secrets are never written to the log | [Environment variables](#secure-environment-variables), [Encrypted credentials](#encrypted-wordpress-credentials) |
| HTTP listener | Refuses to listen on the network without authentication. Checks the client address, the `Origin` header, and the credentials before reading the request body. Limits the request size, the time to send a request, and the number of connections. Also TLS (minimum version, optional client certificates), replayed requests, and suspicious requests | [Listening on the network](#httpwebsocket-listening-on-the-network), [`Origin`](#httpwebsocket-requests-from-web-pages-origin) |
| Who may call | API keys, or OAuth 2.1 access tokens from an external authorization server (HTTP mode). STDIO mode needs neither: only the client that started mcp-rs can talk to it | [API keys](#httpwebsocket-api-key-authentication), [OAuth 2.1](#httpwebsocket-oauth-21-authorization) |
| What they may call | Per API key or token, the tools it may use (for example `@read_only`) | [Limiting tools](#limiting-tools-per-api-key-or-token) |
| How often | Rate limits on WordPress and plugin tool calls | [WordPress](../wordpress/) |
| WordPress input | `upload_media` checks the MIME type, extension, file content, and size, and reads local files only from `upload_allowed_dirs` (off by default) | [WordPress](../wordpress/#supported-file-types) |
| Database | Read-only by default: one `SELECT` per call, run in a read-only transaction. Writes only with `allow_write = true`. Values are bound as parameters. Results have a row and size limit. A warning at startup when TLS does not check the server | [Database](../database/#safety) |
| Plugins | Plugins can run in a Docker container or a Kubernetes Job to limit what they can reach | [Plugins](../plugins/) |
| Record | Every tool call is written to the audit log: who called which tool, and the result | [Audit log](#audit-log) |

## 🔐 Configuration Security

## Secure Environment Variables

These settings can refer to environment variables of the mcp-rs process with `${VAR_NAME}`:

- `url`, `username`, and `password` under `[handlers.wordpress]`
- `host`, `database`, `username`, and `password` under `[database]` (see [Database](../database/))
- The values of `env` of a plugin (see [Plugins](../plugins/))
- `key` of each `[[transport.http.api_keys]]` entry, in HTTP mode with `auth_enabled = true` (v0.24.0 and later; see [API Key Authentication](#httpwebsocket-api-key-authentication))

Other settings are used as written.

```toml
[handlers.wordpress]
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
enabled = true
```

- If WordPress is enabled and one of its variables is not set, or `${` has no matching `}`, mcp-rs does not start. The error names the setting and the variable (for example `handlers.wordpress.password: environment variable 'WORDPRESS_PASSWORD' is not set`), never the value (v0.19.2 and earlier started anyway, with the text `[ERROR:WORDPRESS_PASSWORD]` as the password)
- The values are never written to the log
- For `[database]`, mcp-rs starts without the database tools instead, and `mcp_rs_status` shows the reason. For a plugin's `env`, that entry is not passed to the plugin
- A value is used as is: a `${` inside the value of the variable is not expanded again
- `${...}` in `[handlers.wordpress]` is expanded only when `enabled = true`. Without it, the WordPress tools are not registered and no error is shown

### Which environment variables reach mcp-rs

`${VAR}` reads the environment of the mcp-rs process, and that environment depends on the client that starts it:

| Started by | Environment variables mcp-rs gets |
|---|---|
| Claude Code, a terminal | All of the parent's, including user environment variables (on Windows, after the client is restarted) |
| Claude Desktop | Only a fixed set such as `PATH`, plus what `env` in `claude_desktop_config.json` lists. User environment variables are not passed |

The `env` of the client (or `claude mcp add -e`) only passes values. Which variables mcp-rs reads is decided by the `${...}` in the config file. With `--config`, mcp-rs reads `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD` only through `${...}`; without `--config` (`~/.config/mcp-rs/config.toml`), it also reads them directly when `WORDPRESS_URL` is set, and they override the file (except with `keyring`).

### Which config file mcp-rs reads

With `--config <path>`, mcp-rs reads that file. Without it, mcp-rs reads only `~/.config/mcp-rs/config.toml` (v0.24.0 and later). Config files in the current folder (`mcp-config.toml`, `config.toml`, `config/mcp.toml`) are not read without `--config`; when mcp-rs finds no config file but one of these exists, it stops and tells you to pass it with `--config`.

Up to v0.23.0, mcp-rs also read these names from the current folder. Clients such as Claude Code start MCP servers in the folder you open, so with mcp-rs registered without `--config`, opening a repository that contains a `config.toml` made mcp-rs read it. Such a file could run any command as a plugin (`[[plugins]]`), or send your WordPress credentials to another site (`url` with `keyring`, or `password = "${ANY_VARIABLE}"`). Always register mcp-rs with `--config`.

To use mcp-rs from both Claude Desktop and Claude Code with environment variables, the same values must be written twice. Keep the credentials in the [OS credential store](#os-credential-store) instead.

## OS Credential Store

mcp-rs can read the WordPress username and Application Password from the OS credential store (v0.24.0 and later):

- Windows: Credential Manager (generic credential `<name>.mcp-rs`)
- macOS: Keychain (service `mcp-rs`, account `<name>`)
- Linux and other Unix: Secret Service (GNOME Keyring, KWallet)

No environment variables are involved, so Claude Desktop, Claude Code, and a terminal read the same credentials, and changing the password is one step.

1. Register the credentials under a name of your choice, such as the site's host name. It asks for the site URL, the username, and the Application Password (not shown while typing):

   ```bash
   mcp-rs wordpress-keyring set example.com
   ```

2. Refer to the name from the config file, and leave `username`, `password`, and `encrypted_credentials` out of the section:

   ```toml
   [handlers.wordpress]
   enabled = true
   url = "https://example.com"
   keyring = "example.com"
   ```

Run `set` again to change the password, `mcp-rs wordpress-keyring show example.com` to see the stored site and username (never the password), and `mcp-rs wordpress-keyring delete example.com` to remove it. Restart the client afterwards.

- Startup fails if the entry does not exist, or if `keyring` is combined with `username`, `password`, or `encrypted_credentials`. The error names the entry, never the values
- The entry is tied to the site URL entered with `set`. mcp-rs uses it only when `url` in the config file has the same scheme, host, and port (the path does not matter), and otherwise does not start. So a config file someone else wrote cannot name your entry with `url = "https://attacker.example"` and receive the password. Credentials stored for `https://` are not sent over `http://` either
- With `keyring`, the `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD` environment variables are not used, even without `--config`
- Secrets are never accepted as command-line arguments. Without a terminal, `set` reads the site URL, the username, and the Application Password as three lines from standard input
- The credential store is read only when `enabled = true`
- Docker, CI, and servers without a desktop session have no Secret Service. Keep using `${VAR}` or `encrypted_credentials` there; both work as before. The Docker image is built without this feature (`os-keyring`, on by default when building from source)
- What it protects against: the password is not left in plaintext in the config file, in `claude_desktop_config.json`, or in environment variables. It does not protect against other programs running as the same OS user, which can read the credential store too

## Encrypted WordPress Credentials

Instead of a plaintext `password`, the config file can hold the Application Password encrypted with AES-GCM-256. The master password used to decrypt it is read only from the `WORDPRESS_MASTER_PASSWORD` environment variable, never from the config file. Startup fails if `encrypted_credentials` is set and the variable is missing.

```toml
[handlers.wordpress]
url = "https://your-wordpress-site.com"
enabled = true

[handlers.wordpress.encrypted_credentials]
username = "your_username"
encrypted_password = "..."
nonce = "..."
salt = "..."
```

Generate the `encrypted_credentials` section with the `encrypt-wordpress-credentials` subcommand (v0.18.1 and later):

```bash
mcp-rs encrypt-wordpress-credentials
```

It asks for the WordPress username, the Application Password, and a master password (twice, at least 12 characters). Passwords are not shown while typing. It then prints the `[handlers.wordpress.encrypted_credentials]` section to standard output. Paste it into the config file after `[handlers.wordpress]`, remove any plaintext `password`, and start mcp-rs with the same master password in `WORDPRESS_MASTER_PASSWORD`.

- Secrets are never accepted as command-line arguments, so they do not end up in shell history or process listings
- Before printing, the command decrypts the generated values again to confirm they round-trip
- Without a terminal (for example in CI), it reads the username, Application Password, and master password as three lines from standard input instead, with no confirmation:

  ```bash
  mcp-rs encrypt-wordpress-credentials < secrets.txt > encrypted.toml
  ```

  Delete `secrets.txt` afterwards.

## HTTP/WebSocket: Listening on the Network

When the HTTP transport listens on anything other than loopback (for example `addr = "0.0.0.0"` in Docker, with `network_policy_reject_external_connections = false`), it needs authentication: API keys, OAuth 2.1, or client certificates (mTLS). Without any of them, mcp-rs does not start (v0.24.0 and later), because anyone who can reach the address could use every tool. Up to v0.23.0, it only logged a warning.

If something in front of mcp-rs authenticates the clients (for example a reverse proxy that requires a login, and mcp-rs is reachable only through it), allow it explicitly. mcp-rs then logs a warning at startup:

```toml
[transport.http]
allow_unauthenticated_external = true
```

mcp-rs checks a request in this order, so that a client that is not allowed cannot make mcp-rs read a large body or wait for a slow one:

1. When accepting the connection: the client address (`network_policy_reject_external_connections`, `network_policy_ip_whitelist`). A connection from another address is closed before the TLS handshake
2. After the request headers: the `Origin` header, and the API key or access token
3. Only then the request body

Limits (v0.24.0 and later):

```toml
[transport.http]
# max_request_size = 1048576          # bytes of a request body, and of a WebSocket message. Larger: HTTP 413
# request_timeout_seconds = 30        # the TLS handshake, the request headers, and each wait for more of the body
# max_connections = 1024              # connections served at once (WebSocket included); more wait
# websocket_idle_timeout_seconds = 300  # close /ws when the client stops answering for this long. 0: no limit
```

- `request_timeout_seconds` limits the TLS handshake and the request headers as a whole. It also limits how long a kept-alive connection waits for its next request. For the request body, it limits each wait for more data, not the total: a client that keeps sending a little at a time can take longer in total. This applies only after the client has passed the checks above. It does not limit how long a tool call takes
- On `/ws`, mcp-rs sends a ping every 30 seconds (or every half of `websocket_idle_timeout_seconds`, when that is shorter). Clients, browsers included, answer pings automatically, and the answers count as activity. So a client that is only quiet stays connected, and only a connection that answers nothing for `websocket_idle_timeout_seconds` is closed (close code 1001)
- A `/ws` connection opened with an OAuth access token is closed when the token expires (`exp`, plus `leeway_seconds`), with close code 1008. Up to v0.23.0, the token was checked only when connecting
- `max_connections` is shared by all clients, and there is no limit per client address. A client that is not allowed in can still hold connections until `request_timeout_seconds` passes, so many such connections can make other clients wait. When mcp-rs is reachable from the internet, put a reverse proxy in front of it that limits connections per address

## HTTP/WebSocket: Requests from Web Pages (`Origin`)

In HTTP mode, mcp-rs checks the `Origin` header that browsers add to requests from web pages, as the MCP specification requires. This stops a web page you visit from calling mcp-rs's tools through your browser, including with DNS rebinding (a page whose domain is made to point at `127.0.0.1`). Up to version 0.21.0, mcp-rs did not check `Origin`.

- Requests without an `Origin` header are accepted. AI clients such as Claude Desktop and command-line tools don't send one.
- Requests from loopback origins (`http://localhost`, `http://127.0.0.1`, `http://[::1]`, `https` too, any port) are accepted.
- Requests from other origins get HTTP 403, both on `/mcp` (and `/`) and on the `/ws` upgrade, before mcp-rs reads the request body. The Protected Resource Metadata (OAuth) stays available to any origin.
- CORS follows the same rule: browsers are allowed to send requests from the accepted origins only, so a request from another origin is already stopped at the browser's preflight check. With `cors_enabled = false`, mcp-rs sends no CORS headers at all, so pages on other origins cannot read its responses even when their origin is allowed.

To use mcp-rs from a web app on another origin, add it (`scheme://host[:port]`, compared without regard to case):

```toml
[transport.http]
allowed_origins = ["https://app.example.com"]
# allowed_origins = ["*"]   # any origin: only behind your own proxy that checks Origin itself
```

Because loopback origins are always accepted, a web page served from your own PC, on any port (for example a local development server, or another program's local web interface), can call mcp-rs's tools through your browser when no authentication is set up. If you run untrusted local web pages, or to be safe in general, set up API keys or OAuth: the `Origin` check does not replace authentication. Always enable authentication when the HTTP transport listens on anything other than loopback.

## HTTP/WebSocket API Key Authentication

When the HTTP transport listens on anything other than loopback, enable API key authentication. Each key is hashed in memory at startup and never logged. Startup fails if `auth_enabled = true` and no keys are configured, or if a key is empty or duplicated.

```toml
[transport.http]
auth_enabled = true

[[transport.http.api_keys]]
key = "replace-with-a-long-random-key"
name = "claude-desktop"
user_id = "user-1"
```

Clients send the key as `Authorization: Bearer <key>` or `X-Api-Key: <key>`. Requests without a valid key get HTTP 401.

To keep the key out of the config file, read it from an environment variable with `${VAR}` (v0.24.0 and later). This suits Docker, where the config file is often mounted or built into an image:

```toml
[[transport.http.api_keys]]
key = "${MCP_API_KEY}"
name = "claude-desktop"
user_id = "user-1"
```

- If the variable is not set or is empty, mcp-rs does not start. The error names the entry and the variable (for example `transport.http.api_keys[0].key (name 'claude-desktop'): environment variable 'MCP_API_KEY' is not set`, or `... is empty`), never the value
- `${...}` is expanded only in HTTP mode with `auth_enabled = true`, where the keys are used
- Up to v0.23.0, `${...}` in `key` was used as written. A key that contains `${` now has to come from an environment variable

## Limiting Tools per API Key or Token

In HTTP mode, each API key (and each OAuth scope) can be limited to some of the tools. Other tools are not shown in `tools/list`, and calls to them fail with a tool error ("Tool '...' is not allowed for this API key or access token"). Refused calls are recorded in the [audit log](#audit-log) (category `Authorization`, result `DENIED`, with the key name such as `api_key:reader` or the token's client such as `oauth:my-client`).

```toml
[[transport.http.api_keys]]
key = "replace-with-a-long-random-key"
name = "reader"
user_id = "user-1"
tools = ["@read_only"]            # omit to allow every tool

[[transport.http.api_keys]]
key = "replace-with-another-key"
name = "files"
user_id = "user-2"
tools = ["fs__*", "get_posts"]
```

Each item in `tools` is one of:

- A tool name, such as `get_posts`
- A pattern with `*`, which matches any text. For example, `fs__*` matches every tool of the plugin `fs`, and `delete_*` matches every tool whose name starts with `delete_`
- `@read_only`: tools marked as read-only (`readOnlyHint: true` in their annotations). The WordPress tools that only read (`get_*` and `wordpress_health_check`) are read-only. Plugin tools are read-only when the plugin says so in its own tool definitions, so check a plugin's tools before relying on this for it. Tools without annotations are not read-only

A request is allowed a tool when any item matches it. `tools = []` allows no tools. Unknown presets (such as `@admin`) and empty items stop mcp-rs from starting. STDIO mode is not limited (it is a local pipe used by one person).

The same limits apply to resources (`resources/list` and `resources/read`, v0.24.0 and later). Each resource belongs to the read-only tool that reads the same data, and a key or token can read the resource only when it is allowed that tool. `wordpress://posts` returns every published post with its full content, so it belongs to `get_post` (which returns one post with its content), not to `get_posts` (which returns only summaries):

| Resource | Tool |
|----------|------|
| `wordpress://posts` | `get_post` |
| `wordpress://comments` | `get_comments` |
| `wordpress://categories` | `get_categories` |
| `wordpress://tags` | `get_tags` |
| `db://schema` | `get_schema` |

Other resources are not shown in `resources/list`, and reading them fails with the error "Resource '...' is not allowed for this API key or access token". So `tools = []` allows no resources either, and `tools = ["@read_only"]` allows all of them. Up to version 0.23.0, keys and tokens with limits could list and read every resource.

## HTTP/WebSocket OAuth 2.1 Authorization

In HTTP mode, mcp-rs can accept OAuth 2.1 access tokens, as described in the MCP specification (versions 2025-06-18 and 2025-11-25, "Authorization"). mcp-rs is the resource server: users sign in at an external authorization server (for example Keycloak, Okta, or Microsoft Entra ID), which handles SSO, LDAP/Active Directory, and MFA. mcp-rs checks the access tokens that server issues.

```toml
[transport.http.oauth]
issuer = "https://auth.example.com/realms/mcp"   # the authorization server (iss)
resource = "https://mcp.example.com/mcp"         # the URL clients use to reach mcp-rs
# audiences = ["https://mcp.example.com/mcp"]    # accepted aud values. Default: resource
# jwks_uri = "https://auth.example.com/realms/mcp/protocol/openid-connect/certs"  # default: found from the server's metadata
# required_scopes = ["mcp:tools"]                # tokens without all of these get 403
# leeway_seconds = 60                            # allowed clock skew for exp/nbf
```

- Tokens must be JWTs sent as `Authorization: Bearer <token>`. mcp-rs checks the signature with the authorization server's public keys (JWKS), the issuer (`iss`), the expiry (`exp`, and `nbf` if present), and the audience (`aud`). A token issued for another service (another `aud`) is rejected. Signatures with a shared secret (HS256 etc.) and unsigned tokens are rejected. Opaque (non-JWT) tokens and token introspection are not supported
- Configure the authorization server to put `resource` (or one of `audiences`) in the token's `aud`. In Keycloak, add an "Audience" mapper to the client
- mcp-rs finds the JWKS from the authorization server's metadata (`/.well-known/oauth-authorization-server` or `/.well-known/openid-configuration`), unless `jwks_uri` is set. The keys are loaded at startup and reloaded every hour, and when a token uses an unknown key ID (at most once every 30 seconds)
- `issuer` and `jwks_uri` must use https. http is allowed only for `localhost` and loopback addresses, for testing
- mcp-rs serves the Protected Resource Metadata (RFC 9728) without authentication at `/.well-known/oauth-protected-resource` plus the path of `resource` (for the example above, `/.well-known/oauth-protected-resource/mcp`), and also at `/.well-known/oauth-protected-resource`. It tells clients which authorization server to use
- Requests without a valid token get HTTP 401 with `WWW-Authenticate: Bearer resource_metadata="..."` (and `error="invalid_token"` when a token was sent). Tokens without `required_scopes` get HTTP 403 with `error="insufficient_scope"`
- API keys can be used together with OAuth: when `auth_enabled = true` too, a request is accepted with either a valid API key or a valid access token
- To limit tools by scope, map scopes to tools with `scope_tools`. The items are written as in [`tools`](#limiting-tools-per-api-key-or-token). A token can use the tools of all its scopes that are listed. When `scope_tools` is set, a token with none of the listed scopes can use no tools. When it is not set (the default), every valid token can use every tool

```toml
[transport.http.oauth.scope_tools]
"mcp:read" = ["@read_only"]
"mcp:write" = ["*"]
```

## Audit Log

Tool calls (WordPress, database, and plugins) and, in HTTP mode, refused requests are appended to an audit log file as JSON lines, one entry per line (see [What is recorded](#what-is-recorded)). By default, the file is `audit.log` in the `audit` folder next to the mcp-rs logs:

- Windows: `%USERPROFILE%\.mcp-rs\audit\audit.log`
- Other platforms: `$XDG_STATE_HOME/mcp-rs/audit/audit.log`, or `~/.local/state/mcp-rs/audit/audit.log` without `XDG_STATE_HOME`

The mcp-rs log shows the path at startup (`📝 監査ログ: <path>`), and so does the `Audit log` line of `mcp_rs_status`.

```toml
[audit]
# enabled = true              # false: keep entries in memory only (no file)
# file_path = "audit.log"     # absolute path, or relative to the config file's folder
# max_file_size_mb = 100      # rotate to audit.log.1, audit.log.2, ... before this size. 0: never rotate
# max_files = 5               # number of rotated files to keep
```

- If the file cannot be written (for example, the folder is read-only), mcp-rs logs a warning at startup and keeps entries in memory only. Missing folders are created
- The audit log contains user names and IP addresses. On Linux and macOS, mcp-rs creates missing folders so that only your user can open them (mode `700`) and creates the file so that only your user can read and write it (`600`), whatever the umask. Existing folders and files keep their permissions; to tighten a file created by an older version, run `chmod 600 audit.log*` (and `chmod 700` on its folder). On Windows, the default folder under `%USERPROFILE%` is already limited to your user, SYSTEM, and Administrators
- Up to version 0.21.0, the default was `audit.log` in the same folder as the config file. With Claude Desktop, that is usually `%APPDATA%\Claude\mcp-rs\audit.log`, inside Claude's own folder, where Claude itself could not read it. mcp-rs does not move that file: entries written before the update stay there, and new entries go to the new location. When the old file exists, mcp-rs says where it is in its log at startup, every time it starts. To stop that message, delete the old file or move it somewhere else once you no longer need it there. To keep using the old location instead, set `file_path = "audit.log"` (relative to the config file's folder)
- In the Docker image, the default is `/home/nonroot/.local/state/mcp-rs/audit/audit.log` inside the container, which is lost when the container is recreated (and cannot be written at all with a read-only root file system, as in `docker-compose.yml`). Mount a writable volume at `/home/nonroot/.local/state/mcp-rs/audit`, or set `file_path` to an absolute path on a mounted volume
- Up to version 0.18.4, the file was `audit.log` in the folder mcp-rs was started from. When started from an MCP client such as Claude Desktop, that folder depends on the client, and the file may not have been written at all

### What is recorded

| Event | `category` | `result` | Recorded by |
|-------|------------|----------|-------------|
| Every tool call (WordPress, database, and plugins), whether it succeeds or fails | `DataAccess` | `SUCCESS` or `FAILURE` | STDIO and HTTP |
| Every resource read (`resources/read`, such as `wordpress://posts`), whether it succeeds or fails. `action` is the resource URI, and `resource` is the handler (`wordpress` or `database`) | `DataAccess` | `SUCCESS` or `FAILURE` | STDIO and HTTP |
| A tool call or resource read refused because the API key or token may not use that tool ([Limiting Tools](#limiting-tools-per-api-key-or-token)) | `Authorization` | `DENIED` | HTTP |
| A request refused because the API key or access token is missing or invalid, or lacks the required scopes. At most 100 per minute are recorded one by one, so that a flood of failed requests does not rotate the real entries out of the log. The rest are counted, and recorded as one entry when that minute ends (`Authentication failed N more times in the minute from <time> …`). That entry has no IP addresses | `Authentication` | `DENIED` | HTTP |

Each entry has these fields:

- `timestamp` and `level` (`Info`, `Warning`, or `Error`). `timestamp` is in UTC (ending in `Z`), while the regular log uses local time, so in Japan the two differ by 9 hours
- `user_id`: who called. In HTTP mode, `api_key:<name>` for an API key, or `oauth:<client>` for an access token (the token's `azp` or `client_id`, or its `sub`). In STDIO mode, and in HTTP mode without authentication, `local`. Authentication failures have no `user_id`
- `ip_address`: the client's IP address (HTTP mode)
- `action`: the tool name. `resource`: the WordPress site URL, the database connection (such as `postgresql mcp_reader@localhost:5432/app (read-only)`), or `plugin:<name>`
- `result`, and `message` (for example, `Plugin tool call: fs__read_text_file - SUCCESS`)
- `metadata`: `subject` (the API key's `user_id`, or the token's `sub`), `duration_ms` (how long the tool call took), `error` (why it failed), `target_id` (the ID of the post, page, media, category, or tag, for WordPress tools that take one), and `sql` (for database tools: the SQL statement, shortened to 2,000 characters). Every entry also has `pid` and `instance`, which identify the mcp-rs process that recorded it (see below). Keys are in alphabetical order, and all values are strings

Claude Desktop may start two mcp-rs processes for one entry, and Windows reuses process IDs. To find which process handled a tool call, use `instance` (the process ID and the local time it started, such as `70580-20261110T130333`). The same value appears in the regular log's startup line (`=== mcp-rs … started at … / pid … / instance … / log: … ===`) and in the `Instance` line of `mcp_rs_status`

API keys, access tokens, and tool arguments (such as post content or file contents) are not recorded, because they can contain secrets. The one exception is the SQL statement of database tools, so that you can see what was run; the values of its parameters (`$1`, `$2`, …) are not recorded. Do not write secrets into the SQL itself. In the `error` of database tools, everything in quotes is replaced by `…`, because PostgreSQL error messages can contain a parameter value. Up to version 0.18.4, only WordPress tools that change something were recorded, without the caller.

## Best Practices

- Use WordPress Application Passwords (never plain passwords)
- Prefer `encrypted_credentials` over a plaintext `password` in config files
- Enable API key authentication or OAuth 2.1 authorization whenever the HTTP transport is reachable from the network
- Implement proper environment variable naming conventions
- Regular password rotation and access review
- HTTPS-only connections for all WordPress API calls

## Reporting a Vulnerability

Report security problems privately through GitHub: on the repository's **Security** tab, choose **Report a vulnerability**. Please do not open a public issue for them.

## 📚 Additional Resources

- [WordPress Security](https://make.wordpress.org/core/2020/11/05/application-passwords-integration-guide/) - Application Password setup
- [Rust Security](https://doc.rust-lang.org/security.html) - Rust security guidelines
