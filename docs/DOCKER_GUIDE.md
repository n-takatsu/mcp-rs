# Running mcp-rs in Docker

The image contains only the `mcp-rs` executable, on a distroless base (no shell), running as the non-root user `nonroot` (UID 65532). Settings come from a config file that you mount into the container.

## Getting the image

Release images are published to the GitHub Container Registry:

```bash
docker pull ghcr.io/n-takatsu/mcp-rs:latest     # latest release
docker pull ghcr.io/n-takatsu/mcp-rs:0.23       # a release line
```

Or build it from the repository:

```bash
docker build -t mcp-rs:latest .
```

`docker run --rm mcp-rs:latest --version` prints the version.

## The config file

mcp-rs reads `/app/config/mcp-config.toml`. Mount a folder that contains it:

```bash
-v /path/to/config:/app/config:ro
```

Without it, mcp-rs exits with `設定ファイルが存在しません: /app/config/mcp-config.toml`. To use another file name, pass `--config /app/config/<name>.toml` after the image name.

The config file is the same as outside Docker; see [Getting Started](https://n-takatsu.github.io/mcp-rs/docs/guides/). Pass values used as `${VAR}` in it with `-e`:

```bash
-e WORDPRESS_PASSWORD="your application password"
```

## STDIO mode (Claude Desktop, Claude Code)

The client starts the container and talks to it over standard input and output. Use `-i` (keep standard input open) and do **not** use `-t`: a terminal would mix extra characters into the JSON-RPC messages.

`mcp-config.toml`:

```toml
[server]
log_level = "info"

[transport]
type = "stdio"

[handlers.wordpress]
enabled = true
url = "https://your-wordpress-site.com"
username = "your_username"
password = "${WORDPRESS_PASSWORD}"
```

Claude Desktop (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-v", "/path/to/config:/app/config:ro",
        "-e", "WORDPRESS_PASSWORD",
        "ghcr.io/n-takatsu/mcp-rs:latest"
      ],
      "env": { "WORDPRESS_PASSWORD": "your application password" }
    }
  }
}
```

`-e WORDPRESS_PASSWORD` without a value passes the variable from `env` into the container.

Claude Code:

```bash
claude mcp add mcp-rs --scope user \
  -e WORDPRESS_PASSWORD="your application password" \
  -- docker run -i --rm -v /path/to/config:/app/config:ro \
     -e WORDPRESS_PASSWORD ghcr.io/n-takatsu/mcp-rs:latest
```

## HTTP mode

Inside a container, mcp-rs must listen on all interfaces, and connections arrive from Docker's network rather than from loopback. `network_policy_reject_external_connections = true` (the default) refuses both, so set it to `false`, and require an API key. From v0.24.0, mcp-rs does not start on `0.0.0.0` without authentication (API keys, OAuth, or mTLS):

```toml
[server]
log_level = "info"

[transport]
type = "http"

[transport.http]
addr = "0.0.0.0"
port = 8081
network_policy_reject_external_connections = false
auth_enabled = true

[[transport.http.api_keys]]
key = "${MCP_API_KEY}"
name = "client"
user_id = "user-1"

[handlers]
```

```bash
docker run -d --name mcp-rs \
  -p 127.0.0.1:8081:8081 \
  -e MCP_API_KEY="replace-with-a-long-random-key" \
  -v /path/to/config:/app/config:ro \
  ghcr.io/n-takatsu/mcp-rs:latest
```

`key = "${MCP_API_KEY}"` reads the key from the environment variable passed with `-e` (v0.24.0 and later), so the config file holds no secret. mcp-rs does not start if the variable is not set or is empty. Before v0.24.0, write the key itself in `key`.

The endpoint is `http://127.0.0.1:8081/mcp`. `-p 127.0.0.1:8081:8081` publishes the port on this machine only. To reach it from other machines, put a TLS proxy in front of it, or enable TLS in mcp-rs; see [Security](https://n-takatsu.github.io/mcp-rs/docs/security/).

### Docker Compose

`docker-compose.yml` runs mcp-rs in HTTP mode with `./config/mcp-config.toml` (the comment at the top of the file shows a config), a read-only root filesystem, and no Linux capabilities. It also has an optional PostgreSQL service.

```bash
docker compose up -d mcp-server
docker compose logs -f mcp-server
```

## Logs and the audit log

- Logs are written to standard error, so `docker logs` shows them
- Log files go to `/app/logs`. When the root filesystem is read-only, they go to `/home/nonroot/.local/state/mcp-rs/logs` instead
- The audit log goes to `/home/nonroot/.local/state/mcp-rs/audit/audit.log`
- `mcp_rs_status` (a tool every client can call) shows the version, the config file, and where the logs and the audit log are written

Files written inside the container are lost when it is removed (`--rm`, or when Docker Compose recreates it). To keep the audit log, mount a **named volume at `/home/nonroot`**:

```bash
docker run -i --rm \
  -v /path/to/config:/app/config:ro \
  -v mcp-rs-state:/home/nonroot \
  ghcr.io/n-takatsu/mcp-rs:latest
```

The first time a new named volume is used, Docker copies `/home/nonroot` from the image into it, owned by `nonroot`, so mcp-rs can write there. Mounting at a deeper path such as `/home/nonroot/.local/state/mcp-rs` does not work: that folder is not in the image, so Docker creates it owned by root, and mcp-rs falls back to the temporary folder. A folder on the host (a bind mount) works only if it is owned by UID 65532.

`docker-compose.yml` mounts the named volume `mcp-state` at `/home/nonroot`, and keeps the root filesystem read-only, so the logs go there too.

## Troubleshooting

| Symptom | Cause |
|---------|-------|
| `設定ファイルが存在しません: /app/config/mcp-config.toml` | The config folder is not mounted, or the file has another name |
| `error: unknown argument '--transport'` | Options from older guides. The only options are `--config`, `--version`, and `--help`; the transport is set in the config file |
| `missing field ...` when starting | The config file must have `[server]`, `[transport]`, and `[handlers]` (empty is fine) |
| HTTP mode exits with `Refusing to bind to non-loopback address 0.0.0.0:8081` | `network_policy_reject_external_connections` is not `false` |
| HTTP mode exits with `refusing to listen on 0.0.0.0:8081 without authentication` (v0.24.0 and later) | No API key, OAuth, or mTLS is configured. Add `auth_enabled = true` and an API key as above. Only when a reverse proxy in front of mcp-rs authenticates clients, set `allow_unauthenticated_external = true` |
| HTTP requests get 401 | The API key is missing or wrong (`Authorization: Bearer <key>` or `X-Api-Key: <key>`) |
| The client in STDIO mode gets garbled messages | `-t` was used; use `-i` only |
