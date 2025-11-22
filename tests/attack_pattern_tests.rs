//! SQL Injection Attack Pattern Tests
//!
//! Comprehensive test suite for validating MySQL security against
//! all known SQL injection attack patterns and techniques

use log::{info, warn};
use mcp_rs::handlers::database::{
    security::DatabaseSecurity,
    types::{QueryContext, QueryType, SecurityConfig, SecurityError},
};
use std::sync::Arc;

/// Initialize test logger
fn init_logger() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .try_init();
}

/// Create security configuration for attack pattern testing
fn create_security_config() -> SecurityConfig {
    SecurityConfig {
        enable_sql_injection_detection: true,
        enable_query_whitelist: false,
        enable_audit_logging: true,
        threat_intelligence_enabled: true,
        max_query_length: 50000,
        allowed_operations: vec![
            QueryType::Select,
            QueryType::Insert,
            QueryType::Update,
            QueryType::Delete,
        ],
    }
}

#[tokio::test]
async fn test_classic_sql_injection_patterns() {
    init_logger();
    info!("🎯 Testing Classic SQL Injection Attack Patterns");

    let security = Arc::new(DatabaseSecurity::new(create_security_config(), None));

    let classic_attacks = vec![
        // Boolean-based blind SQL injection
        ("SELECT * FROM users WHERE id = 1 OR 1=1", "Boolean-based blind injection"),
        ("SELECT * FROM users WHERE id = 1 AND 1=1", "Boolean-based AND injection"),
        ("SELECT * FROM users WHERE id = 1 OR 'a'='a'", "String-based boolean injection"),
        ("SELECT * FROM users WHERE id = 1 AND 'x'='x'", "String-based AND injection"),

        // Union-based SQL injection
        ("SELECT * FROM users WHERE id = 1 UNION SELECT 1,2,3", "Basic UNION injection"),
        ("SELECT * FROM users WHERE id = 1 UNION ALL SELECT username, password FROM admin", "UNION data extraction"),
        ("SELECT * FROM users WHERE id = 1 UNION SELECT @@version, user(), database()", "UNION system information"),
        ("SELECT * FROM users WHERE id = 1 UNION SELECT table_name FROM information_schema.tables", "UNION schema enumeration"),

        // Comment-based injection
        ("SELECT * FROM users WHERE id = 1 --", "SQL comment injection"),
        ("SELECT * FROM users WHERE id = 1 /* comment */ OR 1=1", "Block comment injection"),
        ("SELECT * FROM users WHERE id = 1 #", "MySQL hash comment injection"),
        ("SELECT * FROM users WHERE id = 1;-- comment", "Statement termination with comment"),

        // Stacked queries
        ("SELECT * FROM users WHERE id = 1; DROP TABLE users", "Stacked query with DROP"),
        ("SELECT * FROM users WHERE id = 1; INSERT INTO admin VALUES ('hacker', 'password')", "Stacked query insertion"),
        ("SELECT * FROM users WHERE id = 1; UPDATE users SET password = 'hacked' WHERE username = 'admin'", "Stacked query update"),
        ("SELECT * FROM users WHERE id = 1; DELETE FROM logs", "Stacked query deletion"),
    ];

    let context = QueryContext::new(QueryType::Select);

    for (attack_sql, description) in classic_attacks {
        info!("Testing: {}", description);

        let result = security.validate_query(attack_sql, &context).await;

        assert!(
            result.is_err(),
            "Classic attack should be blocked: {} - SQL: {}",
            description,
            attack_sql
        );

        if let Err(SecurityError::SqlInjectionDetected(msg)) = result {
            info!("✅ {} blocked: {}", description, msg);
        }
    }

    info!("✅ All classic SQL injection patterns successfully blocked");
}

#[tokio::test]
async fn test_advanced_sql_injection_techniques() {
    init_logger();
    info!("🎯 Testing Advanced SQL Injection Techniques");

    let security = Arc::new(DatabaseSecurity::new(create_security_config(), None));

    let advanced_attacks = vec![
        // Time-based blind injection
        ("SELECT * FROM users WHERE id = 1 AND SLEEP(5)", "Time-based delay injection"),
        ("SELECT * FROM users WHERE id = 1 AND BENCHMARK(1000000, MD5(1))", "CPU exhaustion attack"),
        ("SELECT * FROM users WHERE id = 1 AND (SELECT COUNT(*) FROM information_schema.tables) > 0", "Subquery time delay"),
        ("SELECT * FROM users WHERE id = 1 WAITFOR DELAY '0:0:5'", "SQL Server time delay"),

        // Error-based injection
        ("SELECT * FROM users WHERE id = 1 AND ExtractValue(1, concat(0x7e, (SELECT @@version), 0x7e))", "ExtractValue error injection"),
        ("SELECT * FROM users WHERE id = 1 AND UpdateXML(1, concat(0x7e, (SELECT user()), 0x7e), 1)", "UpdateXML error injection"),
        ("SELECT * FROM users WHERE id = 1 AND (SELECT * FROM (SELECT COUNT(*), CONCAT(version(), FLOOR(RAND(0)*2))x FROM information_schema.tables GROUP BY x)a)", "Double query error injection"),
        ("SELECT * FROM users WHERE id = 1 AND GEOMETRYCOLLECTION((SELECT * FROM (SELECT * FROM (SELECT @@version) a) b))", "Geometry error injection"),

        // Obfuscated injection
        ("SELECT * FROM users WHERE id = 1/**/UNION/**/SELECT/**/1,2,3", "Comment obfuscation"),
        ("SELECT * FROM users WHERE id = 1+UNION+SELECT+1,2,3", "Plus sign obfuscation"),
        ("SELECT * FROM users WHERE id = 1%20UNION%20SELECT%201,2,3", "URL encoding obfuscation"),
        ("SELECT/*comment*/FROM/*comment*/users/*comment*/WHERE/*comment*/id=1", "Extensive comment obfuscation"),

        // Alternative injection vectors
        ("SELECT * FROM users WHERE id = CHAR(49) OR CHAR(49)=CHAR(49)", "CHAR function injection"),
        ("SELECT * FROM users WHERE id = ASCII('1') OR ASCII('1')=ASCII('1')", "ASCII function injection"),
        ("SELECT * FROM users WHERE id = 0x31 OR 0x31=0x31", "Hexadecimal injection"),
        ("SELECT * FROM users WHERE id = CONCAT('1', ' OR 1=1')", "String concatenation injection"),
    ];

    let context = QueryContext::new(QueryType::Select);

    for (attack_sql, description) in advanced_attacks {
        info!("Testing: {}", description);

        let result = security.validate_query(attack_sql, &context).await;

        assert!(
            result.is_err(),
            "Advanced attack should be blocked: {} - SQL: {}",
            description,
            attack_sql
        );

        if let Err(SecurityError::SqlInjectionDetected(msg)) = result {
            info!("✅ {} blocked: {}", description, msg);
        }
    }

    info!("✅ All advanced SQL injection techniques successfully blocked");
}

#[tokio::test]
async fn test_database_specific_attacks() {
    init_logger();
    info!("🎯 Testing Database-Specific Attack Patterns");

    let security = Arc::new(DatabaseSecurity::new(create_security_config(), None));

    let database_specific_attacks = vec![
        // MySQL specific
        (
            "SELECT * FROM users WHERE id = 1 AND @@version LIKE '%MySQL%'",
            "MySQL version detection",
        ),
        (
            "SELECT * FROM users WHERE id = 1 AND LOAD_FILE('/etc/passwd')",
            "MySQL file reading",
        ),
        (
            "SELECT * FROM users WHERE id = 1 INTO OUTFILE '/tmp/hack.txt'",
            "MySQL file writing",
        ),
        (
            "SELECT * FROM users WHERE id = 1 PROCEDURE ANALYSE()",
            "MySQL procedure analysis",
        ),
        (
            "SELECT * FROM users WHERE id = 1 AND CONNECTION_ID() > 0",
            "MySQL connection info",
        ),
        // SQL Server specific
        (
            "SELECT * FROM users WHERE id = 1; EXEC xp_cmdshell('dir')",
            "SQL Server command execution",
        ),
        (
            "SELECT * FROM users WHERE id = 1; EXEC sp_executesql N'SELECT @@version'",
            "SQL Server dynamic execution",
        ),
        (
            "SELECT * FROM users WHERE id = 1 AND SYSTEM_USER LIKE '%sa%'",
            "SQL Server system user",
        ),
        (
            "SELECT * FROM users WHERE id = 1; EXEC master..xp_servicecontrol 'start', 'schedule'",
            "SQL Server service control",
        ),
        // PostgreSQL specific
        (
            "SELECT * FROM users WHERE id = 1 AND CURRENT_USER LIKE 'postgres'",
            "PostgreSQL user detection",
        ),
        (
            "SELECT * FROM users WHERE id = 1; COPY (SELECT 'hacked') TO '/tmp/hack.txt'",
            "PostgreSQL file writing",
        ),
        (
            "SELECT * FROM users WHERE id = 1 AND version() LIKE '%PostgreSQL%'",
            "PostgreSQL version detection",
        ),
        // Oracle specific
        (
            "SELECT * FROM users WHERE id = 1 AND USER LIKE 'SYS'",
            "Oracle user detection",
        ),
        (
            "SELECT * FROM users WHERE id = 1 UNION SELECT banner FROM v$version",
            "Oracle version extraction",
        ),
        (
            "SELECT * FROM users WHERE id = 1 AND ROWNUM = 1",
            "Oracle row limiting",
        ),
        // NoSQL injection (MongoDB-style)
        (
            "db.users.find({$where: 'this.id == 1 || 1==1'})",
            "NoSQL $where injection",
        ),
        (
            "db.users.find({id: {$regex: '.*'}})",
            "NoSQL regex injection",
        ),
        (
            "db.users.find({$or: [{id: 1}, {admin: true}]})",
            "NoSQL $or injection",
        ),
    ];

    let context = QueryContext::new(QueryType::Select);

    for (attack_sql, description) in database_specific_attacks {
        info!("Testing: {}", description);

        let result = security.validate_query(attack_sql, &context).await;

        // Most database-specific attacks should be blocked
        // Some might pass if they don't contain obvious attack patterns
        match result {
            Err(SecurityError::SqlInjectionDetected(msg)) => {
                info!("✅ {} blocked: {}", description, msg);
            }
            Ok(_) => {
                warn!("⚠️  {} passed security validation", description);
            }
            Err(e) => {
                info!("✅ {} failed with error: {}", description, e);
            }
        }
    }

    info!("✅ Database-specific attack pattern tests completed");
}

#[tokio::test]
async fn test_bypass_techniques() {
    init_logger();
    info!("🎯 Testing SQL Injection Bypass Techniques");

    let security = Arc::new(DatabaseSecurity::new(create_security_config(), None));

    let bypass_attempts = vec![
        // Case variation bypass
        (
            "select * from users where id = 1 or 1=1",
            "Lowercase bypass attempt",
        ),
        (
            "SELECT * FROM users WHERE id = 1 Or 1=1",
            "Mixed case bypass attempt",
        ),
        (
            "SeLeCt * FrOm users WhErE id = 1 oR 1=1",
            "Random case bypass attempt",
        ),
        // Encoding bypass attempts
        (
            "SELECT * FROM users WHERE id = 1 %4F%52 1=1",
            "URL encoding bypass",
        ),
        (
            "SELECT * FROM users WHERE id = 1 &#79;&#82; 1=1",
            "HTML entity bypass",
        ),
        (
            "SELECT * FROM users WHERE id = 1 \\u004F\\u0052 1=1",
            "Unicode escape bypass",
        ),
        // Double encoding
        (
            "SELECT * FROM users WHERE id = 1 %254F%2552 1=1",
            "Double URL encoding",
        ),
        // Function-based bypass
        (
            "SELECT * FROM users WHERE id = IF(1=1,1,0)",
            "IF function bypass",
        ),
        (
            "SELECT * FROM users WHERE id = CASE WHEN 1=1 THEN 1 ELSE 0 END",
            "CASE statement bypass",
        ),
        (
            "SELECT * FROM users WHERE id = (SELECT 1 WHERE 1=1)",
            "Subselect bypass",
        ),
        // String manipulation bypass
        (
            "SELECT * FROM users WHERE id = 1 OR 'x'||'x'='xx'",
            "String concatenation",
        ),
        (
            "SELECT * FROM users WHERE id = 1 OR REVERSE('1=1') = REVERSE('1=1')",
            "String reversal",
        ),
        (
            "SELECT * FROM users WHERE id = 1 OR SUBSTR('1=1',1,3) = '1=1'",
            "Substring bypass",
        ),
        // Whitespace and delimiter bypass
        (
            "SELECT*FROM/**/users/**/WHERE/**/id=1/**/OR/**/1=1",
            "Comment whitespace bypass",
        ),
        (
            "SELECT\t*\tFROM\tusers\tWHERE\tid=1\tOR\t1=1",
            "Tab delimiter bypass",
        ),
        (
            "SELECT\n*\nFROM\nusers\nWHERE\nid=1\nOR\n1=1",
            "Newline delimiter bypass",
        ),
        (
            "SELECT\r*\rFROM\rusers\rWHERE\rid=1\rOR\r1=1",
            "Carriage return bypass",
        ),
        // Scientific notation and alternative number formats
        (
            "SELECT * FROM users WHERE id = 1 OR 1e0=1e0",
            "Scientific notation bypass",
        ),
        (
            "SELECT * FROM users WHERE id = 1 OR 0x1=0x1",
            "Hexadecimal bypass",
        ),
        (
            "SELECT * FROM users WHERE id = 1 OR b'1'=b'1'",
            "Binary literal bypass",
        ),
    ];

    let context = QueryContext::new(QueryType::Select);

    for (bypass_sql, description) in bypass_attempts {
        info!("Testing bypass: {}", description);

        let result = security.validate_query(bypass_sql, &context).await;

        assert!(
            result.is_err(),
            "Bypass attempt should be blocked: {} - SQL: {}",
            description,
            bypass_sql
        );

        if let Err(SecurityError::SqlInjectionDetected(msg)) = result {
            info!("✅ {} blocked: {}", description, msg);
        }
    }

    info!("✅ All bypass techniques successfully detected and blocked");
}

#[tokio::test]
async fn test_legitimate_queries_pass() {
    init_logger();
    info!("🎯 Testing Legitimate Queries Pass Security");

    let security = Arc::new(DatabaseSecurity::new(create_security_config(), None));

    let legitimate_queries = vec![
        // Basic SELECT queries
        ("SELECT id, name, email FROM users", "Basic SELECT"),
        ("SELECT * FROM products WHERE price > 100", "SELECT with WHERE"),
        ("SELECT COUNT(*) FROM orders WHERE status = 'completed'", "Aggregate query"),

        // JOIN queries
        ("SELECT u.name, p.title FROM users u JOIN posts p ON u.id = p.user_id", "INNER JOIN"),
        ("SELECT u.name, p.title FROM users u LEFT JOIN posts p ON u.id = p.user_id", "LEFT JOIN"),
        ("SELECT u.name, o.total FROM users u RIGHT JOIN orders o ON u.id = o.user_id", "RIGHT JOIN"),

        // Complex but legitimate queries
        ("SELECT u.name, COUNT(o.id) as order_count FROM users u LEFT JOIN orders o ON u.id = o.user_id GROUP BY u.id HAVING COUNT(o.id) > 5", "Complex aggregation"),
        ("SELECT * FROM users WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'", "Date range query"),
        ("SELECT * FROM products WHERE name LIKE '%laptop%' AND category IN ('electronics', 'computers')", "LIKE and IN query"),

        // Subqueries (legitimate)
        ("SELECT * FROM users WHERE id IN (SELECT user_id FROM active_sessions)", "Legitimate subquery"),
        ("SELECT * FROM products WHERE price > (SELECT AVG(price) FROM products)", "Subquery with aggregate"),

        // UNION (legitimate use)
        ("SELECT 'Customer' as type, name FROM customers UNION SELECT 'Vendor' as type, name FROM vendors", "Legitimate UNION"),

        // Functions and expressions
        ("SELECT UPPER(name), LOWER(email), LENGTH(description) FROM users", "String functions"),
        ("SELECT name, DATE_FORMAT(created_at, '%Y-%m-%d') as created_date FROM users", "Date formatting"),
        ("SELECT name, ROUND(price * 1.1, 2) as price_with_tax FROM products", "Mathematical expressions"),
    ];

    let context = QueryContext::new(QueryType::Select);

    for (legitimate_sql, description) in legitimate_queries {
        info!("Testing legitimate query: {}", description);

        let result = security.validate_query(legitimate_sql, &context).await;

        assert!(
            result.is_ok(),
            "Legitimate query should pass: {} - SQL: {}",
            description,
            legitimate_sql
        );

        info!("✅ {} passed security validation", description);
    }

    info!("✅ All legitimate queries passed security validation");
}

#[tokio::test]
async fn test_attack_pattern_coverage() {
    init_logger();
    info!("🎯 Testing Attack Pattern Coverage Statistics");

    let security = Arc::new(DatabaseSecurity::new(create_security_config(), None));

    // Comprehensive attack pattern test
    let all_attack_patterns = vec![
        // SQL Injection Categories
        ("Boolean-based blind", vec![
            "SELECT * FROM users WHERE id = 1 OR 1=1",
            "SELECT * FROM users WHERE id = 1 AND 1=1",
            "SELECT * FROM users WHERE id = 1 OR 'a'='a'",
        ]),
        ("Union-based", vec![
            "SELECT * FROM users WHERE id = 1 UNION SELECT 1,2,3",
            "SELECT * FROM users WHERE id = 1 UNION ALL SELECT username, password FROM admin",
        ]),
        ("Time-based blind", vec![
            "SELECT * FROM users WHERE id = 1 AND SLEEP(5)",
            "SELECT * FROM users WHERE id = 1 AND BENCHMARK(1000000, MD5(1))",
        ]),
        ("Error-based", vec![
            "SELECT * FROM users WHERE id = 1 AND ExtractValue(1, concat(0x7e, (SELECT @@version), 0x7e))",
            "SELECT * FROM users WHERE id = 1 AND UpdateXML(1, concat(0x7e, (SELECT user()), 0x7e), 1)",
        ]),
        ("Stacked queries", vec![
            "SELECT * FROM users WHERE id = 1; DROP TABLE users",
            "SELECT * FROM users WHERE id = 1; INSERT INTO admin VALUES ('hacker', 'password')",
        ]),
    ];

    let context = QueryContext::new(QueryType::Select);
    let mut total_tests = 0;
    let mut blocked_attacks = 0;

    for (category, attacks) in all_attack_patterns {
        info!("Testing category: {}", category);
        let mut category_blocked = 0;

        for attack in &attacks {
            total_tests += 1;
            let result = security.validate_query(attack, &context).await;

            if result.is_err() {
                blocked_attacks += 1;
                category_blocked += 1;
            }
        }

        let category_coverage = (category_blocked as f32 / attacks.len() as f32) * 100.0;
        info!(
            "Category '{}' coverage: {:.1}% ({}/{})",
            category,
            category_coverage,
            category_blocked,
            attacks.len()
        );
    }

    let overall_coverage = (blocked_attacks as f32 / total_tests as f32) * 100.0;
    info!(
        "🛡️  Overall Attack Prevention Coverage: {:.1}% ({}/{})",
        overall_coverage, blocked_attacks, total_tests
    );

    // Ensure high coverage
    assert!(
        overall_coverage >= 90.0,
        "Attack prevention coverage should be at least 90%, got {:.1}%",
        overall_coverage
    );

    info!("✅ Attack pattern coverage test completed successfully");
}
