# Database Integration Guide 🗄️ Multi-Engine Database System

## Overview

mcp-rs provides a comprehensive multi-database engine system with enterprise-grade security integration. This guide covers the complete database functionality for developers, integrators, and end users.

## 🏗️ Architecture Overview

## Multi-Engine Support

mcp-rs supports 5 major database engines with unified API access:

- **PostgreSQL** - Enterprise relational database
- **MySQL** - Popular web-scale database  
- **Redis** - High-performance in-memory store
- **MongoDB** - Document-oriented NoSQL database
- **SQLite** - Lightweight embedded database

## Security Integration

All database operations are protected by the 6-layer security architecture:
- ✅ **SQL Injection Protection** (11 attack patterns)
- ✅ **Multi-Factor Authentication** with TOTP
- ✅ **Role-Based Access Control** (RBAC)
- ✅ **Real-time Anomaly Detection**
- ✅ **Column-Level Encryption**
- ✅ **Comprehensive Audit Logging**

## 🚀 Quick Start

## 1. Database Configuration

Configure multiple database engines in `mcp-config.toml`:

```toml


## Primary PostgreSQL database

[[database.engines]]
id = "primary_pg"
type = "postgresql"
host = "localhost"
port = 5432
database = "myapp"
username = "appuser"
password = "secure_password"
ssl_mode = "require"

## Redis cache

[[database.engines]]
id = "cache_redis"
type = "redis"
host = "localhost"
port = 6379
database = 0
password = "redis_password"

## MongoDB documents

[[database.engines]]
id = "docs_mongo"
type = "mongodb"
uri = "mongodb://localhost:27017"
database = "documents"

```

## 2. Basic Database Operations

### Execute Query (PostgreSQL)

```json

  "tool": "execute_query",
  "arguments": {
    "sql": "SELECT * FROM users WHERE active = $1",
    "params": [true],
    "engine": "primary_pg"
  }
}

```

### Redis Operations

```json

  "tool": "execute_query", 
  "arguments": {
    "sql": "GET user:12345",
    "engine": "cache_redis"
  }
}

```

### MongoDB Document Operations

```json

  "tool": "execute_query",
  "arguments": {
    "sql": "{\"operation\": \"find\", \"collection\": \"users\", \"filter\": {\"status\": \"active\"}}",
    "engine": "docs_mongo"
  }
}

```

## 📡 Database API Reference

## Core Database Tools

### `execute_query` - Execute SELECT Queries

**Description**: Execute SELECT queries with automatic security validation

**Parameters**:
- `sql` (string, required): SQL query or database-specific command
- `params` (array, optional): Query parameters for SQL injection prevention
- `engine` (string, optional): Target database engine ID

**Security Features**:
- SQL injection protection (11 patterns)
- Input sanitization and validation
- Query timeout enforcement
- Audit logging

**Examples**:

```json

{
  "tool": "execute_query",
  "arguments": {
    "sql": "SELECT id, name, email FROM users WHERE created_at > $1 LIMIT $2",
    "params": ["2024-01-01", 50],
    "engine": "primary_pg"
  }
}

// Redis key-value retrieval
{
  "tool": "execute_query",
  "arguments": {
    "sql": "HGETALL user:profile:12345",
    "engine": "cache_redis"
  }
}

// MongoDB aggregation
{
  "tool": "execute_query", 
  "arguments": {
    "sql": "{\"operation\": \"aggregate\", \"collection\": \"orders\", \"pipeline\": [{\"$match\": {\"status\": \"completed\"}}, {\"$group\": {\"_id\": \"$customer_id\", \"total\": {\"$sum\": \"$amount\"}}}]}",
    "engine": "docs_mongo"
  }
}

```

### `execute_command` - Execute Data Modification

**Description**: Execute INSERT, UPDATE, DELETE commands with transaction support

**Parameters**:
- `sql` (string, required): Command to execute
- `params` (array, optional): Command parameters
- `engine` (string, optional): Target database engine
- `transaction` (boolean, optional): Execute within transaction

**Security Features**:
- Command validation and sanitization
- Transaction integrity enforcement
- Change audit logging
- Row-level security checks

**Examples**:

```json

{
  "tool": "execute_command",
  "arguments": {
    "sql": "INSERT INTO users (name, email, created_at) VALUES ($1, $2, NOW())",
    "params": ["John Doe", "john@example.com"],
    "engine": "primary_pg",
    "transaction": true
  }
}

// Redis cache update
{
  "tool": "execute_command",
  "arguments": {
    "sql": "SETEX session:abc123 3600 \"user_data_json\"",
    "engine": "cache_redis"
  }
}

// MongoDB document update
{
  "tool": "execute_command",
  "arguments": {
    "sql": "{\"operation\": \"updateOne\", \"collection\": \"users\", \"filter\": {\"_id\": \"507f1f77bcf86cd799439011\"}, \"update\": {\"$set\": {\"last_login\": \"2024-11-07T10:30:00Z\"}}}",
    "engine": "docs_mongo"
  }
}

```

### `begin_transaction` - Start Database Transaction

**Description**: Begin a database transaction with configurable isolation level

**Parameters**:
- `engine` (string, optional): Target database engine
- `isolation_level` (string, optional): Transaction isolation level
  - `READ_COMMITTED` (default)
  - `READ_UNCOMMITTED`
  - `REPEATABLE_READ`
  - `SERIALIZABLE`

**Examples**:

```json

  "tool": "begin_transaction",
  "arguments": {
    "engine": "primary_pg",
    "isolation_level": "REPEATABLE_READ"
  }
}

```

### `get_schema` - Retrieve Database Schema

**Description**: Get database schema information including tables, indexes, and relationships

**Parameters**:
- `engine` (string, optional): Target database engine
- `schema_name` (string, optional): Specific schema name

**Examples**:

```json

  "tool": "get_schema",
  "arguments": {
    "engine": "primary_pg",
    "schema_name": "public"
  }
}

```

## Engine Management Tools

### `list_engines` - List Available Database Engines

**Description**: List all configured database engines and their status

**Examples**:

```json

  "tool": "list_engines",
  "arguments": {}
}

```

**Response**:

```json

  "engines": [
    {
      "id": "primary_pg",
      "type": "postgresql", 
      "status": "healthy",
      "version": "15.0",
      "connections": 5,
      "last_check": "2024-11-07T10:30:00Z"
    },
    {
      "id": "cache_redis",
      "type": "redis",
      "status": "healthy", 
      "version": "7.2.3",
      "connections": 2,
      "last_check": "2024-11-07T10:30:00Z"
    }
  ]
}

```

### `switch_engine` - Switch Active Database Engine

**Description**: Change the default database engine for subsequent operations

**Parameters**:
- `engine_id` (string, required): ID of engine to switch to

**Examples**:

```json

  "tool": "switch_engine",
  "arguments": {
    "engine_id": "cache_redis"
  }
}

```

## 🔐 Security Configuration

## Database-Specific Security Settings

```toml


## Enable comprehensive security features

enable_sql_injection_detection = true
enable_query_whitelist = false
enable_audit_logging = true
threat_intelligence_enabled = true
max_query_length = 10000

## Multi-Factor Authentication

[database.security.mfa]
enable_totp = true
backup_codes_count = 10
device_trust_duration = "30d"

## Role-Based Access Control

[database.security.rbac]
enable_hierarchical_roles = true
enable_resource_policies = true
default_role = "database_user"

## Column-Level Encryption

[database.security.encryption]
enable_column_encryption = true
master_key_rotation_days = 90

```

## Security Integration Examples

### Query with MFA Verification

```json

  "tool": "execute_query",
  "arguments": {
    "sql": "SELECT * FROM sensitive_data WHERE user_id = $1",
    "params": [12345],
    "engine": "primary_pg",
    "security": {
      "require_mfa": true,
      "audit_level": "high"
    }
  }
}

```

### Role-Based Query Execution

```json

  "tool": "execute_command",
  "arguments": {
    "sql": "UPDATE user_roles SET role = $1 WHERE user_id = $2", 
    "params": ["admin", 12345],
    "engine": "primary_pg",
    "security": {
      "required_role": "user_manager",
      "audit_category": "role_change"
    }
  }
}

```

## 🚀 Advanced Usage

## Multi-Engine Workflows

### Cache-Aside Pattern with PostgreSQL + Redis

```json

{
  "tool": "execute_query",
  "arguments": {
    "sql": "GET user:profile:12345",
    "engine": "cache_redis"
  }
}

// 2. If cache miss, query database
{
  "tool": "execute_query", 
  "arguments": {
    "sql": "SELECT * FROM users WHERE id = $1",
    "params": [12345],
    "engine": "primary_pg"
  }
}

// 3. Update cache with result
{
  "tool": "execute_command",
  "arguments": {
    "sql": "SETEX user:profile:12345 3600 \"{user_data_json}\"",
    "engine": "cache_redis"
  }
}

```

### Document + Relational Hybrid

```json

{
  "tool": "execute_command",
  "arguments": {
    "sql": "INSERT INTO orders (id, customer_id, total) VALUES ($1, $2, $3)",
    "params": ["ord_123", 12345, 99.99],
    "engine": "primary_pg"
  }
}

// Store flexible metadata in MongoDB
{
  "tool": "execute_command",
  "arguments": {
    "sql": "{\"operation\": \"insertOne\", \"collection\": \"order_metadata\", \"document\": {\"order_id\": \"ord_123\", \"preferences\": {\"gift_wrap\": true, \"delivery_notes\": \"Ring doorbell\"}, \"analytics\": {\"campaign_id\": \"summer2024\"}}}",
    "engine": "docs_mongo"
  }
}

```

## Performance Optimization

### Connection Pooling Configuration

```toml

max_connections = 50
min_connections = 10
connection_timeout = 30
idle_timeout = 600
max_lifetime = 1800

## Engine-specific optimizations

[database.engines.primary_pg.pool]
max_connections = 100  

## Higher for primary database

[database.engines.cache_redis.pool] 
max_connections = 20   

## Lower for cache

```

### Query Performance Monitoring

```json

  "tool": "execute_query",
  "arguments": {
    "sql": "SELECT * FROM large_table WHERE indexed_column = $1",
    "params": ["search_value"],
    "engine": "primary_pg",
    "options": {
      "explain": true,
      "timeout_seconds": 30,
      "log_slow_queries": true
    }
  }
}

```

## 🛠️ Troubleshooting

## Common Issues

### Connection Problems

**Symptom**: "Connection failed" errors
**Solutions**:
1. Check database server status
2. Verify network connectivity
3. Validate credentials and permissions
4. Check SSL/TLS configuration

### Performance Issues  

**Symptom**: Slow query execution
**Solutions**:
1. Enable query execution plans
2. Check database indexes
3. Monitor connection pool status
4. Review query optimization

### Security Violations

**Symptom**: "Security violation detected" errors
**Solutions**:
1. Review audit logs for attack patterns
2. Check user permissions and roles
3. Validate input sanitization
4. Update security policies

## Debug Mode

Enable detailed logging for troubleshooting:

```toml

level = "debug"
include_query_plans = true
log_parameter_values = false  

## Security: don't log sensitive data

audit_all_operations = true

```

## 🔍 Monitoring and Metrics

## Health Check Monitoring

```json

  "tool": "list_engines",
  "arguments": {
    "include_metrics": true
  }
}

```

**Response includes**:
- Connection pool statistics
- Query execution metrics
- Error rates and response times
- Security event counts

## Audit Log Review

All database operations are automatically logged with:
- User identification
- Query/command executed
- Execution time and result status
- Security validation results
- IP address and client information

## 📚 Best Practices

## Security Best Practices

1. **Always use parameterized queries** to prevent SQL injection
2. **Enable MFA** for sensitive database operations
3. **Implement least-privilege access** with RBAC
4. **Regularly rotate encryption keys** and passwords
5. **Monitor audit logs** for suspicious activity

## Performance Best Practices

1. **Use appropriate database engines** for different data types
2. **Implement connection pooling** for high-traffic applications
3. **Cache frequently accessed data** in Redis
4. **Use read replicas** for scaling read operations
5. **Monitor and optimize slow queries**

## Operational Best Practices

1. **Test database configurations** in staging environments
2. **Implement backup and recovery procedures**
3. **Monitor database health metrics**
4. **Plan for capacity scaling**
5. **Document custom configurations and procedures**

---

## Related Documentation

- **[Security Guide](security-guide.md)** - Complete security implementation
- **[Architecture](architecture.md)** - System architecture overview
- **[API Reference](api-reference.md)** - WordPress API documentation

**Version**: 0.15.0  
**Last Updated**: November 7, 2024
