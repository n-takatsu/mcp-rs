---
layout: page
title: Database Integration Guide
permalink: /docs/database/
---

# Database Integration Guide

**[Home]({{ site.baseurl }}/) > [Documentation]({{ site.baseurl }}/docs/) > Database Guide**

mcp-rs can connect to one PostgreSQL database and give MCP clients tools to read its schema and run SQL. By default it only reads: queries run in a read-only transaction, and the tool that changes data is not offered.

- **Engine**: PostgreSQL only. Other engines (MySQL, SQLite, Redis, MongoDB) exist as library code but cannot be configured yet
- **Build**: included in the default build and in the Docker image. A build made with `--no-default-features` leaves it out, and reports that in `mcp_rs_status`
- **Step-by-step setup**: [PostgreSQL Integration Guide](./guides/postgres-integration.md)

## Configuration

Add a `[database]` section to the configuration file:

```toml
[database]
engine = "postgresql"
host = "localhost"                # default: localhost
port = 5432                       # default: 5432
database = "app"
username = "mcp_reader"
password = "${MCP_DB_PASSWORD}"   # read from an environment variable
ssl_mode = "prefer"               # disable, allow, prefer (default), require, verify-ca, verify-full

# Optional
# ssl_root_cert = "certs/ca.pem"  # CA that signed the server certificate (verify-ca, verify-full)
# allow_write = false             # true offers execute_command (INSERT, UPDATE, DELETE)
# max_rows = 1000                 # rows returned by execute_query at most
# max_result_bytes = 10485760     # size of the execute_query result at most (about 10 MB)
# query_timeout_seconds = 30      # time limit of one SQL statement
# max_connections = 5             # connections in the pool at most
# enabled = true                  # false keeps the section but does not connect
```

| Key | Required | Default | Notes |
|-----|----------|---------|-------|
| `engine` | yes | | Only `"postgresql"` |
| `host` | | `localhost` | `${VAR}` allowed |
| `port` | | `5432` | |
| `database` | yes | | `${VAR}` allowed |
| `username` | yes | | `${VAR}` allowed |
| `password` | | none | `${VAR}` allowed. Never written to logs, `mcp_rs_status`, or error messages |
| `ssl_mode` | | `prefer` | Same meaning as libpq's `sslmode`. See [TLS](#tls) |
| `ssl_root_cert` | | none | PEM file of the CA that signed the server certificate. A relative path is relative to the folder of the configuration file. Used only with `ssl_mode = "verify-ca"` or `"verify-full"`: with a weaker `ssl_mode`, the database tools do not start |
| `allow_write` | | `false` | See [Writing data](#writing-data) |
| `max_rows` | | `1000` | `0` means the default |
| `max_result_bytes` | | `10485760` (10 MB) | About how many bytes of values `execute_query` returns (see below). `0` means the default |
| `query_timeout_seconds` | | `30` | `0` means the default |
| `max_connections` | | `5` | `0` means the default |
| `enabled` | | `true` | |

### TLS

The default `ssl_mode = "prefer"` uses TLS when the server offers it, but does not check the server certificate, and silently connects without TLS when the server does not offer it. Anyone on the network path can then read the queries and results, or pose as the server and receive the password.

When the database is on another machine, use `ssl_mode = "verify-full"`. It checks that the certificate is signed by a trusted CA and matches `host`. For a private CA (for example Amazon RDS), download its certificate and point `ssl_root_cert` to it. The CA file is used only with `verify-ca` or `verify-full`, so mcp-rs starts without the database tools when `ssl_root_cert` is set with a weaker `ssl_mode` (including `PGSSLMODE` when `ssl_mode` is not set), and shows the reason in `mcp_rs_status`. mcp-rs logs a warning at startup when the host is not `localhost`, `127.0.0.1`, `::1`, or a Unix socket, and `ssl_mode` is weaker than `verify-full`.

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

The read-only tools can be allowed alone in HTTP mode with `@read_only` (see [Security](./security.md)).

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
- Values are returned by type: integers, floats, and booleans as JSON values; `numeric` as a string (no digits are lost); `uuid`, `date`, `time`, and `timestamp` as strings; `timestamptz` as RFC 3339; `json`/`jsonb` as JSON; `bytea` as base64; `text[]`, `varchar[]`, `int4[]`, and `int8[]` as JSON arrays. Other types are replaced by a note: cast such columns to text, for example `column::text`

### `get_schema`

Lists the tables of schema `public` with their columns, types, primary keys, foreign keys, and indexes. The same information is available as the resource `db://schema`.

### `execute_command`

Offered only with `allow_write = true`. Runs one `INSERT`, `UPDATE`, or `DELETE` statement, commits it, and returns `rows_affected`. Other statements (`CREATE`, `DROP`, `ALTER`, `TRUNCATE`, `GRANT`, several statements, …) are refused.

```json
{
  "sql": "UPDATE orders SET status = $1 WHERE id = $2",
  "params": ["shipped", 1042]
}
```

## Parameters

Write `$1`, `$2`, … in the SQL and pass the values in `params`. Values are bound as parameters and never inserted into the SQL text.

- `params` takes strings, numbers, booleans, and `null`
- Strings are converted to the type PostgreSQL expects for the parameter: `uuid`, `date` (`YYYY-MM-DD`), `time`, `timestamp`, `timestamptz` (RFC 3339), `numeric`, integers, floats, `boolean`, `json`/`jsonb`, and `bytea` (base64). So values returned by `execute_query` can be passed back as they are
- Strings are also sent as they are to text columns and enum columns
- For other types (`interval`, `money`, `inet`, arrays, …), pass a string and cast it through text in the SQL: `$1::text::interval`. Without the cast, mcp-rs refuses the parameter instead of sending it in the wrong format
- A value that cannot be converted is refused with an error that names the parameter, for example `parameter $1 is not a valid uuid`. The error does not repeat the value

## Safety

What mcp-rs does:

- `execute_query` accepts only a single `SELECT`. Writes inside `WITH` (`WITH d AS (DELETE … RETURNING *) SELECT …`) and `SELECT … INTO` are refused
- Every query also runs in a read-only transaction (`SET TRANSACTION READ ONLY`) that is rolled back, so a `SELECT` that calls a function that writes is refused by PostgreSQL
- Each statement has a time limit (`query_timeout_seconds`, via `statement_timeout`). `get_schema` also runs in a read-only transaction with this limit. The limit applies to each statement, not to the whole tool call: `execute_query` reads rows with several `FETCH` statements, and `get_schema` runs a few statements per table, so one call can take longer in total
- The result has a size limit (`max_rows`, `max_result_bytes`), so one query cannot fill the memory of mcp-rs
- When a connection returns to the pool, mcp-rs resets what a statement may have changed for the session: session-level advisory locks (`pg_advisory_lock`) are released, and settings and the role changed with `set_config(…, false)` inside `execute_command` are reset (`SET SESSION AUTHORIZATION DEFAULT`, `RESET ROLE`, `RESET ALL`)
- Each tool call is recorded in the [audit log](./security.md) with its SQL (shortened to 2,000 characters) and whether it succeeded. Parameter values are not recorded. Error messages are recorded with everything in quotes replaced by `…`, because PostgreSQL errors can contain a value (`invalid input syntax for type integer: "…"`). The MCP client receives the full message

What you should do:

- **Connect with a read-only role.** The read-only transaction cannot stop everything: extensions such as `dblink` open their own connections, which can write. A role that only has `SELECT` closes that gap, as long as such extensions are not installed or their functions are revoked from `PUBLIC` (they are executable by every role by default). See [Create a read-only role](./guides/postgres-integration.md#2-create-a-role-for-mcp-rs)
- Grant the role only the tables the AI client needs to see
- Use `ssl_mode = "verify-full"` when the database is on another machine (see [TLS](#tls))
- Use `allow_write = true` only with a role whose write permissions you are willing to give the AI client, and preferably not on a production database

## Writing data

With `allow_write = true`, mcp-rs offers `execute_command` (its annotations mark it as destructive, so MCP clients usually ask before calling it). `execute_query` stays read-only either way. The status line shows `(read-write)` instead of `(read-only)`.

## Library API

The engines can also be used from Rust code. See the [Database Engine API](./api/database.md).
