//! Column-Level Encryption Demo
//!
//! Demonstrates the use of column-level encryption with key management

use chrono::Utc;
use mcp_rs::handlers::database::column_encryption::{
    ColumnEncryptionConfig, ColumnEncryptionManager, EncryptionAlgorithm, KeyManagerConfig,
    KeyProvider,
};
use mcp_rs::handlers::database::types::{QueryContext, QueryType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Column-Level Encryption Demo ===\n");

    // Create encryption configuration
    let mut config = ColumnEncryptionConfig {
        key_manager: KeyManagerConfig {
            provider: KeyProvider::Local {
                key_path: "./demo_keys".to_string(),
            },
            rotation_interval_secs: 86400 * 30, // 30 days
            max_old_keys: 10,
            default_algorithm: EncryptionAlgorithm::Aes256Gcm,
            enable_hw_acceleration: true,
        },
        cache_ttl_secs: 300,
        max_cache_size: 1000,
        enable_batch_operations: true,
        encrypted_columns: vec![
            "users.email".to_string(),
            "users.ssn".to_string(),
            "customers.credit_card".to_string(),
        ],
    };

    // Add encrypted columns
    config
        .encrypted_columns
        .push("users.password_hash".to_string());

    println!("📋 Configuration:");
    println!("  Algorithm: {:?}", config.key_manager.default_algorithm);
    println!("  Cache TTL: {} seconds", config.cache_ttl_secs);
    println!("  Max Cache Size: {}", config.max_cache_size);
    println!("  Encrypted Columns: {:?}\n", config.encrypted_columns);

    // Create encryption manager
    let manager = ColumnEncryptionManager::new(config);

    // Create query context
    let context = QueryContext {
        query_type: QueryType::Select,
        user_id: Some("admin@example.com".to_string()),
        session_id: "demo-session-001".to_string(),
        timestamp: Utc::now(),
        source_ip: Some("192.168.1.100".to_string()),
        client_info: Some("Demo Client v1.0".to_string()),
    };

    println!("👤 User Context:");
    println!("  User ID: {}", context.user_id.as_ref().unwrap());
    println!("  Session ID: {}", context.session_id);
    println!("  Source IP: {}\n", context.source_ip.as_ref().unwrap());

    // Example 1: Encrypt and decrypt email
    println!("=== Example 1: Email Encryption ===");
    let email = "john.doe@example.com";
    println!("Original email: {}", email);

    let encrypted_email = manager.encrypt("users", "email", email, &context).await?;
    println!(
        "Encrypted: {} ({}  bytes)",
        &encrypted_email[..50.min(encrypted_email.len())],
        encrypted_email.len()
    );

    let decrypted_email = manager
        .decrypt("users", "email", &encrypted_email, &context)
        .await?;
    println!("Decrypted: {}", decrypted_email);
    assert_eq!(email, decrypted_email);
    println!("✅ Email encryption/decryption successful!\n");

    // Example 2: Encrypt SSN
    println!("=== Example 2: SSN Encryption ===");
    let ssn = "123-45-6789";
    println!("Original SSN: {}", ssn);

    let encrypted_ssn = manager.encrypt("users", "ssn", ssn, &context).await?;
    println!(
        "Encrypted: {} (truncated)",
        &encrypted_ssn[..50.min(encrypted_ssn.len())]
    );

    let decrypted_ssn = manager
        .decrypt("users", "ssn", &encrypted_ssn, &context)
        .await?;
    println!("Decrypted: {}", decrypted_ssn);
    assert_eq!(ssn, decrypted_ssn);
    println!("✅ SSN encryption/decryption successful!\n");

    // Example 3: Cache demonstration
    println!("=== Example 3: Cache Performance ===");
    let test_email = "test@example.com";

    use std::time::Instant;
    let start = Instant::now();
    let _ = manager
        .encrypt("users", "email", test_email, &context)
        .await?;
    let first_duration = start.elapsed();
    println!("First encryption: {:?}", first_duration);

    let start = Instant::now();
    let _ = manager
        .encrypt("users", "email", test_email, &context)
        .await?;
    let cached_duration = start.elapsed();
    println!("Cached encryption: {:?}", cached_duration);
    println!(
        "Speedup: {:.2}x\n",
        first_duration.as_nanos() as f64 / cached_duration.as_nanos() as f64
    );

    // Example 4: Multiple values
    println!("=== Example 4: Batch Encryption ===");
    let values = [
        "alice@example.com",
        "bob@example.com",
        "charlie@example.com",
    ];

    for (i, value) in values.iter().enumerate() {
        let encrypted = manager.encrypt("users", "email", value, &context).await?;
        println!(
            "{}. Encrypted {} -> {} bytes",
            i + 1,
            value,
            encrypted.len()
        );
    }
    println!();

    // Example 5: Key rotation
    println!("=== Example 5: Key Rotation ===");
    let original_data = "sensitive-data-before-rotation";
    let encrypted_v1 = manager
        .encrypt("customers", "credit_card", original_data, &context)
        .await?;
    println!("Encrypted with key v1: {} bytes", encrypted_v1.len());

    // Rotate key
    let new_key_id = manager
        .rotate_column_key("customers", "credit_card")
        .await?;
    println!("Rotated to new key: {}", new_key_id);

    // Old data should still decrypt
    let decrypted_old = manager
        .decrypt("customers", "credit_card", &encrypted_v1, &context)
        .await?;
    println!(
        "Old data still decrypts: {}",
        decrypted_old == original_data
    );

    // New data uses new key
    let encrypted_v2 = manager
        .encrypt(
            "customers",
            "credit_card",
            "new-data-after-rotation",
            &context,
        )
        .await?;
    println!("Encrypted with key v2: {} bytes", encrypted_v2.len());
    println!("✅ Key rotation successful!\n");

    // Example 6: Cache statistics
    println!("=== Example 6: Cache Statistics ===");
    let stats = manager.get_cache_stats().await;
    println!("Encryption cache: {} entries", stats.encryption_cache_size);
    println!("Decryption cache: {} entries", stats.decryption_cache_size);
    println!("Max cache size: {}", stats.max_cache_size);
    println!("Cache TTL: {} seconds\n", stats.cache_ttl_secs);

    // Example 7: Key metadata
    println!("=== Example 7: Key Metadata ===");
    let keys = manager.key_manager().list_keys().await;
    println!("Total keys: {}", keys.len());
    for key in keys.iter().take(5) {
        println!(
            "  - {} (v{}) - Active: {}",
            key.key_id, key.version, key.is_active
        );
    }
    println!();

    // Example 8: Permission check (unauthorized user)
    println!("=== Example 8: Permission Check ===");
    let mut unauthorized_context = context.clone();
    unauthorized_context.user_id = None;

    let encrypted_sensitive = manager
        .encrypt("users", "ssn", "111-22-3333", &context)
        .await?;
    let result = manager
        .decrypt("users", "ssn", &encrypted_sensitive, &unauthorized_context)
        .await?;
    println!("Unauthorized user sees: {}", result);
    assert_eq!(result, "***ENCRYPTED***");
    println!("✅ Permission check working!\n");

    println!("=== Demo Complete ===");
    println!("All examples executed successfully! 🎉");

    Ok(())
}
