---
layout: page
title: PostgreSQL Integration Guide
permalink: /docs/guides/postgres-integration/
---

# PostgreSQL Integration Guide

Complete setup and integration guide for PostgreSQL support in MCP-RS v0.16.0+.

---

## 📋 Table of Contents

1. [Overview](#overview)
2. [Installation](#installation)
3. [Configuration](#configuration)
4. [Connection Management](#connection-management)
5. [Query Execution](#query-execution)
6. [Transactions](#transactions)
7. [JSON Support](#json-support)
8. [Performance Tuning](#performance-tuning)
9. [Troubleshooting](#troubleshooting)

---

## Overview

### Supported PostgreSQL Versions

- PostgreSQL 12+
- PostgreSQL 13, 14, 15 (tested)
- PostgreSQL 16 (compatible)

### Key Features

- ✅ Full ACID compliance with 4 isolation levels
- ✅ Parameterized queries (SQL injection prevention)
- ✅ Connection pooling with health checks
- ✅ JSON/JSONB native type support
- ✅ UUID type support
- ✅ Savepoint-based nested transactions
- ✅ SSL/TLS connection support

### Architecture

```
┌─────────────┐
│ Application │
│   (Rust)    │
└──────┬──────┘
       │
       ▼
┌─────────────────────────────────┐
│   DatabaseEngine Trait          │
│  (Unified Interface)            │
└──────┬──────────────────────────┘
       │
       ▼
┌─────────────────────────────────┐
│  PostgreSQL Engine              │
│  ├─ connection.rs (Pool Mgmt)   │
│  ├─ prepared.rs (Queries)       │
│  ├─ transaction.rs (ACID)       │
│  └─ json_support.rs (JSON/JSONB)│
└──────┬──────────────────────────┘
       │
       ▼
┌─────────────────────────────────┐
│   sqlx 0.8 (PostgreSQL Driver)  │
└──────┬──────────────────────────┘
       │
       ▼
┌─────────────────────────────────┐
│  PostgreSQL Database            │
│  (Native Protocol)              │
└─────────────────────────────────┘
```

---

## Installation

### Step 1: Add Dependencies

Update `Cargo.toml`:

```toml
[dependencies]
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio-native-tls", "uuid", "json", "chrono"] }
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
```

### Step 2: Setup Docker (Optional)

For development, use Docker Compose:

```bash
docker-compose -f docker-compose.postgres.yml up -d
```

Includes:
- PostgreSQL 15 Alpine
- pgAdmin for visual management
- Health checks

### Step 3: Create Database

```bash
psql -U postgres -h localhost -c "CREATE DATABASE mcp_rs;"
psql -U postgres -h localhost -d mcp_rs < schema.sql
```

---

## Configuration

### Configuration File (`config.toml`)

```toml
[database]
engine = "postgresql"

[database.postgres]
# Connection parameters
host = "localhost"
port = 5432
username = "postgres"
password = "your_secure_password"
database = "mcp_rs"

# Connection pool
max_connections = 20
connection_timeout_secs = 30
idle_timeout_secs = 300
max_lifetime_secs = 1800

# SSL/TLS (optional)
ssl_enabled = false
ssl_verify = true
ssl_ca_cert = "/path/to/ca-cert.pem"

# Query settings
default_timeout_secs = 30
statement_cache_size = 100
```

### Environment Variables

```bash
# Database connection
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_USER=postgres
POSTGRES_PASSWORD=secure_password
POSTGRES_DB=mcp_rs

# Pool configuration
POSTGRES_MAX_CONNECTIONS=20
POSTGRES_CONNECTION_TIMEOUT=30
POSTGRES_IDLE_TIMEOUT=300
```

### Rust Configuration

```rust
use mcp_rs::handlers::database::types::DatabaseConfig;

let config = DatabaseConfig {
    engine_type: "postgresql".to_string(),
    host: "localhost".to_string(),
    port: 5432,
    username: "postgres".to_string(),
    password: "password".to_string(),
    database: "mcp_rs".to_string(),
    max_connections: 20,
    connection_timeout_secs: 30,
    ..Default::default()
};

let engine = PostgreSqlEngine::new(config)?;
```

---

## Connection Management

### Basic Connection

```rust
use mcp_rs::handlers::database::engines::postgresql::PostgreSqlEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DatabaseConfig { /* ... */ };
    let engine = PostgreSqlEngine::new(config)?;
    
    // Validate configuration
    engine.validate_config()?;
    
    // Get database version
    let version = engine.get_version().await?;
    println!("PostgreSQL {}", version);
    
    // Connect to database
    let conn = engine.connect().await?;
    
    // Health check
    if engine.health_check().await? {
        println!("✓ Database healthy");
    }
    
    conn.close().await?;
    Ok(())
}
```

### Connection Pool Usage

```rust
// Connection pool is created automatically
let engine = PostgreSqlEngine::new(config)?;

// Pool statistics
let pool = engine.get_pool();
println!("Idle connections: {}", pool.num_idle());
println!("Total connections: {}", pool.size());
```

### Error Handling

```rust
match engine.connect().await {
    Ok(conn) => {
        // Use connection
        conn.close().await?;
    }
    Err(DatabaseError::ConnectionFailed(msg)) => {
        eprintln!("Connection error: {}", msg);
    }
    Err(e) => {
        eprintln!("Error: {:?}", e);
    }
}
```

---

## Query Execution

### Safe: Parameterized Queries (Recommended)

```rust
let stmt = conn.prepare(
    "SELECT id, name, email FROM users WHERE age > $1 AND status = $2"
).await?;

let result = stmt.query(&[
    Value::Int(18),
    Value::String("active".to_string()),
]).await?;

for row in result.rows {
    println!("ID: {}, Name: {}", row[0], row[1]);
}
```

### Direct Query (Use with Caution)

```rust
// Only for trusted SQL without user input
let result = conn.query("SELECT COUNT(*) FROM users").await?;
println!("Total users: {}", result.rows[0][0]);
```

### INSERT/UPDATE/DELETE

```rust
// Insert
let stmt = conn.prepare(
    "INSERT INTO users (name, email, age) VALUES ($1, $2, $3)"
).await?;

let result = stmt.execute(&[
    Value::String("John Doe".to_string()),
    Value::String("john@example.com".to_string()),
    Value::Int(30),
]).await?;

println!("Inserted {} rows", result.rows_affected);
```

---

## Transactions

### Basic Transaction

```rust
let txn = conn.begin_transaction().await?;

match execute_operations(&txn).await {
    Ok(_) => {
        txn.commit().await?;
        println!("✓ Transaction committed");
    }
    Err(e) => {
        txn.rollback().await?;
        eprintln!("✗ Transaction rolled back: {}", e);
    }
}

async fn execute_operations(txn: &dyn Transaction) -> Result<(), Box<dyn Error>> {
    let stmt = txn.prepare("INSERT INTO logs VALUES (...)").await?;
    stmt.execute(&[]).await?;
    Ok(())
}
```

### Transaction with Isolation Level

```rust
use mcp_rs::handlers::database::types::IsolationLevel;

let txn = conn.begin_transaction().await?;

// Set isolation level
txn.set_isolation_level(IsolationLevel::RepeatableRead).await?;

// Execute operations
// ...

txn.commit().await?;
```

### Savepoint (Nested Transactions)

```rust
let txn = conn.begin_transaction().await?;

// Main operation
txn.execute("INSERT INTO accounts (balance) VALUES (1000)").await?;

// Create savepoint
txn.savepoint("before_transfer").await?;

// Attempt transfer
match txn.execute("UPDATE accounts SET balance = balance - 100").await {
    Ok(_) => {
        // Success - commit
        txn.commit().await?;
    }
    Err(_) => {
        // Rollback to savepoint only
        txn.rollback_to_savepoint("before_transfer").await?;
        
        // Keep main operation, commit
        txn.commit().await?;
    }
}
```

---

## JSON Support

### Storing JSON

```rust
use serde_json::json;

let json_data = Value::Json(json!({
    "name": "John Doe",
    "preferences": {
        "theme": "dark",
        "notifications": true
    },
    "tags": ["developer", "rust"]
}));

let stmt = conn.prepare(
    "INSERT INTO user_profiles (data) VALUES ($1)"
).await?;

stmt.execute(&[json_data]).await?;
```

### Querying JSON

```rust
// Get entire JSON document
let result = conn.query(
    "SELECT data FROM user_profiles WHERE id = 1"
).await?;

if let Value::Json(json) = &result.rows[0][0] {
    println!("Name: {}", json["name"]);
    println!("Theme: {}", json["preferences"]["theme"]);
}

// JSON contains query
let result = conn.query(
    "SELECT * FROM user_profiles WHERE data @> '{\"preferences\":{\"theme\":\"dark\"}}'"
).await?;

println!("Found {} dark theme users", result.rows.len());
```

### JSON Functions

```rust
// Extract text value
let result = conn.query(
    "SELECT data->>'name' FROM user_profiles WHERE id = 1"
).await?;

// Get array length
let result = conn.query(
    "SELECT jsonb_array_length(data->'tags') FROM user_profiles"
).await?;

// Set JSON value
let result = conn.query(
    "UPDATE user_profiles SET data = jsonb_set(data, '{preferences,theme}', '\"light\"')"
).await?;
```

---

## Performance Tuning

### Connection Pool Optimization

```toml
[database.postgres]
# For high-concurrency scenarios
max_connections = 50          # Match expected concurrent clients
connection_timeout_secs = 5   # Fail fast on exhaustion
idle_timeout_secs = 600       # Clean up after 10 minutes
max_lifetime_secs = 3600      # Recycle after 1 hour
```

### Query Optimization

```rust
// Use prepared statements for repeated queries
let stmt = conn.prepare("SELECT * FROM users WHERE id = $1").await?;

// Reuse statement multiple times
for user_id in 1..1000 {
    let result = stmt.query(&[Value::Int(user_id)]).await?;
    // Process result
}

// Indexes help query performance
// conn.query("CREATE INDEX idx_users_id ON users(id)").await?;
```

### Batch Operations

```rust
// Batch insert (more efficient than individual inserts)
let mut values = vec![];
for i in 1..1000 {
    values.push(format!("({}, 'user{}@example.com')", i, i));
}

let sql = format!(
    "INSERT INTO users (id, email) VALUES {}",
    values.join(",")
);

conn.execute(&sql).await?;
```

---

## Troubleshooting

### Connection Issues

**Problem**: `connection refused`

**Solution**:
```bash
# Check PostgreSQL is running
sudo systemctl status postgresql

# Check port is listening
sudo netstat -tlnp | grep 5432

# Test connection manually
psql -U postgres -h localhost -d mcp_rs
```

### Query Issues

**Problem**: `syntax error at or near`

**Solution**:
```rust
// Use parameterized queries
let stmt = conn.prepare("SELECT * FROM users WHERE age > $1").await?;
stmt.query(&[Value::Int(18)]).await?;

// Not this (SQL injection risk + syntax errors)
// let sql = format!("SELECT * FROM users WHERE age > {}", user_input);
```

### Performance Issues

**Problem**: Slow queries

**Solution**:
```rust
// Enable query logging
// Add execution_time_ms tracking
let result = stmt.query(&params).await?;
println!("Query took {}ms", result.execution_time_ms);

// Add indexes
conn.query("CREATE INDEX idx_col ON table(column)").await?;

// Analyze query plan
conn.query("EXPLAIN ANALYZE SELECT ...").await?;
```

### Pool Exhaustion

**Problem**: `pool limit reached`

**Solution**:
```toml
[database.postgres]
max_connections = 50  # Increase pool size
idle_timeout_secs = 300  # Reduce timeout for unused connections
```

---

## Security Best Practices

### 1. Use Parameterized Queries

```rust
// ✅ SAFE
let stmt = conn.prepare("SELECT * FROM users WHERE email = $1").await?;
stmt.query(&[Value::String(user_email)]).await?;

// ❌ UNSAFE
let sql = format!("SELECT * FROM users WHERE email = '{}'", user_email);
conn.query(sql).await?;
```

### 2. Secure Password Storage

```rust
// Use environment variables or secure vaults
let password = std::env::var("POSTGRES_PASSWORD")
    .expect("Set POSTGRES_PASSWORD env var");

let config = DatabaseConfig {
    password,
    ..Default::default()
};
```

### 3. SSL/TLS Connections

```toml
[database.postgres]
ssl_enabled = true
ssl_verify = true
ssl_ca_cert = "/path/to/ca-cert.pem"
```

### 4. Connection Limits

```toml
[database.postgres]
max_connections = 20  # Limit concurrent connections
connection_timeout_secs = 30
```

---

## Version Information

- **Guide Version**: 1.0
- **MCP-RS**: v0.16.0+
- **PostgreSQL**: 12+
- **Updated**: 2025-11-23

---

See also: [Database Engine API](../api/database.md), [Performance Tuning Guide](./performance-tuning.md)
