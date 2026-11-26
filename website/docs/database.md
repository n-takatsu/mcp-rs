---
layout: page
title: Database Integration Guide
permalink: /docs/database/
---

# Database Integration Guide

**[Home]({{ site.baseurl }}/) > [Documentation]({{ site.baseurl }}/docs/) > Database Guide**

mcp-rs can connect to one PostgreSQL, MySQL, MariaDB, or SQLite database and give MCP clients tools to read its schema and run SQL. By default it only reads: queries run in a read-only transaction, and the tool that changes data is not offered.

- **Engines**: PostgreSQL, MySQL and MariaDB from v0.25.0, and SQLite from v0.31.0 (see [MySQL](#mysql), [MariaDB](#mariadb), and [SQLite](#sqlite)). Other engines (Redis, MongoDB) exist as library code but cannot be configured yet. They are planned for v0.31.0
- **Build**: all four are included in the default build, the [ready-built files](https://github.com/n-takatsu/mcp-rs/releases), and the Docker image. A build made with `--no-default-features` leaves them out, and reports that in `mcp_rs_status`
- **Step-by-step setup**: [PostgreSQL Integration Guide]({{ site.baseurl }}/docs/guides/postgres-integration/)

## Configuration

[`mcp-rs init`](../guides/#mcp-rs-init) can add the `[database]` section for you, with the password in the OS credential store (v0.29.0 and later). From v0.30.0, it can also set up a database without WordPress (answer 2, "A database", to its first question): the config file then has `[database]` and no `[handlers.wordpress]`. To write it by hand, add a `[database]` section to the configuration file:

```toml
[database]
engine = "postgresql"
host = "localhost"                # default: localhost
port = 5432                       # default: 5432
database = "app"
username = "mcp_reader"
password = "${MCP_DB_PASSWORD}"   # read from an environment variable
ssl_mode = "prefer"               # disable, allow, prefer, require, verify-full

# Optional
# ssl_root_cert = "certs/ca.pem"  # CA that signed the server certificate (verify-full)
# allow_write = false             # true offers execute_command (INSERT, UPDATE, DELETE)
# max_rows = 1000                 # rows returned by execute_query at most
# max_result_bytes = 10485760     # size of the execute_query result at most (about 10 MB)
# query_timeout_seconds = 30      # time limit of one SQL statement
# max_connections = 5             # connections in the pool at most
# enabled = true                  # false keeps the section but does not connect
```

| Key | Required | Default | Notes |
|-----|----------|---------|-------|
| `engine` | yes | | `"postgresql"`, `"mysql"`, `"mariadb"`, or `"sqlite"` (v0.31.0 and later). For SQLite, see [SQLite](#sqlite): only `database` and the keys from `allow_write` on are used |
| `host` | | `localhost` | `${VAR}` allowed |
| `port` | | `5432` (MySQL, MariaDB: `3306`) | |
| `database` | yes | | `${VAR}` allowed. SQLite: the path of the database file |
| `username` | yes (not for SQLite) | | `${VAR}` allowed |
| `password` | | none | `${VAR}` allowed. Never written to logs, `mcp_rs_status`, or error messages |
| `keyring` | | none | Name of the entry in the OS credential store that holds the password, instead of `password` (v0.29.0 and later). `mcp-rs init` or `mcp-rs database-keyring set <name>` stores it (see [Keeping the password in the OS credential store](#keyring)). The entry is tied to `host`, `port` and `username`: with other values in `[database]`, mcp-rs does not use it, so a config file someone else wrote cannot send the password to another server. Cannot be combined with `password` |
| `ssl_mode` | when `host` is not local | `prefer` | Same meaning as libpq's `sslmode`. See [TLS](#tls). MySQL and MariaDB have no `allow`. From v0.30.0, it can be left out only when `host` is `localhost`, `127.0.0.1`, or `::1` (PostgreSQL: also a Unix socket path, or when `PGSSLMODE` is set to a valid mode) |
| `ssl_root_cert` | | none | PEM file of the CA that signed the server certificate. A relative path is relative to the folder of the configuration file. Used only with `ssl_mode = "verify-full"` (MySQL and MariaDB: also `"verify-ca"`, which requires it): with a weaker `ssl_mode`, the database tools do not start |
| `allow_write` | | `false` | See [Writing data](#writing-data) |
| `max_rows` | | `1000` | `0` means the default |
| `max_result_bytes` | | `10485760` (10 MB) | About how many bytes of values `execute_query` returns (see below). `0` means the default |
| `query_timeout_seconds` | | `30` | `0` means the default |
| `max_connections` | | `5` | `0` means the default |
| `enabled` | | `true` | |

### Keeping the password in the OS credential store {#keyring}

`mcp-rs database-keyring` keeps the password of `[database]` in the OS credential store (Windows Credential Manager, macOS Keychain, Secret Service on Linux), without running `mcp-rs init` (v0.29.0 and later). Run it in a terminal:

```bash
mcp-rs database-keyring set app@db --config ~/.config/mcp-rs/config.toml
```

- When the config file has `[database]`, its host, port and user are the defaults. After storing the password, it offers to change `[database]` to `keyring = "app@db"` and to remove `password`. Comments and the rest of the file are kept, and a backup copy is made
- Without `[database]`, it asks the same questions as `mcp-rs init` and offers to add `[database]`
- Without `--config`, it uses `~/.config/mcp-rs/config.toml`
- `mcp-rs database-keyring show app@db` shows the host, port and user the entry is for (not the password). `delete` removes the entry. Neither touches WordPress credentials (use `mcp-rs wordpress-keyring` for those)

The password is never taken as an argument and is not shown while typing. Use it again after changing the database password: mcp-rs reads the new one the next time it starts.

### TLS

From v0.30.0, `ssl_mode` must be set when `host` is not `localhost`, `127.0.0.1`, or `::1` (for PostgreSQL, also not a Unix socket path). Without it, mcp-rs starts without the database tools and shows the reason in `mcp_rs_status`. For PostgreSQL, `PGSSLMODE` counts as setting it; an invalid `PGSSLMODE` also stops the database tools, since it would otherwise be ignored. `mcp-rs init` already writes `ssl_mode = "verify-full"` for such hosts; older configuration files that leave it out need one line, `ssl_mode = "verify-full"` (or the weaker mode you mean to use, for example `"require"`).

Without `ssl_mode`, `prefer` is used (on local hosts). `prefer` uses TLS when the server offers it, but does not check the server certificate, and silently connects without TLS when the server does not offer it. Anyone on the network path can then read the queries and results, or pose as the server and receive the password.

When the database is on another machine, use `ssl_mode = "verify-full"`. It checks that the certificate is signed by a trusted CA and matches `host`. For a private CA (for example Amazon RDS), download its certificate and point `ssl_root_cert` to it. The CA file is used only with `verify-full` (and `verify-ca` on MySQL and MariaDB), so mcp-rs starts without the database tools when `ssl_root_cert` is set with a weaker `ssl_mode` (including `PGSSLMODE` when `ssl_mode` is not set), and shows the reason in `mcp_rs_status`. mcp-rs logs a warning at startup when the host is not `localhost`, `127.0.0.1`, `::1`, or a Unix socket, and `ssl_mode` is weaker than `verify-full`.

For PostgreSQL, `verify-ca` is not supported (from v0.27.0): mcp-rs starts without the database tools and shows the reason, also when `PGSSLMODE=verify-ca`. `verify-ca` does not check the host name, and the TLS library mcp-rs uses for PostgreSQL trusts the public CAs even with `ssl_root_cert`, so any certificate from a public CA, for any host, would be accepted. Use `verify-full`. With `ssl_root_cert`, `verify-full` trusts that CA in addition to the public CAs, and checks that the certificate is for `host`.

If you used `verify-ca` because `host` is not the name in the certificate (you connect by an IP address, or through an SSH tunnel to `localhost`), do not change to `require`: it does not check the certificate at all, which is weaker than `verify-ca` was. Connect by the name in the certificate instead. When that name does not resolve to the address you use, map it in the hosts file (`/etc/hosts`, or `C:\Windows\System32\drivers\etc\hosts`), for example `127.0.0.1 db.example.com` for a tunnel, and set `host = "db.example.com"` with the tunnel's `port`.

Like libpq, mcp-rs also reads these when the configuration file does not set the value:

| Source | Used when |
|--------|-----------|
| `PGSSLMODE` | `ssl_mode` is not set |
| `PGSSLROOTCERT` | `ssl_root_cert` is not set |
| `PGSSLCERT`, `PGSSLKEY` | always (client certificate) |
| `PGPASSWORD`, then `~/.pgpass` (`%APPDATA%\postgresql\pgpass.conf` on Windows) | `password` is not set or empty |
| `PGOPTIONS` | always (server settings sent when connecting) |

The warning at startup checks the `ssl_mode` in effect, including `PGSSLMODE`. To be sure of the settings, set `ssl_mode` in the configuration file, and do not set these variables in the environment of mcp-rs.

### When the database cannot be used

mcp-rs connects once at startup. If the settings are invalid, an environment variable in `${VAR}` is not set, or the database cannot be reached, mcp-rs logs a warning and starts without the database tools. WordPress and plugins still start. The reason is shown by the `mcp_rs_status` tool under "Handlers and plugins that failed to start", and a working connection is shown under "Handlers", for example `postgresql mcp_reader@localhost:5432/app (read-only)`.

## Tools

| Tool | Offered | Annotations |
|------|---------|-------------|
| `execute_query` | always | `readOnlyHint` |
| `get_schema` | always | `readOnlyHint` |
| `execute_command` | only with `allow_write = true` | `destructiveHint` |

The read-only tools can be allowed alone in HTTP mode with `@read_only` (see [Security]({{ site.baseurl }}/docs/security/)).

### `execute_query`

Runs one `SELECT` statement (`WITH … SELECT` is allowed) and returns the rows.

```json
{
  "sql": "SELECT id, name, created_at FROM customers WHERE country = $1 ORDER BY id LIMIT 50",
  "params": ["JP"]
}
```

The result is JSON text:

```json
{
  "columns": [
    {"name": "id", "type": "INT4", "nullable": true, "max_length": null},
    {"name": "name", "type": "TEXT", "nullable": true, "max_length": null},
    {"name": "created_at", "type": "TIMESTAMPTZ", "nullable": true, "max_length": null}
  ],
  "rows": [[1, "Example K.K.", "2026-11-11T03:34:56+00:00"]],
  "truncated": false,
  "max_rows": 1000,
  "max_result_bytes": 10485760,
  "total_rows": 1,
  "execution_time_ms": 3
}
```

- At most `max_rows` rows, and about `max_result_bytes` bytes of values, are returned. `truncated` is `true` when there were more. mcp-rs reads the rows through a cursor, a few rows at first and twice as many each time (up to 500), and closes it at the limit. PostgreSQL produces at most about as many rows past the limit as were read before it. Still, use `LIMIT`, `WHERE`, or aggregates for large tables
- `max_result_bytes` is a guide, not an exact size of the response: mcp-rs counts a string as its length plus 2, but quotes and control characters take 2 to 6 bytes each in JSON, and the result is wrapped in a JSON string once more. A response can be several times larger than `max_result_bytes` in the worst case
- When the first row alone is larger than `max_result_bytes`, its large values are cut: a string keeps its beginning and ends with `… [cut: N bytes in total; …]`, and `bytea` and `json` values are replaced by a note with their size. Select a part of such a value, for example `substring(column from 1 for 1000)`. mcp-rs still receives that one row whole, so a row can use as much memory as PostgreSQL allows for one row (up to about 1 GB per value)
- A result without rows still has its `columns`
- Values are returned by type: integers, floats, and booleans as JSON values (`NaN`, `Infinity`, and `-Infinity`, which JSON numbers cannot hold, as those strings; up to v0.23.0, as `0`); `numeric` as a string (no digits are lost); `uuid`, `date`, `time`, and `timestamp` as strings; `timestamptz` as RFC 3339; `json`/`jsonb` as JSON; `bytea` as base64; `text[]`, `varchar[]`, `int4[]`, and `int8[]` as JSON arrays. Other types are replaced by a note: cast such columns to text, for example `column::text`

### `get_schema`

Lists the tables of schema `public` with their columns, types, primary keys, foreign keys, and indexes. The same information is available as the resource `db://schema`.

- A foreign key on several columns is listed once per column, with the same `name`. A table referenced outside `public` is shown as `schema.table`
- The `columns` of an index are its key columns in order. The expression part of an expression index (such as `lower(name)`) and the columns of `INCLUDE` are left out
- Up to v0.23.0, `foreign_keys` and `indexes` were always empty

### `execute_command`

Offered only with `allow_write = true`. Runs one `INSERT`, `UPDATE`, or `DELETE` statement, commits it, and returns `rows_affected`. Other statements (`CREATE`, `DROP`, `ALTER`, `TRUNCATE`, `GRANT`, several statements, …) are refused.

```json
{
  "sql": "UPDATE orders SET status = $1 WHERE id = $2",
  "params": ["shipped", 1042]
}
```

## Prompt

`summarize_table` (v0.30.0 and later) is a ready-made request, picked in the client's prompt menu, that asks the AI to describe a table: its columns, number of rows and how values are distributed. It reads with small `SELECT` queries only, and does not quote personal data.

## Parameters

Write `$1`, `$2`, … in the SQL and pass the values in `params`. Values are bound as parameters and never inserted into the SQL text.

- `params` takes strings, numbers, booleans, and `null`
- Strings are converted to the type PostgreSQL expects for the parameter: `uuid`, `date` (`YYYY-MM-DD`), `time`, `timestamp`, `timestamptz` (RFC 3339), `numeric`, integers, floats, `boolean`, `json`/`jsonb`, and `bytea` (base64). So values of these types returned by `execute_query` can be passed back as they are, except `json`/`jsonb`: `execute_query` returns them as JSON values (objects, arrays, …), and `params` takes only strings, numbers, booleans and `null`, so pass them as a JSON string (for example `"{\"a\":1}"`). Other types (arrays, `interval`, `inet`, …) need a cast: see below
- Strings are also sent as they are to text columns and enum columns
- For other types (`interval`, `money`, `inet`, arrays, …), pass a string and cast it through text in the SQL: `$1::text::interval`. Without the cast, mcp-rs refuses the parameter instead of sending it in the wrong format
- A value that cannot be converted is refused with an error that names the parameter, for example `parameter $1 is not a valid uuid`. The error does not repeat the value

## Safety

What mcp-rs does:

- `execute_query` accepts only a single `SELECT`. Writes inside `WITH` (`WITH d AS (DELETE … RETURNING *) SELECT …`) and `SELECT … INTO` are refused. Row locks (`FOR UPDATE`, `FOR SHARE`) are refused too, because a read-only tool should not make other writes wait (v0.27.0 and later)
- Both tools refuse functions that reach files or connections on the database server, which a read-only transaction does not stop: `LOAD_FILE()` and the `sys_exec()`/`sys_eval()` functions on MySQL and MariaDB; `pg_read_file()`, `pg_read_binary_file()`, `pg_ls_dir()`, `pg_stat_file()`, `lo_import()`, `lo_export()`, the `pg_ls_*` and `dblink*` functions, and `pg_terminate_backend()`/`pg_cancel_backend()` (which stop other connections of the same role) on PostgreSQL (v0.27.0 and later). On PostgreSQL, names written with Unicode escapes (`U&"…"`) are refused, so these functions cannot be written another way. On MySQL, the optimizer hint `SET_VAR(…)` is refused: it changes session settings, such as `foreign_key_checks`, for the statement. This is a second line of defense: do not give the role the privileges these need (MySQL's `FILE`, PostgreSQL's `pg_read_server_files` and `pg_write_server_files`)
- Every query also runs in a read-only transaction (`SET TRANSACTION READ ONLY`) that is rolled back, so a `SELECT` that calls a function that writes is refused by PostgreSQL
- Each statement has a time limit (`query_timeout_seconds`, via `statement_timeout`). `get_schema` also runs in a read-only transaction with this limit. The limit applies to each statement, not to the whole tool call: `execute_query` reads rows with several `FETCH` statements, and `get_schema` runs a few statements per table, so one call can take longer in total
- The result has a size limit (`max_rows`, `max_result_bytes`), so one query cannot fill the memory of mcp-rs
- When a connection returns to the pool, mcp-rs resets what a statement may have changed for the session: session-level advisory locks (`pg_advisory_lock`) are released, and settings and the role changed with `set_config(…, false)` inside `execute_command` are reset (`SET SESSION AUTHORIZATION DEFAULT`, `RESET ROLE`, `RESET ALL`)
- Each tool call is recorded in the [audit log]({{ site.baseurl }}/docs/security/) with its SQL (shortened to 2,000 characters) and whether it succeeded. Parameter values are not recorded. Error messages are recorded with everything in quotes replaced by `…`, because PostgreSQL errors can contain a value (`invalid input syntax for type integer: "…"`). The MCP client receives the full message

What you should do:

- **Connect with a read-only role.** The read-only transaction cannot stop everything: extensions such as `dblink` open their own connections, which can write. A role that only has `SELECT` closes that gap, as long as such extensions are not installed or their functions are revoked from `PUBLIC` (they are executable by every role by default). See [Create a read-only role]({{ site.baseurl }}/docs/guides/postgres-integration/#2-create-a-role-for-mcp-rs)
- Grant the role only the tables the AI client needs to see
- Use `ssl_mode = "verify-full"` when the database is on another machine (see [TLS](#tls))
- Use `allow_write = true` only with a role whose write permissions you are willing to give the AI client, and preferably not on a production database

## Writing data

With `allow_write = true`, mcp-rs offers `execute_command` (its annotations mark it as destructive, so MCP clients usually ask before calling it). `execute_query` stays read-only either way. The status line shows `(read-write)` instead of `(read-only)`.

`execute_command` takes one `INSERT`, `UPDATE`, or `DELETE`, including the forms that read other tables (`INSERT … SELECT`, an `UPDATE` or `DELETE` that joins several tables, and MySQL's `ON DUPLICATE KEY UPDATE`). They can only do what the database role may do, so limit the role's write permissions to the tables the AI client should change.

## MySQL

From v0.25.0, `engine = "mysql"` connects to MySQL (8.0 or later). The tools, the limits, and the audit log are the same as for PostgreSQL. For MariaDB, see [MariaDB](#mariadb).

```toml
[database]
engine = "mysql"
host = "db.example.com"
# port = 3306                     # default for MySQL
database = "app"
username = "mcp_reader"
password = "${MCP_DB_PASSWORD}"
ssl_mode = "verify-full"
```

What differs from PostgreSQL:

- **Parameters**: write `?` in the SQL, in the order of `params` (not `$1`). MySQL converts strings to the column type, so dates (`YYYY-MM-DD HH:MM:SS`), `DECIMAL`, and `JSON` can be passed as the strings `execute_query` returns. To pass a binary value, pass it as base64 and use `FROM_BASE64(?)`
- **Values**: integers and floats as JSON numbers (`BIGINT UNSIGNED` values too large for a 64-bit signed integer as strings); `DECIMAL` as a string; `DATE`, `DATETIME`, `TIMESTAMP`, and `TIME` as strings (`2026-11-16`, `2026-11-16 09:05:07`, `-26:03:04`); `JSON` as JSON; binary types (`BINARY`, `VARBINARY`, `BLOB`, `BIT`) as base64. Column types are shown in lower case (`int`, `varchar`, `int unsigned`, …)
- **Read-only**: each query runs in `START TRANSACTION READ ONLY`, which is rolled back, so a `SELECT` that calls a function that writes is refused by MySQL. `SELECT … INTO` (variables or files) is refused by mcp-rs
- **Time limit**: `SELECT` statements are stopped by MySQL (`MAX_EXECUTION_TIME`). Other statements (`execute_command`, or a statement waiting for a lock) are stopped by mcp-rs: after `query_timeout_seconds` plus 2 seconds, it runs `KILL QUERY` on a separate connection and closes the connection. A user can stop its own statements this way without extra privileges. Before `KILL QUERY`, mcp-rs checks in `information_schema.PROCESSLIST` that the connection with that id belongs to the same user, and that either the separate connection reached the same server (MySQL: `server_uuid`; MariaDB: host name, port and `server_id`) and the server has not restarted since (compared with its `Uptime`), or the connection is running the statement mcp-rs sent (v0.30.0 and later). When both servers can be read and differ, or the server has restarted, it does not send `KILL QUERY` even for the same statement; the statement is compared only when the server or its restart cannot be told. Otherwise it does not send `KILL QUERY` and only closes its connection. The error says whether the statement was stopped with `KILL QUERY`, only its connection was closed, or it may still be running. Getting a connection from the pool also has a time limit: `timeout_seconds` (10 seconds by default)
- **Size limit**: MySQL sends the whole result without a cursor. At the limit, mcp-rs stops reading, runs `KILL QUERY` so MySQL stops sending the rest, and closes the connection; the next call opens a new connection
- **Through a proxy** (ProxySQL, Amazon RDS Proxy, …): `KILL QUERY` needs the MySQL thread id of the connection, which a proxy may hide. A proxy or load balancer may also send the separate connection to another server, where the same id is someone else's session: the check above makes stopping that statement less likely, but does not rule it out. When the server cannot be told (it hides `server_uuid` or `Uptime`), the statement is compared only up to its first `?` (at most 64 characters), so a statement of the same user that starts the same way is stopped too, and the session can change between the check and `KILL QUERY`. Then mcp-rs still returns (it waits at most about 7 more seconds: 5 for `KILL QUERY` and 2 to close, then drops the connection), but MySQL may keep running the statement until it finishes, and mcp-rs logs a warning. Connect directly to MySQL when you rely on the time limit for writes
- **Sessions**: when a connection returns to the pool, it is reset (`COM_RESET_CONNECTION`): user variables, locks taken with `GET_LOCK()`, temporary tables, and session settings do not carry over
- **`sql_mode`**: on its connections, mcp-rs removes `NO_BACKSLASH_ESCAPES`, `ANSI_QUOTES`, and the combination modes that include `ANSI_QUOTES` (`ANSI`; on MariaDB also `DB2`, `MSSQL`, `ORACLE`, `POSTGRESQL`, `MAXDB`) from the session's `sql_mode`, keeping the other modes. mcp-rs checks the SQL before running it, and these modes make the server split strings differently from that check (`\'` would end a string, and `"…"` would be a name), so SQL that looks harmless to the check could run something else. With mcp-rs, `\'` escapes a quote and `"…"` is a string: quote names with backticks (`` `name` ``)
- **`get_schema`**: lists the tables and views of the configured database (from `information_schema`) with their columns, primary keys, foreign keys, and indexes. Views are listed under `views`, with their `definition` (`VIEW_DEFINITION` of `information_schema.VIEWS`, v0.30.0 and later). The `definition` is an empty string when the user may not read it (without the `SHOW VIEW` privilege, for example)
- **`execute_command`**: also returns `last_insert_id` (the `AUTO_INCREMENT` id of an inserted row)
- **TLS**: `ssl_mode` takes `disable`, `prefer` (default on local hosts), `require`, `verify-ca`, and `verify-full` (MySQL's `VERIFY_IDENTITY`), with the same meaning as for PostgreSQL, and must be set when `host` is not local (from v0.30.0). `prefer` connects without TLS only when the server does not offer it. With `ssl_root_cert`, only that CA is trusted; without it, `verify-full` trusts the public CAs (Mozilla's list). `verify-ca` requires `ssl_root_cert` (from v0.27.0): it does not check the host name, so it accepts a certificate for any host from that CA. Use `verify-ca` only when `ssl_root_cert` is a CA that issues certificates only for your own servers (your own CA, or a cloud provider's database CA such as Amazon RDS). When the server's certificate is from a public CA (Let's Encrypt, DigiCert, …), use `verify-full`

Create a user that can only read what the AI client should see. Do not grant `FILE` (it lets `LOAD_FILE()` read files on the database server) or `PROCESS`:

```sql
CREATE USER 'mcp_reader'@'%' IDENTIFIED BY '...';
GRANT SELECT ON app.* TO 'mcp_reader'@'%';
```

## MariaDB

From v0.25.0, `engine = "mariadb"` connects to MariaDB (10.6 or later; tested with 11.4). MariaDB uses the same protocol as MySQL, so everything in [MySQL](#mysql) applies, with these differences:

```toml
[database]
engine = "mariadb"
host = "db.example.com"
# port = 3306                     # default for MariaDB
database = "app"
username = "mcp_reader"
password = "${MCP_DB_PASSWORD}"
ssl_mode = "verify-full"
```

- **Time limit**: `SELECT` statements are stopped by MariaDB (`max_statement_time`, as MariaDB has no `MAX_EXECUTION_TIME`). Other statements are stopped with `KILL QUERY`, as for MySQL
- **Executable comments**: MariaDB runs the SQL inside `/*M! … */` comments, which mcp-rs cannot check (for example, `SELECT 1 /*M! INTO OUTFILE '/tmp/x' */`). mcp-rs refuses SQL with such a comment, with `engine = "mysql"` too
- **Values**: MariaDB's `JSON` type is text (`LONGTEXT`), so `JSON` values are returned as strings, not as JSON
- **Which server**: mcp-rs reads the server version when it connects, and uses MariaDB's time limit when the server is MariaDB. A configuration that says `engine = "mysql"` but points at MariaDB also works
- **TLS**: MariaDB 11.4 and later offer TLS with a certificate they generate themselves. Such a certificate is not signed by a CA, so `verify-ca` and `verify-full` need a certificate from your own CA (`ssl_root_cert`)

## SQLite

From v0.31.0, `engine = "sqlite"` reads a SQLite database file. There is no server: `database` is the path of the file, and the settings for connecting to a server are not used.

```toml
[database]
engine = "sqlite"
database = "data/app.db"          # relative to the config file's folder
# allow_write = false             # true offers execute_command
```

- **The file**: a relative path is relative to the folder of the configuration file (a path given with `${VAR}` is used as the variable says). mcp-rs does not create the file: when it does not exist, mcp-rs starts without the database tools and `mcp_rs_status` (and `mcp-rs check`) says the file was not found. `:memory:` and `file:` URIs are refused
- **Settings that are not used**: `host`, `port`, `username`, `password`, `keyring`, `ssl_mode`, and `ssl_root_cert`. When one of them is set, mcp-rs starts without the database tools and shows the reason in `mcp_rs_status` (and `mcp-rs check`), so a setting meant for another database is not ignored silently. The OS credential store is not read
- **Read-only**: the file is opened read-only, so SQLite itself refuses any write, and each query runs in a transaction that is rolled back. With `allow_write = true`, mcp-rs also opens the file for writing, for `execute_command` only: `execute_query` still reads through the read-only connection. `ATTACH`, `PRAGMA`, `VACUUM` and the other statements that are not `SELECT`, `INSERT`, `UPDATE`, or `DELETE` are refused, so a query cannot open or write another file. The functions `load_extension()` and `fts3_tokenizer()` are refused too (SQLite does not offer them by default)
- **Connections**: with `allow_write = true`, the read-only connections and the connections for writing are two pools, each with up to `max_connections` connections
- **Other programs**: other programs can keep using the file. A database in WAL mode can be read while another program writes to it. When another program holds a lock, mcp-rs waits up to `query_timeout_seconds`
- **Time limit**: SQLite has no server to stop a statement, so mcp-rs checks the time while SQLite runs it, and stops it after `query_timeout_seconds` (the error says it was stopped). The wait for another program's lock is capped at about 24 days, the longest SQLite accepts
- **Parameters**: write `?` in the SQL, in the order of `params`. Numbers are bound as `INTEGER` or `REAL`, booleans as 0 or 1, and strings as `TEXT`. To pass a binary value, pass it as hex text and use `unhex(?)`
- **Values**: SQLite stores a type with each value, so values are returned by that type, not by the column's declared type: integers and floats as JSON numbers, text as strings, `BLOB` as base64, and `NULL` as `null`. Dates are whatever the application stored (usually text). The `type` of a column is its declared type (empty for an expression)
- **`get_schema`**: lists the tables and views of the file (not SQLite's own `sqlite_` tables) with their columns (generated columns too), primary keys, foreign keys (a foreign key that does not name the column refers to the primary key of the other table), and indexes. Views are listed under `views` with the SQL that created them. Indexes on expressions are left out
- **`execute_command`**: also returns `last_insert_id` (the `rowid` of an inserted row)
- **Trusted files only**: the SQLite that mcp-rs bundles has a known flaw ([RUSTSEC-2026-0459](https://rustsec.org/advisories/RUSTSEC-2026-0459.html)) that a specially made database file (with its `-wal` file) can use. It is fixed in a newer SQLite, which mcp-rs cannot use yet ([#449](https://github.com/n-takatsu/mcp-rs/issues/449)). Open only files that you or a program you trust wrote, not files received from others

## Library API

The engines can also be used from Rust code. See the [Database Engine API]({{ site.baseurl }}/docs/api/database/).
