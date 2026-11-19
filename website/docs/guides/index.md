---
layout: page
title: Getting Started
permalink: /docs/guides/
---

# Getting Started

**[Home]({{ site.baseurl }}/) > [Documentation]({{ site.baseurl }}/docs/) > Getting Started**

mcp-rs runs on your computer. Claude Desktop, Claude Code, and the Copilot chat in VS Code start it and talk to it over STDIO. This guide installs mcp-rs, sets it up for a WordPress site, and connects it to those clients.

The short way is two commands: the [install script](#install-script), then [`mcp-rs init`](#mcp-rs-init). The other sections describe the same steps done by hand, and where to look when something does not work.

- [1. Install](#install)
- [2. Set up with `mcp-rs init`](#mcp-rs-init)
- [3. Set up by hand](#by-hand)
- [4. Check](#check)
- [5. Log files](#log-files)
- [HTTP clients](#http)

## 1. Install {#install}

You need a WordPress site with Application Passwords (WordPress 5.6 and later: Users > Profile > Application Passwords) that mcp-rs can reach over HTTPS.

Each [release](https://github.com/n-takatsu/mcp-rs/releases) from v0.25.0 has ready-built mcp-rs for these systems. Rust is not needed:

| System | File |
|---|---|
| Windows (x64) | `mcp-rs-<version>-x86_64-pc-windows-msvc.zip` |
| macOS (Apple Silicon) | `mcp-rs-<version>-aarch64-apple-darwin.tar.gz` |
| Linux (x64, glibc 2.35 or later) | `mcp-rs-<version>-x86_64-unknown-linux-gnu.tar.gz` |

### Install script {#install-script}

From v0.26.0, the install script downloads the latest release, checks its SHA256, puts mcp-rs in `~/.mcp-rs/bin` (`%USERPROFILE%\.mcp-rs\bin` on Windows) without changing anything else (no PATH or other settings), and then offers to run [`mcp-rs init`](#mcp-rs-init). No administrator rights are needed. Run it again to update; on Windows, this works even while a client is running mcp-rs (the client uses the new one after a restart):

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 | iex
```

```bash
# macOS (Apple Silicon) and Linux (x64)
curl -fsSL https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh | sh
```

To read the script first, download it ([install.ps1](https://github.com/n-takatsu/mcp-rs/blob/main/install/install.ps1), [install.sh](https://github.com/n-takatsu/mcp-rs/blob/main/install/install.sh)) and run the file. `MCP_RS_VERSION` (such as `v0.25.0`) installs another release, `MCP_RS_INSTALL_DIR` another folder, and `MCP_RS_NO_INIT=1` skips the question about `mcp-rs init`.

### By hand {#install-by-hand}

Put `mcp-rs` (`mcp-rs.exe`) in a folder of your own, such as `~/.mcp-rs/bin` (`%USERPROFILE%\.mcp-rs\bin` on Windows). No administrator rights are needed, and updating means replacing the file. mcp-rs writes its log to `logs` next to it. On Windows, avoid folders under `AppData`: Claude Desktop from the Microsoft Store may redirect files written there, so the log can end up somewhere else.

Windows (PowerShell):

```powershell
$v = "v0.25.0"; $name = "mcp-rs-$v-x86_64-pc-windows-msvc"
Invoke-WebRequest "https://github.com/n-takatsu/mcp-rs/releases/download/$v/$name.zip" -OutFile "$name.zip"
Invoke-WebRequest "https://github.com/n-takatsu/mcp-rs/releases/download/$v/$name.zip.sha256" -OutFile "$name.zip.sha256"
# True if the file is the one built by the release workflow
(Get-FileHash "$name.zip" -Algorithm SHA256).Hash.ToLower() -eq (Get-Content "$name.zip.sha256").Split(" ")[0]
Expand-Archive "$name.zip" -DestinationPath .
New-Item -ItemType Directory -Force "$env:USERPROFILE\.mcp-rs\bin" | Out-Null
Copy-Item "$name\mcp-rs.exe" "$env:USERPROFILE\.mcp-rs\bin\"
Unblock-File "$env:USERPROFILE\.mcp-rs\bin\mcp-rs.exe"
& "$env:USERPROFILE\.mcp-rs\bin\mcp-rs.exe" --version
```

macOS and Linux:

```bash
v=v0.25.0; name=mcp-rs-$v-aarch64-apple-darwin   # Linux: mcp-rs-$v-x86_64-unknown-linux-gnu
curl -LO https://github.com/n-takatsu/mcp-rs/releases/download/$v/$name.tar.gz
curl -LO https://github.com/n-takatsu/mcp-rs/releases/download/$v/$name.tar.gz.sha256
shasum -a 256 -c $name.tar.gz.sha256   # Linux: sha256sum -c $name.tar.gz.sha256
tar -xzf $name.tar.gz
mkdir -p ~/.mcp-rs/bin && cp $name/mcp-rs ~/.mcp-rs/bin/
~/.mcp-rs/bin/mcp-rs --version
```

The files are not signed:

- Windows may warn about a downloaded program. `Unblock-File` (above) removes the mark that Windows puts on downloaded files. If you extracted the zip in Explorer, Windows may show "Windows protected your PC" the first time; choose "More info" → "Run anyway" only after checking the SHA256
- macOS blocks a program downloaded with a browser ("cannot be opened because the developer cannot be verified"). After checking the SHA256, run `xattr -d com.apple.quarantine ~/.mcp-rs/bin/mcp-rs`. Files downloaded with `curl` are not blocked
- `SHA256SUMS` in the release lists the SHA256 of every file
- The SHA256 files come from the same release, so they show that the download is complete, not who made it. From v0.27.0, each `.zip` and `.tar.gz` also has a build provenance attestation, signed by GitHub, that shows it was built by this repository's release workflow. With the [GitHub CLI](https://cli.github.com/), check it before running the file: `gh attestation verify mcp-rs-v0.27.0-x86_64-pc-windows-msvc.zip -R n-takatsu/mcp-rs`

### Build from source {#build-from-source}

Rust 1.98 or later is needed (`rust-version` in `Cargo.toml`; the latest stable is recommended):

```bash
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo build --release
```

The executable is `target/release/mcp-rs` (`mcp-rs.exe` on Windows). The default build includes WordPress, the databases (PostgreSQL, MySQL, MariaDB), the plugins, and the OS credential store. Copy it to a folder of your own as above.

`mcp-rs --version` prints the version, and `mcp-rs --help` lists the options and commands. mcp-rs does not start when it gets an unknown option or `--config` without a path, so check the `args` of the mcp-rs entry in your MCP client's settings if it stops starting.

## 2. Set up with `mcp-rs init` {#mcp-rs-init}

From v0.26.0, `mcp-rs init` asks a few questions in a terminal and does the rest:

1. The WordPress site URL (`https://` only), username and Application Password. They are kept in the OS credential store (Windows Credential Manager, macOS Keychain, Secret Service on Linux), never in the config file. The password is not shown while typing
2. It writes the config file, `~/.config/mcp-rs/config.toml`, which mcp-rs also reads without `--config`. `--config <path>` writes another file. It asks before replacing a file, and keeps a backup copy (when the backup has a password in plain text, it offers to delete it). On macOS and Linux, the file and its folder are readable by you only
3. It asks whether the AI should also read a database (PostgreSQL, MySQL or MariaDB, v0.29.0 and later). If so, it asks for the engine, host, port, database, user and password, and the TLS setting (`verify-full` is the default for a database on another machine), and adds `[database]` to the config file. The password goes to the OS credential store (`keyring` in `[database]`), not the file. Writing data (`allow_write`) stays off unless you allow it, and it asks twice. When the config file already has `[database]`, it is kept as it is. When you keep an existing config file, only the `[database]` section is added (a backup copy is kept). See [Database Integration](../database/)
4. It checks the config file like [`mcp-rs check`](#check): it connects to the site and signs in, and connects to the database. If the sign-in fails, it asks for the username and the Application Password again. If only the database cannot be used, it still registers mcp-rs: mcp-rs starts without the database tools until `[database]` is fixed
5. It registers mcp-rs with Claude Desktop, Claude Code and GitHub Copilot (VS Code), asking for each one. For the ones you skip, or whose command is not found, it prints the command or the settings to add yourself

```bash
~/.mcp-rs/bin/mcp-rs init        # Windows: & "$env:USERPROFILE\.mcp-rs\bin\mcp-rs.exe" init
```

If a client already starts mcp-rs with another config file (for example one you registered by hand), and no client uses the new config file yet, `init` asks whether that is the setup the new config file replaces (answer n for one you keep, such as for another site). If it is, `init` lists what that file has and a new config file for WordPress would not, such as `[[plugins]]`, `[database]`, `upload_allowed_dirs`, or `[audit]` (v0.27.0 and later). You choose to copy those settings into the new config file (the default; the WordPress username and password go to the OS credential store, not the file), to keep using that config file as it is, or to leave them out. When that file has a password in plain text, `init` reminds you at the end to delete it or remove the password. When the new mcp-rs is in another folder than the one you used before, `init` also shows where the log is now written: mcp-rs writes it next to its executable when it can.

If a client already starts mcp-rs under another name (for example `mcp-rs-wordpress`, registered by hand), `init` offers to remove that one, so that the same tools do not appear twice. One that uses the same config file, or the config file you just replaced, is removed unless you answer n; one that uses another config file (such as for a second site) is kept unless you answer y.

At the end, `init` shows how to restart each client. Then ask the client to call `mcp_rs_status` (see [Check](#check)).

## 3. Set up by hand {#by-hand}

### WordPress credentials {#credentials}

Keep the WordPress username and Application Password in the OS credential store (v0.24.0 and later). mcp-rs then reads them from one place, whether Claude Desktop, Claude Code, or a terminal starts it:

```bash
mcp-rs wordpress-keyring set example.com   # asks for the site URL, the username and the Application Password
```

The credentials are used only when the config file's `url` is the site you entered with `set` (same scheme, host, and port), so a config file that names the entry with another `url` cannot send them elsewhere. `mcp-rs wordpress-keyring show example.com` shows the site and the username (never the password).

Why not environment variables: Claude Desktop passes MCP servers only a fixed set such as `PATH` plus the `env` of `claude_desktop_config.json`, never your user environment variables, while Claude Code and terminals pass all of them. With `${WORDPRESS_PASSWORD}` you would write the same values twice. Environment variables and encrypted credentials still work; see [Security]({{ site.baseurl }}/docs/security/#which-environment-variables-reach-mcp-rs). In Docker and in HTTP mode on a server, there is no OS credential store; use `${...}` or encrypted credentials.

### The config file {#config-file}

Create `~/.config/mcp-rs/config.toml` (or another file, passed with `--config`):

```toml
[transport]
type = "stdio"          # "http" for HTTP mode

[server]
log_level = "warn"      # Logs go to a file, never to standard output in STDIO mode

[handlers.wordpress]
enabled = true            # required: without it, the WordPress tools are not registered (no error)
url = "https://example.com"
keyring = "example.com"   # the name used with `wordpress-keyring set`
```

- Pass the config file with `--config` and an absolute path, or put it at `~/.config/mcp-rs/config.toml`. Without `--config`, mcp-rs reads only `~/.config/mcp-rs/config.toml` (v0.24.0 and later). A relative path is read from the folder mcp-rs starts in, which for Claude Code is the folder you open, so mcp-rs warns about it at startup. Config files in the current folder (`mcp-config.toml`, `config.toml`, `config/mcp.toml`) are not read without `--config`: a config file in a folder you open could otherwise run commands (`[[plugins]]`) or send your credentials to another site
- `${VAR}` is replaced by the environment variable `VAR` when mcp-rs starts. If a variable used by `[handlers.wordpress]` is not set, mcp-rs does not start (in `[database]`, it starts without the database tools instead). With `--config`, the credentials come only from the config file: `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD` are read only through `${...}`. Without `--config` (`~/.config/mcp-rs/config.toml`), mcp-rs also reads them directly when `WORDPRESS_URL` is set, and they override the file (except with `keyring`)
- Ready-made config files are in [`configs/`](https://github.com/n-takatsu/mcp-rs/tree/main/configs). `configs/production/claude-desktop.toml` reads the credentials from `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD`; to use the OS credential store instead, change `[handlers.wordpress]` as above

Add only what you use:

- `[handlers.wordpress]`: see [WordPress]({{ site.baseurl }}/docs/wordpress/)
- `[database]`: see [Database]({{ site.baseurl }}/docs/database/) and the [PostgreSQL Integration Guide](./postgres-integration.md)
- `[[plugins]]`: see [Plugins]({{ site.baseurl }}/docs/plugins/)
- `[transport.http]` for HTTP mode, `[audit]` for the audit log: see [Security]({{ site.baseurl }}/docs/security/)

### Claude Desktop {#claude-desktop}

Add mcp-rs to `claude_desktop_config.json` (Settings > Developer > Edit Config) with absolute paths. On Windows, write paths with `/`, or write each `\` as `\\` in JSON. With the credentials in the OS credential store, no `env` is needed:

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:/Users/you/.mcp-rs/bin/mcp-rs.exe",
      "args": ["--config", "C:/Users/you/.config/mcp-rs/config.toml"]
    }
  }
}
```

- If the config file uses `${...}` instead, list every variable it reads in `env`. `env` only passes values; the `${...}` in the config file decides which variables are read. For example, `configs/production/claude-desktop.toml` reads `WORDPRESS_URL`, `WORDPRESS_USERNAME`, and `WORDPRESS_PASSWORD`, so all three are needed
- After changing the config file, the credentials, or `env`, quit Claude Desktop from the system tray (menu bar) and start it again. Closing the window leaves it running
- If mcp-rs does not start ("Server disconnected"), errors in the config file are not in the mcp-rs log file, because they happen before logging starts. Open the mcp-rs log from Claude Desktop's settings (Developer). The Microsoft Store version does not write to `%APPDATA%\Claude\logs`. `mcp-rs check --config <path>` shows the error without Claude Desktop
- Register mcp-rs once. Two entries that start mcp-rs with the same config file give the AI the same tools twice

More settings and troubleshooting (Japanese): [Claude Desktop Integration Guide](https://github.com/n-takatsu/mcp-rs/blob/main/project-docs/CLAUDE_DESKTOP_INTEGRATION.md).

### Claude Code {#claude-code}

Claude Code also uses the same config file. Register mcp-rs with absolute paths. Everything after `--` is the mcp-rs command line. With the credentials in the OS credential store, no `-e` is needed:

```bash
claude mcp add mcp-rs --scope user -- "C:/Users/you/.mcp-rs/bin/mcp-rs.exe" --config C:/Users/you/.config/mcp-rs/config.toml
```

Claude Code passes its own environment to mcp-rs, so `${...}` can also read user environment variables (restart VS Code after changing them), or values passed with `-e`. Values passed with `-e` are stored in Claude Code's settings, so do not use `--scope project` (which writes them to the shared `.mcp.json`) with real credentials. Check the connection with `claude mcp list`, or `/mcp` inside Claude Code.

More (Japanese): [Claude Code Integration Guide](https://github.com/n-takatsu/mcp-rs/blob/main/project-docs/CLAUDE_CODE_INTEGRATION.md).

### VS Code (GitHub Copilot) {#vs-code}

The Copilot chat in VS Code runs on the Copilot CLI (the agent host; the older local harness, which reads `.vscode/mcp.json` itself, is to be removed in February 2027) and uses the MCP servers registered for it. Register mcp-rs with the `copilot` command, with absolute paths. Everything after `--` is the mcp-rs command line. This writes `~/.copilot/mcp-config.json`, which both the chat in VS Code and `copilot` in a terminal read:

```bash
copilot mcp add mcp-rs -- "C:/Users/you/.mcp-rs/bin/mcp-rs.exe" --config C:/Users/you/.config/mcp-rs/config.toml
```

- Keep the credentials in the OS credential store (`keyring`). The Copilot CLI does not read `.vscode/mcp.json` itself; VS Code forwards the servers from it, except those that ask for a value with `${input:...}`
- Which environment variables reach mcp-rs depends on how the Copilot CLI is started. In VS Code on Windows, mcp-rs got almost all of VS Code's environment, including user environment variables, except the GitHub token (`GITHUB_PERSONAL_ACCESS_TOKEN`). [GitHub Docs](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers) says only `PATH` is passed. This is another reason to use `keyring`: it needs no environment variables and keeps the password out of files. If the config file reads `${...}` or uses `encrypted_credentials` (`WORDPRESS_MASTER_PASSWORD`), either rely on user environment variables (which worked in VS Code on Windows, but may not elsewhere), or pass them with `--env`, which always works but writes the values in plain text to `~/.copilot/mcp-config.json`. VS Code keeps the environment it started with: after changing user environment variables, quit every VS Code window and start it again
- Tested on Windows with `keyring`. macOS and Linux are not tested yet; on Linux, the Secret Service may not be reachable without `DBUS_SESSION_BUS_ADDRESS`
- Do not register mcp-rs with credentials in a workspace `.mcp.json` or `.github/mcp.json`. They live in the repository folder and can be committed (`.github/mcp.json` is meant to be)
- After registering, restart VS Code and start a new chat. Check with `copilot mcp list`, or ask the chat to call `mcp_rs_status`
- If mcp-rs does not start, the chat only says it cannot find the tool. The reason is in the Copilot CLI log, `~/.copilot/logs/process-*.log`: look for `MCP server connection failed`, which includes the error from mcp-rs. `mcp-rs check --config <path>` shows the same error in a terminal

## 4. Check {#check}

Before starting a client, `mcp-rs check --config <path>` (v0.25.0 and later) checks the config file the same way mcp-rs reads it when it starts:

```bash
mcp-rs check --config C:/Users/you/.config/mcp-rs/config.toml
```

It shows whether the file loads (the line and the reason of an error), whether the WordPress credentials can be read, whether mcp-rs can reach the site and sign in (read-only requests; when the sign-in fails, the permissions and media are not checked), and where the log and the audit log are written. It exits with 1 when it finds an error. Passwords, tokens and user names are not shown. Like starting mcp-rs, it creates the log and audit log folders and files if they are missing, and adds a `checked` line to the log file. It also checks `[database]` (v0.28.0 and later): the settings (`engine`, `ssl_mode`, `ssl_root_cert`, `${VAR}`), a warning for weak TLS settings, and whether mcp-rs can connect and sign in (nothing is written). For each of `[[plugins]]`, it checks that the command (or `docker`, `kubectl`) can be found, then starts the plugin, waits for its list of tools, and stops it. Connecting and starting each wait up to 10 seconds. `--offline` skips the connections to the site and the database, and does not start the plugins.

After the client has started mcp-rs, ask it to call the `mcp_rs_status` tool. It shows the version, the config file in use, where the log and the audit log are written, and anything that failed to start, with the reason. See the [API Reference]({{ site.baseurl }}/docs/api/).

## 5. Log files {#log-files}

mcp-rs writes its log to the first of these folders that it can write to:

1. `logs` in the folder of the `mcp-rs` executable (for example, `~/.mcp-rs/bin/logs`)
2. The user folder: `%USERPROFILE%\.mcp-rs\logs` on Windows, `$XDG_STATE_HOME/mcp-rs/logs` or `~/.local/state/mcp-rs/logs` elsewhere. When mcp-rs is installed in `C:\Program Files\MCP-rs`, the log is always here
3. `logs` in the current folder, then, on Windows only, `mcp-rs/logs` in the temporary folder (`%TEMP%` is per user there; the shared `/tmp` of other systems is not used, v0.24.0 and later)

On Linux and macOS, new log folders are created readable by you only (700) and new log files 600, like the audit log, and mcp-rs does not follow a symbolic link in place of a log file.

The `mcp_rs_status` tool shows the log file in use. It says `fallback` only when mcp-rs could not write to 1 or 2.

The file name ends with the date, such as `mcp-rs.log.2026-11-09`, and a new file starts at midnight local time. The date in the file name and the times inside the log are both in local time (older versions used the UTC date in the file name). When a running mcp-rs moves to the next day's file, the new file starts with a line such as `=== mcp-rs … continued at … / pid … / instance … / log: … ===`, so each file shows which process wrote it. When two processes are running (see below), the new file gets one such line from each, with different `instance` values.

Claude Desktop may start two mcp-rs processes for one entry in `claude_desktop_config.json`. This is how Claude Desktop works, not a problem with your mcp-rs settings. Both processes write to the same log file, so the log can look doubled:

- Each line has the process ID (`[pid …]`), so you can tell the two processes apart
- Each process writes its own startup line (`=== mcp-rs … started at … / pid … / instance … / log: … ===`). Windows reuses process IDs, so the same `pid` can belong to a different process after a restart. Read `[pid …]` together with the latest startup line above it. The startup line also has `instance` (the process ID and start time), which the audit log records with every tool call, so you can match the two logs
- Startup messages, such as the WordPress connection check, appear once per process. Seeing them twice is normal

The audit log (who called which tool) is written next to the logs, to `%USERPROFILE%\.mcp-rs\audit\audit.log` on Windows (`$XDG_STATE_HOME/mcp-rs/audit` or `~/.local/state/mcp-rs/audit` elsewhere). `[audit] file_path` changes it. Up to version 0.21.0, it was `audit.log` in the config file's folder; that file is left where it is. See [Security]({{ site.baseurl }}/docs/security/#audit-log).

## HTTP clients {#http}

Set `type = "http"` under `[transport]` and the address under `[transport.http]`. Turn on authentication before the listener is reachable from other machines, and see TLS, client certificates (mTLS), and WebSocket (`/ws`) in [Security]({{ site.baseurl }}/docs/security/#http-tls). For Docker, see the [Docker Guide](https://github.com/n-takatsu/mcp-rs/blob/main/docs/DOCKER_GUIDE.md).

## Guides

- [PostgreSQL Integration Guide](./postgres-integration.md) - Connect a database with a read-only role
