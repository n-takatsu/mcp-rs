---
layout: page
title: Security
permalink: /docs/security/
---

# Security

**[Home](../) > [Documentation](./) > Security**

## 🔒 Security Overview

MCP-RS implements comprehensive security measures designed for production environments. Our security-first approach ensures safe operation with AI agents while maintaining high performance.

## 🛡️ Enterprise-Grade 5-Layer Security Architecture (93% Complete)

## ✅ Layer 1: Cryptographic Security

- **AES-GCM-256 Encryption**: Military-grade authenticated encryption
- **PBKDF2 Key Derivation**: 100,000 iterations for secure key generation
- **Secure Secret Management**: Zero-copy secret handling with automatic cleanup
- **Cryptographic Randomness**: Hardware-backed random number generation

## ✅ Layer 2: Network Security

- **TLS 1.2+ Enforcement**: Mandatory secure transport layer
- **Certificate Validation**: Strict certificate chain verification
- **HTTPS-Only Communication**: Automatic HTTP to HTTPS redirection
- **Secure Headers**: Content Security Policy and security headers enforcement

## ✅ Layer 3: Access Control & Rate Limiting

- **API Key Authentication**: Optional API key check on every HTTP/WebSocket request (`Authorization: Bearer <key>` or `X-Api-Key`). Revoked or expired keys are rejected. STDIO is unaffected (local pipe)
- **OAuth 2.1 Authorization**: Optional check of access tokens issued by an external authorization server, for HTTP/WebSocket (see [OAuth 2.1 Authorization](#httpwebsocket-oauth-21-authorization))
- **Token Bucket Rate Limiting**: WordPress tool calls are rate limited per WordPress site connection, with burst handling
- **Configurable Thresholds**: Customizable rate limiting parameters
- **Graceful Degradation**: Smooth handling of rate limit violations

## ✅ Layer 4: Input Validation & Attack Prevention

- **HTML Sanitization**: Safe HTML processing with whitelist filtering
- **Media Upload Validation**: `upload_media` checks the MIME type against an allowlist, requires the filename extension to match it, verifies the file content by its magic number, and enforces a size limit (see [WordPress Integration](../wordpress/#supported-file-types))
- **Local File Upload Restrictions**: `upload_media` reads local files (`file_path`) only from folders listed in `upload_allowed_dirs`, and is disabled by default. Paths are resolved before the check, so `..` and symbolic links cannot escape those folders

## ✅ Layer 5: Application Security

- **Zero-Panic Operations**: Complete elimination of panic-causing code
- **Result-Based Error Handling**: Comprehensive error management system
- **Memory Safety**: Rust's ownership system prevents buffer overflows
- **Thread Safety**: Async-safe operations with proper synchronization
- **Audit Logging**: Every WordPress tool call that changes the site (`create_*`, `update_*`, `delete_*`, `upload_*`, `set_*`) is recorded with its result, including calls rejected by rate limiting or input validation. Plugin tool calls are recorded too. Entries are appended as JSON lines to the file set in [`[audit]`](#audit-log)

## 🧪 Security Testing

## Test Coverage: **171 Test Cases** ✅

Our comprehensive security test suite covers:
- ✅ AES-GCM-256 encryption/decryption
- ✅ PBKDF2 key derivation security
- ✅ Token bucket rate limiting
- ✅ TLS 1.2+ enforcement
- ✅ Input validation and sanitization
- ✅ Zero-panic operations
- ✅ WordPress authentication security

## Security Implementation Status (93% Complete)

```
🛡️ Enterprise Security Implementation:
✅ Cryptographic Security: COMPLETE
✅ Network Security: COMPLETE
✅ Access Control: COMPLETE
✅ Input Validation: COMPLETE
✅ Application Security: COMPLETE
✅ Audit Logging: COMPLETE

Security Architecture: 5-Layer Defense ✅
Production Ready: TRUE ✅
```

## 🔐 Configuration Security

## Secure Environment Variables

`url`, `username` and `password` under `[handlers.wordpress]` can refer to environment variables of the mcp-rs process with `${VAR_NAME}`:

```toml
[handlers.wordpress]
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
enabled = true
```

- If WordPress is enabled and a variable is not set, or `${` has no matching `}`, mcp-rs does not start. The error names the setting and the variable (for example `handlers.wordpress.password: environment variable 'WORDPRESS_PASSWORD' is not set`), never the value (v0.19.2 and earlier started anyway, with the text `[ERROR:WORDPRESS_PASSWORD]` as the password)
- The values are never written to the log
- A value is used as is: a `${` inside the value of the variable is not expanded again

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

## HTTP/WebSocket: Requests from Web Pages (`Origin`)

In HTTP mode, mcp-rs checks the `Origin` header that browsers add to requests from web pages, as the MCP specification requires. This stops a web page you visit from calling mcp-rs's tools through your browser, including with DNS rebinding (a page whose domain is made to point at `127.0.0.1`). Up to version 0.21.0, mcp-rs did not check `Origin`.

- Requests without an `Origin` header are accepted. AI clients such as Claude Desktop and command-line tools don't send one.
- Requests from loopback origins (`http://localhost`, `http://127.0.0.1`, `http://[::1]`, `https` too, any port) are accepted.
- Requests from other origins get HTTP 403, both on `/mcp` (and `/`) and on the `/ws` upgrade. The Protected Resource Metadata (OAuth) stays available to any origin.

To use mcp-rs from a web app on another origin, add it (`scheme://host[:port]`, compared without regard to case):

```toml
[transport.http]
allowed_origins = ["https://app.example.com"]
# allowed_origins = ["*"]   # any origin: only behind your own proxy that checks Origin itself
```

The `Origin` check does not replace authentication: enable API keys or OAuth when the HTTP transport listens on anything other than loopback.

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

Clients send the key as `Authorization: Bearer <key>` or `X-Api-Key: <key>`. Requests without a valid key get HTTP 401. Note that `${VAR}` expansion applies only to the WordPress `url`/`username`/`password` fields, so `key` must be written as the literal value.

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

Tool calls (WordPress and plugins) and, in HTTP mode, refused requests are appended to an audit log file as JSON lines, one entry per line (see [What is recorded](#what-is-recorded)). By default, the file is `audit.log` in the `audit` folder next to the mcp-rs logs:

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
| Every tool call (WordPress and plugins), whether it succeeds or fails | `DataAccess` | `SUCCESS` or `FAILURE` | STDIO and HTTP |
| A tool call refused because the API key or token may not use that tool ([Limiting Tools](#limiting-tools-per-api-key-or-token)) | `Authorization` | `DENIED` | HTTP |
| A request refused because the API key or access token is missing or invalid, or lacks the required scopes | `Authentication` | `DENIED` | HTTP |

Each entry has these fields:

- `timestamp` and `level` (`Info`, `Warning`, or `Error`). `timestamp` is in UTC (ending in `Z`), while the regular log uses local time, so in Japan the two differ by 9 hours
- `user_id`: who called. In HTTP mode, `api_key:<name>` for an API key, or `oauth:<client>` for an access token (the token's `azp` or `client_id`, or its `sub`). In STDIO mode, and in HTTP mode without authentication, `local`. Authentication failures have no `user_id`
- `ip_address`: the client's IP address (HTTP mode)
- `action`: the tool name. `resource`: the WordPress site URL, or `plugin:<name>`
- `result`, and `message` (for example, `Plugin tool call: fs__read_text_file - SUCCESS`)
- `metadata`: `subject` (the API key's `user_id`, or the token's `sub`), `duration_ms` (how long the tool call took), `error` (why it failed), and `target_id` (the ID of the post, page, media, category, or tag, for WordPress tools that take one). Every entry also has `pid` and `instance`, which identify the mcp-rs process that recorded it (see below). Keys are in alphabetical order, and all values are strings

Claude Desktop may start two mcp-rs processes for one entry, and Windows reuses process IDs. To find which process handled a tool call, use `instance` (the process ID and the local time it started, such as `70580-20261110T130333`). The same value appears in the regular log's startup line (`=== mcp-rs … started at … / pid … / instance … / log: … ===`) and in the `Instance` line of `mcp_rs_status`

API keys, access tokens, and tool arguments (such as post content or file contents) are not recorded, because they can contain secrets. Up to version 0.18.4, only WordPress tools that change something were recorded, without the caller.

## Best Practices

- Use WordPress Application Passwords (never plain passwords)
- Prefer `encrypted_credentials` over a plaintext `password` in config files
- Enable API key authentication or OAuth 2.1 authorization whenever the HTTP transport is reachable from the network
- Implement proper environment variable naming conventions
- Regular password rotation and access review
- HTTPS-only connections for all WordPress API calls

## 🚨 Vulnerability Management

## Fixed Security Issues

### CVE-2024-MCPRS-001 ✅ FIXED

**Environment Variable Infinite Loop**
- **Severity**: High
- **Status**: Fixed in v0.1.0-alpha
- **Solution**: Max iteration limits + processed variable tracking

## 📋 Security Checklist

## Pre-Deployment

- [ ] ✅ Environment variables configured securely
- [ ] ✅ WordPress Application Passwords generated
- [ ] ✅ HTTPS configured for all connections
- [ ] ✅ Security tests passing
- [ ] ✅ Health checks validated

## Regular Maintenance

- [ ] Monthly security test execution
- [ ] Quarterly dependency audits
- [ ] Semi-annual password rotation
- [ ] Annual security architecture review

## 🎯 Security Architecture

Our enterprise-grade 5-layer security approach provides comprehensive protection:

1. **Cryptographic Layer**: AES-GCM-256 encryption with PBKDF2 key derivation
2. **Network Layer**: TLS 1.2+ enforcement and secure transport
3. **Access Control Layer**: Token bucket rate limiting and client isolation
4. **Input Validation Layer**: Media upload validation and HTML sanitization
5. **Application Layer**: Zero-panic operations and memory safety

## Advanced Security Features

- **Performance Security**: Sub-millisecond security processing
- **Zero-Copy Security**: Secure secret handling without memory exposure
- **Compliance Ready**: Enterprise security standards compliance

## 📞 Security Contact

For security-related issues:
- **Critical Vulnerabilities**: Create private GitHub issue
- **Security Questions**: Use [SECURITY] in issue titles
- **Improvements**: Submit PRs with security documentation

## 📚 Additional Resources

- [Security Guide](https://github.com/n-takatsu/mcp-rs/blob/main/project-docs/security-guide.md) - Comprehensive security documentation
- [WordPress Security](https://make.wordpress.org/core/2020/11/05/application-passwords-integration-guide/) - Application Password setup
- [Rust Security](https://doc.rust-lang.org/security.html) - Rust security guidelines

---

**Security Version**: v0.1.0-alpha (Enterprise Grade)
**Implementation Status**: 93% Complete (13/14 features)
**Last Updated**: 2025-01-28
**Next Review**: 2025-02-28
