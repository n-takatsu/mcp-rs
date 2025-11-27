# Role-Based Access Control (RBAC) Guide

## Overview

The RBAC system in mcp-rs provides comprehensive, multi-layered access control for database operations. It supports role hierarchy, time-based restrictions, IP filtering, column-level permissions, row-level security, and data masking.

## Table of Contents

1. [Architecture](#architecture)
2. [Core Concepts](#core-concepts)
3. [Configuration](#configuration)
4. [Access Control Layers](#access-control-layers)
5. [Usage Examples](#usage-examples)
6. [Best Practices](#best-practices)
7. [API Reference](#api-reference)

## Architecture

The RBAC system consists of the following components:

```
┌─────────────────────────────────────────────────────────────┐
│           RoleBasedAccessControl (Main Component)           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │ Role         │  │ User Role    │  │ Permission   │     │
│  │ Hierarchy    │  │ Mapping      │  │ Cache        │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
│                                                             │
│  ┌──────────────────────────────────────────────────┐     │
│  │            Access Control Policies                │     │
│  │  • Condition Evaluation                          │     │
│  │  • Time-Based Access Control                     │     │
│  │  • IP Restrictions                               │     │
│  │  • Column-Level Permissions                      │     │
│  │  • Row-Level Security                            │     │
│  │  • Data Masking                                  │     │
│  └──────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────┘
```

## Core Concepts

### Roles

A **Role** represents a set of permissions granted to users. Roles support inheritance through the role hierarchy.

```rust
pub struct Role {
    pub name: String,
    pub description: String,
    pub permissions: HashMap<String, Vec<ActionType>>,
    pub conditions: Vec<AccessCondition>,
}
```

**Key Features:**
- **Hierarchical**: Roles can inherit permissions from parent roles
- **Conditional**: Permissions can be conditional (time, IP, attributes)
- **Resource-Specific**: Permissions are mapped to resources (tables, columns)

### Action Types

Operations are categorized into these action types:

- **Read**: SELECT queries
- **Write**: INSERT, UPDATE queries
- **Delete**: DELETE queries
- **Admin**: DDL operations (CREATE, ALTER, DROP)
- **Execute**: Stored procedures, transactions

### Access Conditions

Conditions allow fine-grained control over when permissions apply:

| Condition Type | Description | Example |
|---------------|-------------|---------|
| `TimeOfDay` | Time-based access | Business hours only |
| `DayOfWeek` | Day-based access | Weekdays only |
| `IpAddress` | IP-based access | Corporate network only |
| `UserAttribute` | User property check | Department == "Finance" |
| `DataSensitivity` | Data classification | Sensitivity level <= 3 |
| `QueryComplexity` | Query cost limits | Complexity < 100 |

**Supported Operators:**
- `Equals`, `NotEquals`
- `Contains`, `NotContains`
- `GreaterThan`, `LessThan`
- `Between`, `In`, `NotIn`

## Configuration

### Basic RBAC Configuration

```rust
use mcp_rs::handlers::database::advanced_security::*;
use mcp_rs::handlers::database::security_config::*;

let rbac_config = RbacConfig {
    enabled: true,
    role_hierarchy: HashMap::from([
        ("admin".to_string(), vec![]),
        ("developer".to_string(), vec!["read_only".to_string()]),
        ("analyst".to_string(), vec!["read_only".to_string()]),
        ("read_only".to_string(), vec![]),
    ]),
    default_role: Some("read_only".to_string()),
    roles: HashMap::from([
        ("admin".to_string(), Role {
            name: "admin".to_string(),
            description: "Full administrative access".to_string(),
            permissions: HashMap::from([
                ("*".to_string(), vec![
                    ActionType::Read,
                    ActionType::Write,
                    ActionType::Delete,
                    ActionType::Admin,
                ]),
            ]),
            conditions: vec![],
        }),
        // ... more roles
    ]),
    // ... other configurations
};

let rbac = RoleBasedAccessControl::new(rbac_config);
```

### Time-Based Access Control

```rust
use chrono::NaiveTime;

let time_based_access = TimeBasedAccess {
    enabled: true,
    business_hours: HashMap::from([
        (Weekday::Monday, vec![
            TimeRange {
                start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            },
        ]),
        // ... other days
    ]),
    emergency_access: Some(EmergencyAccessConfig {
        enabled: true,
        roles: vec!["admin".to_string(), "oncall".to_string()],
        requires_mfa: true,
        audit_level: "critical".to_string(),
    }),
    timezone: "UTC".to_string(),
};
```

### IP Restrictions

```rust
use ipnet::IpNet;
use std::str::FromStr;

let ip_restrictions = IpRestrictions {
    enabled: true,
    allowed_ranges: vec![
        IpNet::from_str("10.0.0.0/8").unwrap(),      // Corporate network
        IpNet::from_str("192.168.1.0/24").unwrap(),  // Office subnet
    ],
    denied_ranges: vec![
        IpNet::from_str("192.168.1.100/32").unwrap(), // Blocked IP
    ],
    role_based_restrictions: HashMap::from([
        ("developer".to_string(), IpRoleRestriction {
            allowed_ranges: vec![
                IpNet::from_str("10.0.0.0/8").unwrap(),
            ],
            require_vpn: true,
        }),
    ]),
};
```

### Column-Level Permissions

```rust
let column_permissions = HashMap::from([
    ("users.email".to_string(), ColumnPermission {
        read_roles: vec!["admin".to_string(), "support".to_string()],
        write_roles: vec!["admin".to_string()],
        masking_rules: vec![
            MaskingRule {
                masking_type: MaskingType::Partial(PartialMaskConfig {
                    reveal_first: 3,
                    reveal_last: 0,
                    mask_char: '*',
                }),
                applies_to_roles: vec!["support".to_string()],
            },
        ],
        encryption_required: false,
    }),
    ("users.ssn".to_string(), ColumnPermission {
        read_roles: vec!["admin".to_string()],
        write_roles: vec!["admin".to_string()],
        masking_rules: vec![
            MaskingRule {
                masking_type: MaskingType::Hash,
                applies_to_roles: vec!["analyst".to_string()],
            },
        ],
        encryption_required: true,
    }),
]);
```

### Row-Level Security

```rust
let row_level_security = RowLevelSecurityConfig {
    enabled: true,
    policies: HashMap::from([
        ("users".to_string(), RowSecurityPolicy {
            policy_column: "owner_id".to_string(),
            user_attribute: "user_id".to_string(),
            allow_admin_bypass: true,
        }),
        ("documents".to_string(), RowSecurityPolicy {
            policy_column: "department_id".to_string(),
            user_attribute: "department".to_string(),
            allow_admin_bypass: true,
        }),
    ]),
};
```

## Access Control Layers

### Layer 1: Role-Based Permissions

Basic permission check based on user roles and resource access.

```rust
// Check if user has permission
let decision = rbac.check_access(
    "user_id",
    "users",
    &ActionType::Read,
    &context
).await?;
```

### Layer 2: Conditional Access

Permissions can be conditional based on time, IP, or user attributes.

```rust
let condition = AccessCondition {
    condition_type: ConditionType::TimeOfDay,
    operator: ConditionOperator::Between,
    value: ConditionValue::TimeRange {
        start: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        end: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
    },
};
```

### Layer 3: Time-Based Control

Access is restricted to business hours or emergency access scenarios.

```rust
// Business hours are automatically checked
let is_business_hours = rbac.check_business_hours(&context).await?;

// Emergency access requires MFA
if emergency_access && !mfa_verified {
    return AccessDecision::Deny;
}
```

### Layer 4: IP Restrictions

Access is restricted based on source IP address.

```rust
// IP address is checked against allowed/denied ranges
let client_ip = context.get("client_ip");
let is_allowed = rbac.check_ip_restrictions(client_ip).await?;
```

### Layer 5: Column-Level Permissions

Access to specific columns can be restricted or masked.

```rust
// Check column read access
let can_read = rbac.check_column_access(
    &user_roles,
    "users",
    "email",
    &ActionType::Read
).await?;

// Apply data masking
let masked_value = rbac.apply_data_masking(
    &user_roles,
    "users",
    "ssn",
    "123-45-6789"
).await?;
```

### Layer 6: Row-Level Security

Access to specific rows based on ownership or attributes.

```rust
// Check row access
let row_data = HashMap::from([
    ("owner_id".to_string(), "user123".to_string()),
]);

let can_access = rbac.check_row_level_security(
    &user_roles,
    "users",
    &row_data,
    &context
).await?;
```

## Usage Examples

### Example 1: Basic Role Assignment

```rust
use mcp_rs::handlers::database::integrated_security::*;

let security_manager = IntegratedSecurityManager::new(config);

// Assign role to user
security_manager.assign_user_role(
    "user123",
    "developer"
).await?;

// Check access
let context = QueryContext::new();
context.insert("user_id", "user123");
context.insert("client_ip", "10.0.1.50");

let decision = security_manager.check_authentication_and_authorization(
    "user123",
    "SELECT * FROM users",
    &context
).await?;
```

### Example 2: Time-Based Access

```rust
// Configure analyst role with business hours restriction
let analyst_role = Role {
    name: "analyst".to_string(),
    description: "Data analyst with business hours access".to_string(),
    permissions: HashMap::from([
        ("analytics_db.*".to_string(), vec![ActionType::Read]),
    ]),
    conditions: vec![
        AccessCondition {
            condition_type: ConditionType::TimeOfDay,
            operator: ConditionOperator::Between,
            value: ConditionValue::TimeRange {
                start: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
                end: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
            },
        },
    ],
};

// Access is automatically denied outside business hours
```

### Example 3: Column-Level Data Masking

```rust
// Configure email masking for support role
let email_permission = ColumnPermission {
    read_roles: vec!["admin".to_string(), "support".to_string()],
    write_roles: vec!["admin".to_string()],
    masking_rules: vec![
        MaskingRule {
            masking_type: MaskingType::Partial(PartialMaskConfig {
                reveal_first: 3,
                reveal_last: 0,
                mask_char: '*',
            }),
            applies_to_roles: vec!["support".to_string()],
        },
    ],
    encryption_required: false,
};

// Support user sees: "joh***************"
// Admin sees: "john.doe@example.com"
let masked = rbac.apply_data_masking(
    &vec!["support".to_string()],
    "users",
    "email",
    "john.doe@example.com"
).await?;
```

### Example 4: Row-Level Security

```rust
// Configure row-level security for multi-tenant app
let rls_config = RowSecurityPolicy {
    policy_column: "tenant_id".to_string(),
    user_attribute: "tenant_id".to_string(),
    allow_admin_bypass: true,
};

// User can only access rows where tenant_id matches their attribute
let row_data = HashMap::from([
    ("tenant_id".to_string(), "tenant_123".to_string()),
    ("name".to_string(), "John Doe".to_string()),
]);

let context = QueryContext::new();
context.insert("user_id", "user123");
context.insert("tenant_id", "tenant_123"); // Must match

let can_access = rbac.check_row_level_security(
    &user_roles,
    "users",
    &row_data,
    &context
).await?;
```

## Best Practices

### 1. Role Design

- **Principle of Least Privilege**: Grant minimum required permissions
- **Role Hierarchy**: Use inheritance to avoid duplication
- **Naming Convention**: Use clear, descriptive role names (e.g., `finance_analyst`, `hr_manager`)

```rust
// Good: Clear hierarchy
role_hierarchy: HashMap::from([
    ("senior_developer", vec!["developer"]),
    ("developer", vec!["read_only"]),
    ("read_only", vec![]),
])

// Bad: Flat structure with duplicated permissions
```

### 2. Time-Based Access

- **Emergency Access**: Always configure emergency access for critical situations
- **Timezone**: Use UTC for consistency, convert in application layer
- **Break Periods**: Configure lunch breaks to prevent unauthorized access

```rust
// Configure emergency access with MFA
emergency_access: Some(EmergencyAccessConfig {
    enabled: true,
    roles: vec!["admin", "oncall"],
    requires_mfa: true,
    audit_level: "critical",
})
```

### 3. IP Restrictions

- **Allowlist Approach**: Use allowed_ranges by default
- **VPN Requirements**: Require VPN for remote access
- **Dynamic Updates**: Implement IP range updates for cloud environments

```rust
// Require VPN for developer access
role_based_restrictions: HashMap::from([
    ("developer", IpRoleRestriction {
        allowed_ranges: vec![corporate_network],
        require_vpn: true,
    }),
])
```

### 4. Data Masking

- **Sensitivity Levels**: Apply appropriate masking based on data sensitivity
- **Role-Based Masking**: Different roles see different masking levels
- **PII/PHI Protection**: Always mask personal identifiable information

```rust
// Progressive masking based on role
masking_rules: vec![
    MaskingRule {
        masking_type: MaskingType::Hash,
        applies_to_roles: vec!["analyst"],  // Most restrictive
    },
    MaskingRule {
        masking_type: MaskingType::Partial(...),
        applies_to_roles: vec!["support"],  // Partial visibility
    },
    // admin role: no masking (full access)
]
```

### 5. Row-Level Security

- **Multi-Tenancy**: Essential for SaaS applications
- **Admin Bypass**: Allow admin bypass with audit logging
- **Attribute Mapping**: Use consistent attribute names

```rust
// Multi-tenant configuration
RowSecurityPolicy {
    policy_column: "tenant_id",
    user_attribute: "tenant_id",
    allow_admin_bypass: true,
}
```

### 6. Performance Optimization

- **Permission Caching**: Enabled by default, configure TTL appropriately
- **Batch Operations**: Use bulk role assignments for user imports
- **Index Policy Columns**: Ensure row-level security columns are indexed

```rust
// Configure cache with appropriate TTL
permission_cache: HashMap::new(), // Auto-cleaned based on TTL
```

### 7. Security Monitoring

- **Audit Logging**: Log all access decisions (especially denials)
- **Alert on Anomalies**: Monitor for unusual access patterns
- **Regular Review**: Periodically review and update role permissions

```rust
// Always log access denials
if let AccessDecision::Deny = decision {
    audit_logger.log(SecurityEvent {
        event_type: "access_denied",
        user_id: user_id,
        resource: resource,
        timestamp: Utc::now(),
    });
}
```

## API Reference

### RoleBasedAccessControl

#### Constructor

```rust
pub fn new(config: RbacConfig) -> Self
```

Creates a new RBAC instance with the provided configuration.

#### Core Methods

```rust
pub async fn check_access(
    &self,
    user_id: &str,
    resource: &str,
    action: &ActionType,
    context: &QueryContext
) -> Result<AccessDecision, SecurityError>
```

Checks if a user has permission to perform an action on a resource.

**Parameters:**
- `user_id`: User identifier
- `resource`: Resource name (e.g., table name)
- `action`: Action type (Read, Write, Delete, Admin, Execute)
- `context`: Query context with metadata (IP, time, attributes)

**Returns:** `AccessDecision::Allow` or `AccessDecision::Deny`

```rust
pub async fn assign_role(
    &self,
    user_id: &str,
    role: &str
) -> Result<(), SecurityError>
```

Assigns a role to a user.

```rust
pub async fn revoke_role(
    &self,
    user_id: &str,
    role: &str
) -> Result<(), SecurityError>
```

Revokes a role from a user.

```rust
pub async fn get_user_roles(
    &self,
    user_id: &str
) -> Vec<String>
```

Gets all roles assigned to a user (including inherited roles).

#### Column-Level Methods

```rust
pub async fn check_column_access(
    &self,
    user_roles: &[String],
    table: &str,
    column: &str,
    action: &ActionType
) -> Result<bool, SecurityError>
```

Checks if user roles have permission to access a specific column.

```rust
pub async fn apply_data_masking(
    &self,
    user_roles: &[String],
    table: &str,
    column: &str,
    value: &str
) -> Result<String, SecurityError>
```

Applies data masking to a column value based on user roles.

**Masking Types:**
- `Full`: Returns `"***"`
- `Partial`: Reveals configured characters
- `Hash`: Returns SHA-256 hash
- `Tokenize`: Returns random token

#### Row-Level Methods

```rust
pub async fn check_row_level_security(
    &self,
    user_roles: &[String],
    table: &str,
    row_data: &HashMap<String, String>,
    context: &QueryContext
) -> Result<bool, SecurityError>
```

Checks if user can access a specific row based on row-level security policies.

### IntegratedSecurityManager

#### RBAC Integration Methods

```rust
pub async fn assign_user_role(
    &self,
    user_id: &str,
    role: &str
) -> Result<(), SecurityError>
```

Public API to assign a role to a user.

```rust
pub async fn revoke_user_role(
    &self,
    user_id: &str,
    role: &str
) -> Result<(), SecurityError>
```

Public API to revoke a role from a user.

```rust
pub async fn update_rbac_config(
    &mut self,
    config: RbacConfig
) -> Result<(), SecurityError>
```

Updates the RBAC configuration at runtime.

```rust
pub async fn check_column_access(
    &self,
    user_id: &str,
    table: &str,
    column: &str,
    action: &ActionType
) -> Result<bool, SecurityError>
```

Checks column access for a user.

```rust
pub async fn check_row_level_security(
    &self,
    user_id: &str,
    table: &str,
    row_data: &HashMap<String, String>,
    context: &QueryContext
) -> Result<bool, SecurityError>
```

Checks row-level access for a user.

```rust
pub async fn apply_data_masking(
    &self,
    user_id: &str,
    table: &str,
    column: &str,
    value: &str
) -> Result<String, SecurityError>
```

Applies data masking for a user on a specific column value.

---

## Additional Resources

- [Security Best Practices](../SECURITY.md)
- [Configuration Examples](../configs/examples/)
- [Integration Guide](../docs/implementation/)
- [API Documentation](https://docs.rs/mcp-rs)

## Support

For issues or questions about RBAC:
- GitHub Issues: https://github.com/yourusername/mcp-rs/issues
- Documentation: https://github.com/yourusername/mcp-rs/tree/main/docs

## License

This documentation is part of the mcp-rs project and is licensed under MIT OR Apache-2.0.
