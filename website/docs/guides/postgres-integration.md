---
layout: page
title: PostgreSQL Integration Guide
permalink: /docs/guides/postgres-integration/
---

## PostgreSQL Integration Guide

Connect mcp-rs to a PostgreSQL database so that an MCP client (Claude Desktop, Claude Code, …) can read the schema and query the data. For every setting and tool, see the [Database Integration Guide](../database.md).

---

## 1. Check the build

The database tools are in the default build (`cargo build --release`) and in the Docker image. A build made with `--no-default-features` does not include them.

## 2. Create a role for mcp-rs

Connect with a role that can only read what the AI client should see. mcp-rs already runs every query in a read-only transaction, but a role without write permissions also covers what a transaction cannot, such as extensions (`dblink`) that open their own connections.

```sql
CREATE ROLE mcp_reader LOGIN PASSWORD 'choose-a-password';
GRANT CONNECT ON DATABASE app TO mcp_reader;
GRANT USAGE ON SCHEMA public TO mcp_reader;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO mcp_reader;
-- Tables created later by the role that owns the tables (run as that role,
-- or add FOR ROLE <owner>):
ALTER DEFAULT PRIVILEGES FOR ROLE app_owner IN SCHEMA public GRANT SELECT ON TABLES TO mcp_reader;
```

To hide some tables, grant `SELECT` on the tables you want to show instead of `ALL TABLES`.

`ALTER DEFAULT PRIVILEGES` applies only to tables that the given role (`app_owner` here) creates later. Without `FOR ROLE`, it applies to tables created by the role running the statement.

Extension functions can be executed by every role (`PUBLIC`) by default, so a read-only role can still use `dblink` if the extension is installed in the database. Do not install such extensions in the database mcp-rs connects to, or revoke their functions from `PUBLIC`. For example, move `dblink` to its own schema and revoke every function there (roles that need `dblink` then have to be granted it explicitly):

```sql
CREATE SCHEMA ext;
ALTER EXTENSION dblink SET SCHEMA ext;
REVOKE EXECUTE ON ALL FUNCTIONS IN SCHEMA ext FROM PUBLIC;
```

## 3. Add `[database]` to the configuration file

```toml
[database]
engine = "postgresql"
host = "localhost"
port = 5432
database = "app"
username = "mcp_reader"
password = "${MCP_DB_PASSWORD}"
ssl_mode = "prefer"   # use "verify-full" for a database on another machine
```

`${MCP_DB_PASSWORD}` is read from the environment when mcp-rs starts, so the password does not have to be in the file. With Claude Desktop, set it in the `env` of the mcp-rs entry:

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:\\Program Files\\MCP-rs\\mcp-rs.exe",
      "args": ["--config", "C:\\Users\\you\\mcp-rs\\mcp-config.toml"],
      "env": { "MCP_DB_PASSWORD": "choose-a-password" }
    }
  }
}
```

## 4. Restart and check the connection

Restart the MCP client, then ask it to call `mcp_rs_status`.

- Connected: the database appears under "Handlers", for example `postgresql mcp_reader@localhost:5432/app (read-only)`
- Not connected: it appears under "Handlers and plugins that failed to start" with the reason. mcp-rs still starts its other handlers

Common reasons:

| Reason | What to check |
|--------|---------------|
| `cannot connect to …` | Host, port, and that PostgreSQL accepts connections from this machine (`listen_addresses`, `pg_hba.conf`) |
| `password authentication failed` | The password, and that the environment variable is set for the MCP client, not only in your shell |
| `environment variable … is not set` | The `env` of the mcp-rs entry in the MCP client's settings |
| `database.engine = "…" is not supported` | `engine` must be `"postgresql"` |
| `this build of mcp-rs does not include database support` | The binary was built with `--no-default-features`; build it without that option |

## 5. Use the tools

Ask the client about your data. It will call:

- `get_schema` to see the tables and columns
- `execute_query` to run a `SELECT`, with values passed as `$1`, `$2`, … parameters

Results are limited to 1,000 rows by default (`max_rows`), and each statement to 30 seconds (`query_timeout_seconds`). For large tables, ask for `LIMIT` or aggregates.

## Allowing writes

To let the client run `INSERT`, `UPDATE`, and `DELETE`, set `allow_write = true` and connect with a role that has those permissions on the tables it may change. mcp-rs then offers `execute_command`, which runs one such statement at a time. DDL (`CREATE`, `DROP`, `ALTER`, `TRUNCATE`, …) is always refused.

```toml
[database]
engine = "postgresql"
database = "app"
username = "mcp_writer"
password = "${MCP_DB_PASSWORD}"
allow_write = true
```

---

See also: [Database Integration Guide](../database.md), [Security](../security.md)
