---
layout: page
title: API Reference
permalink: /docs/api/
---

## API Reference

Complete API documentation for MCP-RS v0.16.0+ - Multi-database support with MySQL and PostgreSQL.

---

## 📚 API Documentation Sections

### 1. Database Engine API

Comprehensive reference for database engine implementations:

- [📄 Database Engine API](./database.md)
  - Core types (Value, DatabaseError, QueryResult, ExecuteResult)
  - DatabaseEngine trait
  - DatabaseConnection trait
  - PreparedStatement trait
  - Transaction trait with ACID support
  - Isolation levels
  - Error handling

### 2. MCP Protocol API

Standard JSON-RPC 2.0 protocol methods:

- Initialization
- Tools (list, call)
- Resources (list, read)
- Prompts (list, get)
- Error codes

### 3. Integration Guides

Step-by-step guides for each database:

- [PostgreSQL Integration Guide](../guides/postgres-integration.md)
- MySQL Integration Guide (coming soon)
- [Performance Tuning Guide](../guides/performance-tuning.md)

---

## 🗄️ Supported Database Engines

| Database | Status | Features | Driver | Version |
|----------|--------|----------|--------|---------|
| **MySQL** | ✅ Phase 1 Complete | Transactions, Prepared Statements, JSON | sqlx 0.8 | 5.7+ |
| **PostgreSQL** | ✅ Phase 2 Complete | Transactions, Prepared Statements, JSON, UUID | sqlx 0.8 | 12+ |

---

## Core Data Types Quick Reference

### Value Enum

```rust
pub enum Value {
    Null,                                    // SQL NULL
    Bool(bool),                              // Boolean
    Int(i64),                                // 64-bit integer
    Float(f64),                              // 64-bit float
    String(String),                          // UTF-8 text
    Binary(Vec<u8>),                         // Raw binary
    Json(serde_json::Value),                 // JSON objects/arrays
    DateTime(chrono::DateTime<Utc>),         // UTC timestamp
}

```

### DatabaseError Enum

```rust
pub enum DatabaseError {
    ConnectionFailed(String),
    QueryFailed(String),
    TransactionFailed(String),
    SecurityViolation(String),
    ConfigurationError(String),
    ConfigValidationError(String),
    PoolError(String),
    TimeoutError(String),
    UnsupportedOperation(String),
    ValidationError(String),
}

```

### IsolationLevel Enum

```rust
pub enum IsolationLevel {
    Serializable,       // Highest isolation
    RepeatableRead,     // Prevents dirty/non-repeatable reads
    ReadCommitted,      // Prevents dirty reads
    ReadUncommitted,    // Lowest isolation, fastest
}

```

---

## MCP Protocol Methods

### Initialization

#### `initialize`

Establishes connection and negotiates capabilities.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "roots": { "listChanged": true },
      "sampling": {}
    },
    "clientInfo": {
      "name": "client-name",
      "version": "1.0.0"
    }
  },
  "id": 1
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "logging": {},
      "tools": { "listChanged": true },
      "resources": { "subscribe": true, "listChanged": true },
      "prompts": { "listChanged": true }
    },
    "serverInfo": {
      "name": "mcp-rs",
      "version": "0.16.0"
    }
  },
  "id": 1
}
```

---

### Tools

#### `tools/list`

Lists available tools.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "tools/list",
  "id": 2
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "tools": [
      {
        "name": "get_posts",
        "description": "Retrieve WordPress posts",
        "inputSchema": {
          "type": "object",
          "properties": {},
          "required": []
        }
      }
    ]
  },
  "id": 2
}
```

#### `tools/call`

Executes a tool.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "create_post",
    "arguments": {
      "title": "My New Post",
      "content": "This is the post content."
    }
  },
  "id": 3
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Created post with ID: 123"
      }
    ],
    "isError": false
  },
  "id": 3
}
```

---

## Resources (MCP)

### `resources/list`

Lists available resources.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "resources/list",
  "id": 4
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "resources": [
      {
        "uri": "wordpress://posts",
        "name": "WordPress Posts",
        "description": "All published WordPress posts",
        "mimeType": "application/json"
      },
      {
        "uri": "wordpress://pages",
        "name": "WordPress Pages",
        "description": "All published WordPress pages",
        "mimeType": "application/json"
      }
    ]
  },
  "id": 4
}
```

#### `resources/read`

Reads a resource.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "resources/read",
  "params": {
    "uri": "wordpress://posts"
  },
  "id": 5
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "contents": [
      {
        "uri": "wordpress://posts",
        "mimeType": "application/json",
        "text": "[{\"id\":1,\"title\":\"Sample Post\",\"content\":\"...\"}]"
      }
    ]
  },
  "id": 5
}
```

---

### Prompts

#### `prompts/list`

Lists available prompts.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "prompts/list",
  "id": 6
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "prompts": [
      {
        "name": "wordpress_content_analysis",
        "description": "Analyze WordPress content for SEO and readability",
        "arguments": [
          {
            "name": "post_id",
            "description": "WordPress post ID to analyze",
            "required": true
          }
        ]
      }
    ]
  },
  "id": 6
}
```

#### `prompts/get`

Retrieves a prompt.

**Request:**

```json
{
  "jsonrpc": "2.0",
  "method": "prompts/get",
  "params": {
    "name": "wordpress_content_analysis",
    "arguments": { "post_id": "123" }
  },
  "id": 7
}
```

**Response:**

```json
{
  "jsonrpc": "2.0",
  "result": {
    "description": "WordPress content analysis prompt",
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "Please analyze the following WordPress post..."
        }
      }
    ]
  },
  "id": 7
}
```

---

## Error Codes

| Code | Message | Description |
|------|---------|-------------|
| -32700 | Parse error | Invalid JSON |
| -32600 | Invalid Request | Invalid JSON-RPC request |
| -32601 | Method not found | Method doesn't exist |
| -32602 | Invalid params | Invalid method parameters |
| -32603 | Internal error | Internal JSON-RPC error |
| -32000 | Server error | Generic server error |
| -32001 | Invalid tool | Tool not found or invalid |
| -32002 | Tool execution failed | Tool execution error |
| -32003 | Resource not found | Resource URI not found |
| -32004 | Resource read failed | Resource read error |
| -32005 | Configuration error | Configuration validation error |

---

## WordPress Handler API

### Available Tools

#### `get_posts`

Retrieves WordPress posts.

#### `create_post`

Creates a new WordPress post.

- `title` (string, required)
- `content` (string, required)

#### `get_comments`

Retrieves WordPress comments.

- `post_id` (number, optional)

### Resources

- `wordpress://posts` - All published posts
- `wordpress://pages` - All published pages
- `wordpress://media` - Media library items
- `wordpress://categories` - Post categories
- `wordpress://tags` - Post tags
- `wordpress://users` - Site users

---

## Version Information

- **Current Version**: v0.16.0
- **Phase 1**: MySQL (Complete)
- **Phase 2**: PostgreSQL (Complete)
- **Last Updated**: 2025-11-23

---

## Quick Start

### Database Connection Example

```rust
use mcp_rs::handlers::database::engines::postgresql::PostgreSqlEngine;
use mcp_rs::handlers::database::types::{DatabaseConfig, DatabaseEngine};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DatabaseConfig {
        engine_type: "postgresql".to_string(),
        host: "localhost".to_string(),
        port: 5432,
        username: "postgres".to_string(),
        password: "password".to_string(),
        database: "myapp".to_string(),
        ..Default::default()
    };

    let engine = PostgreSqlEngine::new(config)?;
    let conn = engine.connect().await?;

    // Execute query
    let stmt = conn.prepare(
        "SELECT * FROM users WHERE age > $1"
    ).await?;

    let result = stmt.query(&[
        Value::Int(18)
    ]).await?;

    for row in result.rows {
        println!("{:?}", row);
    }

    conn.close().await?;
    Ok(())
}

```

---

For more details, see:

- [📄 Database Engine API](./database.md) - Complete API reference
- [📖 PostgreSQL Integration Guide](../guides/postgres-integration.md) - Setup and usage
- [📖 Implementation Guides](../guides/) - All guides
