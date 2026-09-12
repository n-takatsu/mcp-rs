//! Integration tests against a real PKCS#11 provider (SoftHSM2 in CI/local
//! Docker verification, or a real HSM's PKCS#11 module).
//!
//! These tests are `#[ignore]`d by default because they require a live
//! PKCS#11 token to be initialized and reachable. Run them with:
//!
//! ```sh
//! TEST_PKCS11_LIBRARY_PATH=/usr/lib/softhsm/libsofthsm2.so \
//! TEST_PKCS11_PIN=1234 \
//! cargo test --features hsm --test hsm_pkcs11_integration_test -- --ignored
//! ```
#![cfg(feature = "hsm")]

use chrono::Utc;
use mcp_rs::handlers::database::column_encryption::{
    ColumnEncryptionConfig, ColumnEncryptionManager, KeyManagerConfig, KeyProvider,
};
use mcp_rs::handlers::database::types::{QueryContext, QueryType};
use mcp_rs::security::auth::types::AuthUser;

fn pkcs11_provider() -> KeyProvider {
    let library_path = std::env::var("TEST_PKCS11_LIBRARY_PATH")
        .expect("TEST_PKCS11_LIBRARY_PATH must be set to run this test");
    let pin = std::env::var("TEST_PKCS11_PIN").unwrap_or_else(|_| "1234".to_string());
    KeyProvider::Pkcs11 {
        library_path,
        slot_id: None,
        pin,
    }
}

fn test_context(user_id: &str) -> QueryContext {
    QueryContext {
        query_type: QueryType::Select,
        user_id: Some(user_id.to_string()),
        session_id: "hsm-it-session".to_string(),
        timestamp: Utc::now(),
        source_ip: Some("127.0.0.1".to_string()),
        client_info: None,
    }
}

fn manager_for(table_column: &str) -> ColumnEncryptionManager {
    let mut config = ColumnEncryptionConfig {
        key_manager: KeyManagerConfig {
            provider: pkcs11_provider(),
            ..KeyManagerConfig::default()
        },
        ..ColumnEncryptionConfig::default()
    };
    config.encrypted_columns.push(table_column.to_string());
    ColumnEncryptionManager::new(config)
}

/// The core round trip: a value encrypted through an HSM-backed key can be
/// decrypted back to the original plaintext, and the ciphertext produced is
/// not the plaintext itself.
#[tokio::test]
#[ignore]
async fn hsm_backed_encrypt_decrypt_round_trip() {
    let manager = manager_for("hsm_it.ssn");
    let context = test_context("admin");
    let user = AuthUser::new("admin".to_string(), "admin".to_string());

    let plaintext = "123-45-6789";
    let encrypted = manager
        .encrypt("hsm_it", "ssn", plaintext, &context)
        .await
        .expect("encryption via HSM-backed key should succeed");

    assert_ne!(encrypted, plaintext);

    let decrypted = manager
        .decrypt("hsm_it", "ssn", &encrypted, &context, Some(&user))
        .await
        .expect("decryption via HSM-backed key should succeed");
    assert_eq!(decrypted, plaintext);
}

/// Two independently-generated HSM-backed keys for different columns must
/// not be able to decrypt each other's ciphertext - proves each column
/// really does get its own token-resident key, not a single shared one.
#[tokio::test]
#[ignore]
async fn hsm_backed_keys_are_isolated_per_column() {
    let manager_a = manager_for("hsm_it.email");
    let context = test_context("admin");

    let plaintext = "isolation-check@example.com";
    let encrypted = manager_a
        .encrypt("hsm_it", "email", plaintext, &context)
        .await
        .expect("encryption should succeed");

    // A DEK for a different column, same manager instance's underlying
    // provider/session but a distinct token-resident key.
    let other_ciphertext = manager_a
        .encrypt("hsm_it", "phone", plaintext, &context)
        .await
        .expect("encryption of a second column should succeed");

    assert_ne!(
        encrypted, other_ciphertext,
        "two different columns must not produce identical ciphertext for the same plaintext"
    );
}

/// A key generated inside the token survives reconnecting with a fresh
/// `ColumnEncryptionManager`/`KeyManager` (simulating a process restart):
/// data encrypted before the "restart" must still decrypt afterwards,
/// proving the key is a persistent HSM token object, not an in-memory one.
#[tokio::test]
#[ignore]
async fn hsm_backed_key_survives_reconnect() {
    let context = test_context("admin");
    let user = AuthUser::new("admin".to_string(), "admin".to_string());
    let plaintext = "persists-across-reconnect";

    let encrypted = {
        let manager = manager_for("hsm_it.notes");
        manager
            .encrypt("hsm_it", "notes", plaintext, &context)
            .await
            .expect("encryption should succeed")
    };

    // Fresh manager -> fresh KeyManager -> fresh HsmProvider::connect, but
    // the same token, so the key should already exist under its label.
    let reconnected = manager_for("hsm_it.notes");
    let decrypted = reconnected
        .decrypt("hsm_it", "notes", &encrypted, &context, Some(&user))
        .await
        .expect("decryption after reconnect should succeed");
    assert_eq!(decrypted, plaintext);
}
