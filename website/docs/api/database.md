---
layout: page
title: Database Engine API
permalink: /docs/api/database/
---

## Database Engine API

Comprehensive API reference for MySQL and PostgreSQL database engines in MCP-RS v0.16.0+.

---

## 🗄️ Supported Databases

| Database | Status | Features | Version |
|----------|--------|----------|---------|
| **MySQL** | ✅ Complete | Transactions, Prepared Statements, JSON | Phase 1 |
| **PostgreSQL** | ✅ Complete | Transactions, Prepared Statements, JSON, UUID | Phase 2 |

---

## Core Data Types

### `Value` Enum

Universal value representation across all database engines:

```rust
pub enum Value {
    Null,                                    // SQL NULL
    Bool(bool),                              // Boolean
    Int(i64),                                // 64-bit integer
    Float(f64),                              // 64-bit float
    String(String),                          // UTF-8 text
    Binary(Vec<u8>),                         // Raw binary data
    Json(serde_json::Value),                 // JSON objects/arrays
    DateTime(chrono::DateTime<Utc>),         // UTC timestamp
}
```

**Type Mapping Examples:**

| Rust Value | MySQL | PostgreSQL |
|------------|-------|------------|
| `Value::Null` | NULL | NULL |
| `Value::Bool(true)` | BOOLEAN | BOOLEAN |
| `Value::Int(42)` | BIGINT | BIGINT |
| `Value::Float(3.14)` | DOUBLE | DOUBLE PRECISION |
| `Value::String(...)` | VARCHAR | TEXT |
| `Value::Binary(...)` | BLOB | BYTEA |
| `Value::Json(...)` | JSON | JSONB |
| `Value::DateTime(...)` | DATETIME | TIMESTAMP |

### `DatabaseError` Enum

Comprehensive error handling:

```rust
pub enum DatabaseError {
    ConnectionFailed(String),      // Connection pool exhausted or unavailable
    QueryFailed(String),           // SQL syntax or execution error
    TransactionFailed(String),     // Transaction rollback or deadlock
    SecurityViolation(String),     // Access denied or policy violation
    ConfigurationError(String),    // Invalid configuration values
    ConfigValidationError(String), // Configuration validation failure
    PoolError(String),             // Connection pool error
    TimeoutError(String),          // Operation timeout
    UnsupportedOperation(String),  // Operation not supported on engine
    ValidationError(String),       // Parameter validation failure
}
```

### `QueryResult`

Result structure for SELECT queries:

```rust
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,      // Column metadata
    pub rows: Vec<Vec<Value>>,         // Result rows
    pub total_rows: Option<u64>,       // Total row count
    pub execution_time_ms: u64,        // Query execution time
}
```

### `ExecuteResult`

Result structure for INSERT/UPDATE/DELETE operations:

```rust
pub struct ExecuteResult {
    pub rows_affected: u64,     // Number of rows modified
    pub last_insert_id: Option<i64>,  // Last inserted ID (MySQL)
    pub execution_time_ms: u64, // Execution time
}
```

### `IsolationLevel`

Transaction isolation levels (same for MySQL & PostgreSQL):

```rust
pub enum IsolationLevel {
    Serializable,      // Level 3: Highest isolation, slowest
    RepeatableRead,    // Level 2: Prevents dirty and non-repeatable reads
    ReadCommitted,     // Level 1: Prevents dirty reads
    ReadUncommitted,   // Level 0: Lowest isolation, fastest
}
```

---

## DatabaseEngine Trait

Primary interface for database operations:

```rust
pub trait DatabaseEngine {
    fn engine_type(&self) -> &str;
    fn connect(&self) -> impl Future<Output = Result<Box<dyn DatabaseConnection>, DatabaseError>>;
    fn health_check(&self) -> impl Future<Output = Result<bool, DatabaseError>>;
    fn supported_features(&self) -> Vec<String>;
    fn validate_config(&self) -> Result<(), DatabaseError>;
    fn get_version(&self) -> impl Future<Output = Result<String, DatabaseError>>;
}
```

### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `engine_type()` | `&str` | "MySQL" or "PostgreSQL" |
| `connect()` | `Result<Connection>` | Establishes new connection |
| `health_check()` | `Result<bool>` | Validates pool health |
| `supported_features()` | `Vec<String>` | [Transactions, PreparedStatements, JsonSupport, Acid] |
| `validate_config()` | `Result<()>` | Validates configuration |
| `get_version()` | `Result<String>` | Database version string |

### Usage Example

```rust
let engine = MySqlEngine::new(config)?;

// Check engine type
println!("Engine: {}", engine.engine_type()); // "MySQL"

// Validate configuration
engine.validate_config()?;

// Get database version
let version = engine.get_version().await?;
println!("Version: {}", version);

// Check supported features
let features = engine.supported_features();
assert!(features.contains(&"Transactions".to_string()));
```

---

## DatabaseConnection Trait

Interface for individual database connections:

```rust
pub trait DatabaseConnection {
    async fn query(&self, sql: &str) -> Result<QueryResult, DatabaseError>;
    async fn execute(&self, sql: &str) -> Result<ExecuteResult, DatabaseError>;
    async fn prepare(&self, sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError>;
    async fn begin_transaction(&self) -> Result<Box<dyn Transaction>, DatabaseError>;
    async fn close(&self) -> Result<(), DatabaseError>;
}
```

### Methods

| Method | Params | Returns | Description |
|--------|--------|---------|-------------|
| `query()` | SQL string | `QueryResult` | Execute SELECT query |
| `execute()` | SQL string | `ExecuteResult` | Execute INSERT/UPDATE/DELETE |
| `prepare()` | SQL template | `PreparedStatement` | Create prepared statement |
| `begin_transaction()` | - | `Transaction` | Start new transaction |
| `close()` | - | `Result<()>` | Close connection |

### Usage Example

```rust
let engine = MySqlEngine::new(config)?;
let conn = engine.connect().await?;

// Direct query (not recommended for user input)
let result = conn.query("SELECT * FROM users").await?;
for row in result.rows {
    println!("Row: {:?}", row);
}

// Prepared statement (recommended - prevents SQL injection)
let stmt = conn.prepare("SELECT * FROM users WHERE id = ?").await?;
let result = stmt.query(&[Value::Int(123)]).await?;

conn.close().await?;
```

---

## PreparedStatement Trait

**Purpose**: Parameterized query execution with SQL injection prevention

```rust
pub trait PreparedStatement {
    async fn query(&self, params: &[Value]) -> Result<QueryResult, DatabaseError>;
    async fn execute(&self, params: &[Value]) -> Result<ExecuteResult, DatabaseError>;
    fn parameter_count(&self) -> usize;
    fn get_sql(&self) -> &str;
    async fn close(&self) -> Result<(), DatabaseError>;
}
```

### Parameter Placeholder Syntax

#### MySQL

Uses `?` placeholders:

```sql
-- Placeholder syntax
SELECT * FROM users WHERE id = ? AND status = ?

-- Rust usage
let stmt = conn.prepare("SELECT * FROM users WHERE id = ? AND status = ?").await?;
let result = stmt.query(&[
    Value::Int(123),
    Value::String("active".to_string())
]).await?;
```

#### PostgreSQL

Uses `$1, $2...` placeholders:

```sql
-- Placeholder syntax
SELECT * FROM users WHERE id = $1 AND status = $2

-- Rust usage
let stmt = conn.prepare("SELECT * FROM users WHERE id = $1 AND status = $2").await?;
let result = stmt.query(&[
    Value::Int(123),
    Value::String("active".to_string())
]).await?;
```

### Security

✅ **SQL Injection Prevention**: Parameterized queries separate SQL structure from data

❌ **Vulnerable (String concatenation):**
```rust
// DO NOT DO THIS
let user_input = "1' OR '1'='1";
let sql = format!("SELECT * FROM users WHERE id = '{}'", user_input);
conn.query(sql).await?; // Unsafe!
```

✅ **Secure (Parameterized query):**
```rust
// DO THIS
let stmt = conn.prepare("SELECT * FROM users WHERE id = ?").await?;
let result = stmt.query(&[Value::String(user_input)]).await?; // Safe!
```

### Usage Example

```rust
// Prepare statement
let stmt = conn.prepare(
    "INSERT INTO users (name, email, age) VALUES (?, ?, ?)"
).await?;

// Bind parameters
let result = stmt.execute(&[
    Value::String("John Doe".to_string()),
    Value::String("john@example.com".to_string()),
    Value::Int(30),
]).await?;

println!("Inserted {} rows", result.rows_affected);
println!("Last ID: {:?}", result.last_insert_id);
```

---

## Transaction Trait

ACID transaction support:

```rust
pub trait Transaction {
    async fn commit(&self) -> Result<(), DatabaseError>;
    async fn rollback(&self) -> Result<(), DatabaseError>;
    async fn set_isolation_level(&self, level: IsolationLevel) -> Result<(), DatabaseError>;
    async fn savepoint(&self, name: &str) -> Result<(), DatabaseError>;
    async fn rollback_to_savepoint(&self, name: &str) -> Result<(), DatabaseError>;
    async fn release_savepoint(&self, name: &str) -> Result<(), DatabaseError>;
}
```

### ACID Properties

| Property | Guarantee | Description |
|----------|-----------|-------------|
| **Atomicity** | All-or-nothing | Transaction succeeds completely or not at all |
| **Consistency** | Data validity | All database constraints remain satisfied |
| **Isolation** | Independent | Transactions don't interfere (level-dependent) |
| **Durability** | Persistent | Committed data survives failures |

### Isolation Levels Comparison

| Level | Dirty Read | Non-Repeatable | Phantom Read | Speed | Use Case |
|-------|-----------|---------------|-----------|----|----------|
| **Serializable** | ❌ No | ❌ No | ❌ No | Slowest | Critical data |
| **RepeatableRead** | ❌ No | ❌ No | ⚠️ Maybe | Medium | Most transactions |
| **ReadCommitted** | ❌ No | ✅ Yes | ✅ Yes | Fast | Read-heavy apps |
| **ReadUncommitted** | ✅ Yes | ✅ Yes | ✅ Yes | Fastest | Analytics, reports |

### Transaction Example

```rust
// Begin transaction
let txn = conn.begin_transaction().await?;

// Set isolation level
txn.set_isolation_level(IsolationLevel::ReadCommitted).await?;

// Create savepoint
txn.savepoint("sp1").await?;

try {
    // Execute operations
    let stmt = txn.prepare("INSERT INTO accounts (balance) VALUES (?)").await?;
    stmt.execute(&[Value::Int(1000)]).await?;
    
    // More operations...
    
    // Commit on success
    txn.commit().await?;
} catch {
    // Rollback on error
    txn.rollback().await?;
}
```

### Nested Transactions (Savepoints)

```rust
let txn = conn.begin_transaction().await?;

// Main operations
txn.execute("INSERT INTO log VALUES (...)").await?;

// Create savepoint
txn.savepoint("before_delete").await?;

// Try operation
match txn.execute("DELETE FROM users WHERE id = ?").await {
    Ok(_) => {
        txn.commit().await?;
    }
    Err(_) => {
        // Rollback only to savepoint, keep previous operations
        txn.rollback_to_savepoint("before_delete").await?;
        txn.commit().await?;
    }
}
```

---

## PostgreSQL Specific Features

### JSON/JSONB Support

PostgreSQL native JSON types with advanced operations:

```rust
use serde_json::json;

// Store JSON data
let json_data = Value::Json(json!({
    "name": "John Doe",
    "age": 30,
    "email": "john@example.com",
    "tags": ["developer", "rust"]
}));

let stmt = conn.prepare("INSERT INTO users (data) VALUES ($1)").await?;
stmt.execute(&[json_data]).await?;

// Query JSON - retrieve as JSON
let result = stmt.query(&[]).await?;
for row in result.rows {
    if let Value::Json(json) = &row[0] {
        println!("Name: {}", json["name"]);
    }
}
```

### UUID Type Support

```rust
// UUID as string (PostgreSQL converts automatically)
let uuid = Value::String("550e8400-e29b-41d4-a716-446655440000".to_string());

let stmt = conn.prepare("INSERT INTO sessions (id, user_id) VALUES ($1, $2)").await?;
stmt.execute(&[uuid, Value::Int(123)]).await?;
```

### Connection Pool Configuration

```toml
[database]
engine = "postgresql"

[database.postgres]
# Connection parameters
host = "localhost"
port = 5432
username = "postgres"
password = "password"
database = "myapp"

# Pool settings
max_connections = 20
connection_timeout_secs = 30
idle_timeout_secs = 300
max_lifetime_secs = 1800

# SSL/TLS (optional)
ssl_enabled = true
ssl_verify = true
```

---

## Error Handling

### Connection Errors

```rust
// Example: Connection refused
match engine.connect().await {
    Ok(conn) => { /* use connection */ },
    Err(DatabaseError::ConnectionFailed(msg)) => {
        eprintln!("Cannot connect: {}", msg);
    }
}
```

### Query Errors

```rust
match stmt.query(&params).await {
    Ok(result) => { /* process result */ },
    Err(DatabaseError::QueryFailed(msg)) => {
        eprintln!("Query failed: {}", msg);
        // Example: "syntax error at or near \"SELCT\""
    }
}
```

### Transaction Errors

```rust
match txn.commit().await {
    Ok(_) => println!("Committed successfully"),
    Err(DatabaseError::TransactionFailed(msg)) => {
        eprintln!("Transaction failed: {}", msg);
        // Example: "Deadlock found when trying to get lock"
    }
}
```

---

## Version Information

- **Current Version**: v0.16.0
- **Phase 1**: MySQL (Complete)
- **Phase 2**: PostgreSQL (Complete)
- **Updated**: 2025-11-23

---

See also: [PostgreSQL Integration Guide](../../guides/postgres-integration.md), [Performance Tuning](../../guides/performance-tuning.md)
