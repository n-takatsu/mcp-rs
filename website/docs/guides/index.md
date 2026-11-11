---
layout: page
title: Getting Started
permalink: /docs/guides/
---

# Getting Started

**[Home]({{ site.baseurl }}/) > [Documentation]({{ site.baseurl }}/docs/) > Getting Started**

## 1. Build

```bash
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo build --release
```

The executable is `target/release/mcp-rs` (`mcp-rs.exe` on Windows). The default build includes the WordPress, database (PostgreSQL), and plugin support.

`mcp-rs --version` prints the version, and `mcp-rs --help` lists the options. The only other option is `--config <path>`.

## 2. Write a configuration file

Create `mcp-config.toml`:

```toml
[transport]
type = "stdio"          # "http" for HTTP mode

[server]
log_level = "warn"

[handlers.wordpress]
enabled = true
url = "https://your-wordpress-site.com"
username = "your_username"
password = "${WORDPRESS_PASSWORD}"   # read from the environment
```

Add only what you use:

- `[handlers.wordpress]`: see [WordPress]({{ site.baseurl }}/docs/wordpress/)
- `[database]`: see [Database]({{ site.baseurl }}/docs/database/) and the [PostgreSQL Integration Guide](./postgres-integration.md)
- `[[plugins]]`: see [Plugins]({{ site.baseurl }}/docs/plugins/)
- `[transport.http]` for HTTP mode, `[audit]` for the audit log: see [Security]({{ site.baseurl }}/docs/security/)

`${VAR}` is replaced by the environment variable `VAR` when mcp-rs starts. If a variable used by `[handlers.wordpress]` is not set, mcp-rs does not start, and the error names the setting.

## 3. Connect a client

### Claude Desktop

Add mcp-rs to `claude_desktop_config.json` with absolute paths, and pass the environment variables in `env`:

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "/path/to/mcp-rs",
      "args": ["--config", "/path/to/mcp-config.toml"],
      "env": { "WORDPRESS_PASSWORD": "your application password" }
    }
  }
}
```

Claude Desktop does not always pass your own environment variables to mcp-rs, so put them in `env`.

### Claude Code

```bash
claude mcp add mcp-rs --scope user \
  -e WORDPRESS_PASSWORD="your application password" \
  -- /path/to/mcp-rs --config /path/to/mcp-config.toml
```

### HTTP clients

Set `type = "http"` under `[transport]` and the address under `[transport.http]`. Turn on authentication before the listener is reachable from other machines: see [Security]({{ site.baseurl }}/docs/security/).

## 4. Check

Ask the client to call `mcp_rs_status`. It shows the version, the config file in use, where the log and the audit log are written, and anything that failed to start, with the reason. See the [API Reference]({{ site.baseurl }}/docs/api/).

## Guides

- [PostgreSQL Integration Guide](./postgres-integration.md) - Connect a database with a read-only role
